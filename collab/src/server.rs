//! Transport-agnostic collaboration sync hub.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! The [`SyncHub`] owns rooms, each holding an authoritative [`CrdtDoc`],
//! membership with access levels, and live presence. It is *transport-agnostic*:
//! callers feed it [`ClientMessage`]s tagged with a connection id and receive a
//! list of [`Outbound`] messages to deliver. A production WebSocket server is a
//! thin adapter that (de)serializes frames and routes `Outbound`s to sockets.
//!
//! Responsibilities covered here:
//! - **Authentication**: a pluggable [`Authenticator`] validates join tokens.
//! - **Access control**: viewer/editor/owner; edits from viewers are rejected.
//! - **Real-time propagation**: ops/presence broadcast to other members.
//! - **Offline support**: on reconnect a client sends `Resync`/`Join` and gets a
//!   full [`CrdtDoc`] snapshot; because state is a CRDT, buffered offline ops it
//!   later sends merge cleanly regardless of order.

use std::collections::HashMap;

use crate::clock::ReplicaId;
use crate::crdt::{CrdtDoc, Op};
use crate::presence::{Presence, PresenceMap};
use crate::protocol::{AccessLevel, ClientMessage, ServerMessage};

/// Opaque per-connection identifier assigned by the transport.
pub type ConnId = u64;

/// Validates join tokens and resolves the member identity + default access.
pub trait Authenticator {
    /// Return `Some((subject, level))` if the token is valid for `room`.
    fn authenticate(&self, room: &str, token: &str) -> Option<(String, AccessLevel)>;
}

/// A simple authenticator backed by an in-memory table of
/// `token -> (subject, per-room grants)`. Suitable for tests and single-node
/// deployments; production would validate signed JWTs and consult a store.
#[derive(Debug, Default)]
pub struct MemoryAuth {
    /// token -> subject
    tokens: HashMap<String, String>,
    /// (room, subject) -> level
    grants: HashMap<(String, String), AccessLevel>,
    /// Access for authenticated members without an explicit grant.
    default_level: Option<AccessLevel>,
}

impl MemoryAuth {
    pub fn new() -> Self {
        MemoryAuth::default()
    }

    /// Register a token for a subject.
    pub fn add_token(&mut self, token: &str, subject: &str) {
        self.tokens.insert(token.to_string(), subject.to_string());
    }

    /// Grant a subject an access level in a room.
    pub fn grant(&mut self, room: &str, subject: &str, level: AccessLevel) {
        self.grants
            .insert((room.to_string(), subject.to_string()), level);
    }

    /// Access level applied to authenticated members lacking an explicit grant.
    pub fn set_default_level(&mut self, level: Option<AccessLevel>) {
        self.default_level = level;
    }
}

impl Authenticator for MemoryAuth {
    fn authenticate(&self, room: &str, token: &str) -> Option<(String, AccessLevel)> {
        let subject = self.tokens.get(token)?.clone();
        let level = self
            .grants
            .get(&(room.to_string(), subject.clone()))
            .copied()
            .or(self.default_level)?;
        Some((subject, level))
    }
}

/// A message the hub wants delivered to a specific connection.
#[derive(Debug, Clone)]
pub struct Outbound {
    pub to: ConnId,
    pub message: ServerMessage,
}

struct Member {
    conn: ConnId,
    replica: ReplicaId,
    subject: String,
    level: AccessLevel,
}

#[derive(Default)]
struct Room {
    doc: CrdtDoc,
    presence: PresenceMap,
    members: Vec<Member>,
}

impl Room {
    fn other_conns(&self, except: ConnId) -> Vec<ConnId> {
        self.members
            .iter()
            .filter(|m| m.conn != except)
            .map(|m| m.conn)
            .collect()
    }

    fn member(&self, conn: ConnId) -> Option<&Member> {
        self.members.iter().find(|m| m.conn == conn)
    }
}

