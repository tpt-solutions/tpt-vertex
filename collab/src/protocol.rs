//! Wire protocol for the collaboration sync channel.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! Transport-agnostic message types exchanged between clients and the sync hub.
//! These are `serde`-serializable and are intended to be sent as JSON (or any
//! serde format) over a WebSocket in production. Keeping them transport-agnostic
//! lets the hub be unit-tested in-process without a network stack.

use serde::{Deserialize, Serialize};

use crate::clock::ReplicaId;
use crate::crdt::{CrdtDoc, Op};
use crate::presence::Presence;

/// Access level a member holds on a document/room.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum AccessLevel {
    /// May read state and presence but not mutate the document.
    Viewer,
    /// May read and mutate the document.
    Editor,
    /// Editor plus membership/permission management.
    Owner,
}

impl AccessLevel {
    pub fn can_edit(self) -> bool {
        matches!(self, AccessLevel::Editor | AccessLevel::Owner)
    }
    pub fn can_admin(self) -> bool {
        matches!(self, AccessLevel::Owner)
    }
}

/// Client → server messages.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ClientMessage {
    /// Join a room. `token` authenticates the session; `since` (a state hash or
    /// count) lets the server decide whether to send a full snapshot or deltas.
    Join {
        room: String,
        token: String,
        replica: ReplicaId,
        display_name: String,
    },
    /// Apply CRDT ops produced locally.
    Ops { ops: Vec<Op> },
    /// Update ephemeral presence.
    Presence { presence: Presence },
    /// Owner-only: grant/change a member's access level.
    SetAccess { subject: String, level: AccessLevel },
    /// Request a full resync (used after a long offline period).
    Resync,
    /// Leave the room.
    Leave,
}

/// Server → client messages.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ServerMessage {
    /// Join accepted; includes the caller's access level and a full snapshot.
    Welcome {
        level: AccessLevel,
        snapshot: CrdtDoc,
        presence: Vec<Presence>,
    },
    /// Join or an action was rejected.
    Rejected { reason: String },
    /// Broadcast of ops from another participant (or echoed for confirmation).
    Ops { from: ReplicaId, ops: Vec<Op> },
    /// Broadcast of a presence update.
    Presence { presence: Presence },
    /// A participant left.
    Left { replica: ReplicaId },
    /// Full-state resync payload.
    Snapshot { snapshot: CrdtDoc },
}
