//! Real-time collaboration for TPT Vertex via CRDTs over the feature tree.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! Implements ADR-0006: a custom, Rust-native CRDT tailored to the parametric
//! feature tree, synchronized over a transport-agnostic hub.
//!
//! Modules:
//! - [`clock`]: hybrid logical clocks ([`clock::HybridClock`], [`clock::ReplicaId`]).
//! - [`crdt`]: the replicated document ([`crdt::CrdtDoc`], [`crdt::Op`],
//!   [`crdt::LocalReplica`]) with OR-Set membership and LWW parameter registers.
//! - [`presence`]: ephemeral awareness ([`presence::Presence`]).
//! - [`protocol`]: wire messages and [`protocol::AccessLevel`].
//! - [`server`]: the [`server::SyncHub`] with authentication, access control,
//!   real-time propagation, and offline resync.
//!
//! The document converges to a feature tree, so a room's [`crdt::CrdtDoc`] can be
//! snapshotted at any time (see [`snapshot`]) to feed the versioning crate.

pub mod clock;
pub mod crdt;
pub mod presence;
pub mod protocol;
pub mod server;

pub use clock::{HybridClock, ReplicaId};
pub use crdt::{CrdtDoc, FeatureKey, FeatureState, LocalReplica, Op, ParamValue, Register};
pub use presence::{Presence, PresenceMap};
pub use protocol::{AccessLevel, ClientMessage, ServerMessage};
pub use server::{Authenticator, ConnId, MemoryAuth, Outbound, SyncHub};

/// A flat, ordered snapshot of a collaborative document: one entry per live
/// feature, in history order, with its kind and parameter values. This is the
/// hand-off point to the versioning crate (which builds a `FeatureManifest` from
/// the same feature-tree data), keeping collaboration and version control on one
/// data model per ADR-0005/0006.
#[derive(Debug, Clone, PartialEq)]
pub struct SnapshotEntry {
    pub key: FeatureKey,
    pub kind: String,
    pub params: Vec<(String, ParamValue)>,
}

/// Produce an ordered snapshot of the live features in `doc`.
pub fn snapshot(doc: &CrdtDoc) -> Vec<SnapshotEntry> {
    doc.ordered_keys()
        .into_iter()
        .filter_map(|key| {
            let f = doc.feature(key)?;
            let kind = match f.kind.as_ref().map(|r| &r.value) {
                Some(ParamValue::Text(s)) => s.clone(),
                _ => String::new(),
            };
            let params = f
                .params
                .iter()
                .map(|(name, reg)| (name.clone(), reg.value.clone()))
                .collect();
            Some(SnapshotEntry { key, kind, params })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_replicas_converge_end_to_end() {
        // Simulate two clients editing through the hub.
        let mut a = MemoryAuth::new();
        a.add_token("t1", "u1");
        a.add_token("t2", "u2");
        a.set_default_level(Some(AccessLevel::Editor));
        let mut hub = SyncHub::new(a);

        let join = |hub: &mut SyncHub<MemoryAuth>, conn, tok: &str, r| {
            hub.handle(
                conn,
                ClientMessage::Join {
                    room: "r".into(),
                    token: tok.into(),
                    replica: ReplicaId(r),
                    display_name: "x".into(),
                },
            )
        };
        join(&mut hub, 1, "t1", 1);
        join(&mut hub, 2, "t2", 2);

        // Client 1 builds a feature and edits it; feed ops through the hub.
        let mut r1 = LocalReplica::new(ReplicaId(1));
        let (k, add) = r1.add_feature("Extrude", "a");
        let set = r1.set_param(k, "height", ParamValue::Number(4.0));
        hub.handle(
            1,
            ClientMessage::Ops {
                ops: vec![add, set],
            },
        );

        let doc = hub.document("r").unwrap();
        let snap = snapshot(doc);
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].kind, "Extrude");
        assert!(snap[0]
            .params
            .iter()
            .any(|(n, v)| n == "height" && *v == ParamValue::Number(4.0)));
    }
}