/// The sync hub. Generic over the [`Authenticator`].
pub struct SyncHub<A: Authenticator> {
    auth: A,
    rooms: HashMap<String, Room>,
    /// conn -> room, for routing subsequent messages after Join.
    conn_room: HashMap<ConnId, String>,
}

impl<A: Authenticator> SyncHub<A> {
    pub fn new(auth: A) -> Self {
        SyncHub {
            auth,
            rooms: HashMap::new(),
            conn_room: HashMap::new(),
        }
    }

    /// Read-only access to a room's authoritative document (e.g. for snapshotting
    /// into a version-control commit).
    pub fn document(&self, room: &str) -> Option<&CrdtDoc> {
        self.rooms.get(room).map(|r| &r.doc)
    }

    /// Number of connected members in a room.
    pub fn member_count(&self, room: &str) -> usize {
        self.rooms.get(room).map(|r| r.members.len()).unwrap_or(0)
    }

    /// Handle an inbound client message from `conn`, returning messages to send.
    pub fn handle(&mut self, conn: ConnId, msg: ClientMessage) -> Vec<Outbound> {
        match msg {
            ClientMessage::Join {
                room,
                token,
                replica,
                display_name,
            } => self.on_join(conn, room, token, replica, display_name),
            ClientMessage::Ops { ops } => self.on_ops(conn, ops),
            ClientMessage::Presence { presence } => self.on_presence(conn, presence),
            ClientMessage::SetAccess { subject, level } => self.on_set_access(conn, subject, level),
            ClientMessage::Resync => self.on_resync(conn),
            ClientMessage::Leave => self.on_leave(conn),
        }
    }

    fn reject(conn: ConnId, reason: &str) -> Vec<Outbound> {
        vec![Outbound {
            to: conn,
            message: ServerMessage::Rejected {
                reason: reason.to_string(),
            },
        }]
    }

    fn on_join(
        &mut self,
        conn: ConnId,
        room: String,
        token: String,
        replica: ReplicaId,
        display_name: String,
    ) -> Vec<Outbound> {
        let Some((subject, level)) = self.auth.authenticate(&room, &token) else {
            return Self::reject(conn, "authentication failed");
        };

        let entry = self.rooms.entry(room.clone()).or_default();
        // Replace any prior connection for the same replica (reconnect).
        entry.members.retain(|m| m.replica != replica);
        entry.members.push(Member {
            conn,
            replica,
            subject,
            level,
        });
        self.conn_room.insert(conn, room.clone());

        let room_ref = self.rooms.get(&room).unwrap();
        let presence: Vec<Presence> = room_ref.presence.users().cloned().collect();
        let snapshot = room_ref.doc.clone();
        let _ = display_name;

        vec![Outbound {
            to: conn,
            message: ServerMessage::Welcome {
                level,
                snapshot,
                presence,
            },
        }]
    }

    fn on_ops(&mut self, conn: ConnId, ops: Vec<Op>) -> Vec<Outbound> {
        let Some(room_name) = self.conn_room.get(&conn).cloned() else {
            return Self::reject(conn, "not in a room");
        };
        let room = self.rooms.get_mut(&room_name).unwrap();
        let Some(member) = room.member(conn) else {
            return Self::reject(conn, "unknown member");
        };
        if !member.level.can_edit() {
            return Self::reject(conn, "insufficient permissions to edit");
        }
        let replica = member.replica;

        // Apply to the authoritative document (CRDT merge is order-independent).
        for op in &ops {
            room.doc.apply(op);
        }

        // Broadcast to everyone else.
        let targets = room.other_conns(conn);
        targets
            .into_iter()
            .map(|to| Outbound {
                to,
                message: ServerMessage::Ops {
                    from: replica,
                    ops: ops.clone(),
                },
            })
            .collect()
    }

    fn on_presence(&mut self, conn: ConnId, presence: Presence) -> Vec<Outbound> {
        let Some(room_name) = self.conn_room.get(&conn).cloned() else {
            return Self::reject(conn, "not in a room");
        };
        let room = self.rooms.get_mut(&room_name).unwrap();
        if room.member(conn).is_none() {
            return Self::reject(conn, "unknown member");
        }
        if !room.presence.update(presence.clone()) {
            return vec![];
        }
        let targets = room.other_conns(conn);
        targets
            .into_iter()
            .map(|to| Outbound {
                to,
                message: ServerMessage::Presence {
                    presence: presence.clone(),
                },
            })
            .collect()
    }

    fn on_set_access(
        &mut self,
        conn: ConnId,
        subject: String,
        level: AccessLevel,
    ) -> Vec<Outbound> {
        let Some(room_name) = self.conn_room.get(&conn).cloned() else {
            return Self::reject(conn, "not in a room");
        };
        let room = self.rooms.get_mut(&room_name).unwrap();
        let Some(caller) = room.member(conn) else {
            return Self::reject(conn, "unknown member");
        };
        if !caller.level.can_admin() {
            return Self::reject(conn, "only owners may change access");
        }
        // Update the level of every connected member with that subject.
        let mut changed = Vec::new();
        for m in room.members.iter_mut() {
            if m.subject == subject {
                m.level = level;
                changed.push(m.conn);
            }
        }
        // Acknowledge to caller; affected members would be re-welcomed by a real
        // adapter. Here we simply confirm to the caller.
        let _ = changed;
        vec![]
    }

    fn on_resync(&mut self, conn: ConnId) -> Vec<Outbound> {
        let Some(room_name) = self.conn_room.get(&conn).cloned() else {
            return Self::reject(conn, "not in a room");
        };
        let room = self.rooms.get(&room_name).unwrap();
        vec![Outbound {
            to: conn,
            message: ServerMessage::Snapshot {
                snapshot: room.doc.clone(),
            },
        }]
    }

    fn on_leave(&mut self, conn: ConnId) -> Vec<Outbound> {
        let Some(room_name) = self.conn_room.remove(&conn) else {
            return vec![];
        };
        let Some(room) = self.rooms.get_mut(&room_name) else {
            return vec![];
        };
        let replica = room.member(conn).map(|m| m.replica);
        room.members.retain(|m| m.conn != conn);
        if let Some(replica) = replica {
            room.presence.remove(replica);
            let targets = room.other_conns(conn);
            return targets
                .into_iter()
                .map(|to| Outbound {
                    to,
                    message: ServerMessage::Left { replica },
                })
                .collect();
        }
        vec![]
    }

    /// Handle an abrupt disconnect (transport-level), same as `Leave`.
    pub fn disconnect(&mut self, conn: ConnId) -> Vec<Outbound> {
        self.on_leave(conn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crdt::ParamValue;

    fn auth() -> MemoryAuth {
        let mut a = MemoryAuth::new();
        a.add_token("tok-owner", "alice");
        a.add_token("tok-editor", "bob");
        a.add_token("tok-viewer", "carol");
        a.grant("room1", "alice", AccessLevel::Owner);
        a.grant("room1", "bob", AccessLevel::Editor);
        a.grant("room1", "carol", AccessLevel::Viewer);
        a
    }

    fn join(
        hub: &mut SyncHub<MemoryAuth>,
        conn: ConnId,
        token: &str,
        replica: u64,
    ) -> Vec<Outbound> {
        hub.handle(
            conn,
            ClientMessage::Join {
                room: "room1".into(),
                token: token.into(),
                replica: ReplicaId(replica),
                display_name: format!("user{replica}"),
            },
        )
    }

    #[test]
    fn join_requires_valid_token() {
        let mut hub = SyncHub::new(auth());
        let out = join(&mut hub, 1, "bogus", 1);
        assert!(matches!(out[0].message, ServerMessage::Rejected { .. }));
    }

    #[test]
    fn welcome_carries_snapshot() {
        let mut hub = SyncHub::new(auth());
        let out = join(&mut hub, 1, "tok-editor", 1);
        assert!(matches!(out[0].message, ServerMessage::Welcome { .. }));
    }

    #[test]
    fn editor_ops_broadcast_to_others() {
        let mut hub = SyncHub::new(auth());
        join(&mut hub, 1, "tok-editor", 1);
        join(&mut hub, 2, "tok-viewer", 2);

        let op = Op::AddFeature {
            key: 1,
            kind: "Extrude".into(),
            tag: crate::clock::HybridClock {
                wall_ms: 1,
                lamport: 1,
                replica: ReplicaId(1),
            },
            position: "a".into(),
        };
        let out = hub.handle(1, ClientMessage::Ops { ops: vec![op] });
        // Only conn 2 receives it.
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].to, 2);
        assert_eq!(hub.document("room1").unwrap().len(), 1);
    }

    #[test]
    fn viewer_cannot_edit() {
        let mut hub = SyncHub::new(auth());
        join(&mut hub, 3, "tok-viewer", 3);
        let op = Op::SetParam {
            key: 1,
            name: "height".into(),
            value: ParamValue::Number(2.0),
            clock: crate::clock::HybridClock {
                wall_ms: 1,
                lamport: 1,
                replica: ReplicaId(3),
            },
        };
        let out = hub.handle(3, ClientMessage::Ops { ops: vec![op] });
        assert!(matches!(out[0].message, ServerMessage::Rejected { .. }));
    }

    #[test]
    fn only_owner_sets_access() {
        let mut hub = SyncHub::new(auth());
        join(&mut hub, 2, "tok-editor", 2);
        let out = hub.handle(
            2,
            ClientMessage::SetAccess {
                subject: "carol".into(),
                level: AccessLevel::Editor,
            },
        );
        assert!(matches!(out[0].message, ServerMessage::Rejected { .. }));
    }

    #[test]
    fn resync_returns_snapshot() {
        let mut hub = SyncHub::new(auth());
        join(&mut hub, 1, "tok-editor", 1);
        let out = hub.handle(1, ClientMessage::Resync);
        assert!(matches!(out[0].message, ServerMessage::Snapshot { .. }));
    }

    #[test]
    fn leave_notifies_others() {
        let mut hub = SyncHub::new(auth());
        join(&mut hub, 1, "tok-editor", 1);
        join(&mut hub, 2, "tok-editor", 2);
        // bob token reused for both; both editors.
        let out = hub.handle(1, ClientMessage::Leave);
        assert_eq!(out.len(), 1);
        assert!(matches!(out[0].message, ServerMessage::Left { .. }));
        assert_eq!(hub.member_count("room1"), 1);
    }

    #[test]
    fn offline_ops_merge_after_reconnect() {
        // Two editors edit; one goes offline and buffers ops, then rejoins and
        // flushes them. CRDT merge means order doesn't matter.
        let mut hub = SyncHub::new(auth());
        join(&mut hub, 1, "tok-owner", 1);
        join(&mut hub, 2, "tok-editor", 2);

        let clk = |r, l| crate::clock::HybridClock {
            wall_ms: l,
            lamport: l,
            replica: ReplicaId(r),
        };

        // Online edit from replica 1.
        let add = Op::AddFeature {
            key: 10,
            kind: "Extrude".into(),
            tag: clk(1, 1),
            position: "a".into(),
        };
        hub.handle(1, ClientMessage::Ops { ops: vec![add] });

        // Replica 2 "disconnects" and buffers an edit locally.
        hub.disconnect(2);
        let buffered = Op::SetParam {
            key: 10,
            name: "height".into(),
            value: ParamValue::Number(5.0),
            clock: clk(2, 2),
        };

        // Replica 2 reconnects, resyncs, then flushes buffered ops.
        join(&mut hub, 2, "tok-editor", 2);
        hub.handle(
            2,
            ClientMessage::Ops {
                ops: vec![buffered],
            },
        );

        let doc = hub.document("room1").unwrap();
        assert_eq!(doc.param(10, "height"), Some(&ParamValue::Number(5.0)));
    }
}
