//! WebAssembly bindings for real-time collaboration (browser use via `wasm-bindgen`).
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! Compiled with `cargo build --target wasm32-unknown-unknown --features wasm`
//! (or `wasm-pack`), mirroring `tpt-vertex-kernel::wasm`. This is the *client*
//! half of the collaboration stack: it owns a [`LocalReplica`], turns local
//! edits into JSON [`ClientMessage`]s to push over a WebSocket, and folds
//! inbound JSON [`ServerMessage`]s back into the local document and presence
//! map.
//!
//! The JSON encoding is serde's default (externally tagged) representation, so
//! it matches `src/bin/sync_server.rs` and `frontend/src/collab/client.ts`
//! byte-for-byte. Numbers cross the JS boundary as `f64` rather than `u64` to
//! avoid forcing `BigInt` on callers; replica ids and feature keys stay well
//! inside the exactly-representable integer range.

use wasm_bindgen::prelude::*;

use crate::clock::ReplicaId;
use crate::crdt::{FeatureKey, LocalReplica, Op, ParamValue};
use crate::presence::{Presence, PresenceMap};
use crate::protocol::{ClientMessage, ServerMessage};

/// A browser-side collaboration session: local CRDT replica + outbound op queue
/// + remote presence.
#[wasm_bindgen]
pub struct CollabSession {
    replica: LocalReplica,
    display_name: String,
    /// Ops produced locally and not yet handed to the transport.
    pending: Vec<Op>,
    /// Remote users' ephemeral state (never includes the local replica).
    remote: PresenceMap,
    selection: Vec<FeatureKey>,
    epoch: u64,
}

#[wasm_bindgen]
impl CollabSession {
    #[wasm_bindgen(constructor)]
    pub fn new(replica_id: f64, display_name: String) -> CollabSession {
        CollabSession {
            replica: LocalReplica::new(ReplicaId(replica_id as u64)),
            display_name,
            pending: Vec::new(),
            remote: PresenceMap::new(),
            selection: Vec::new(),
            epoch: 0,
        }
    }

    /// This session's replica id.
    #[wasm_bindgen(getter)]
    pub fn replica_id(&self) -> f64 {
        self.replica.id.0 as f64
    }

    /// JSON `ClientMessage::Join` to send as the first frame on a socket.
    pub fn join_message(&self, room: &str, token: &str) -> String {
        encode(&ClientMessage::Join {
            room: room.to_string(),
            token: token.to_string(),
            replica: self.replica.id,
            display_name: self.display_name.clone(),
        })
    }

    /// Add a feature locally, queueing the op. Returns the new feature key.
    pub fn add_feature(&mut self, kind: &str, position: &str) -> f64 {
        let (key, op) = self.replica.add_feature(kind, position);
        self.pending.push(op);
        key as f64
    }

    /// Set a numeric parameter locally, queueing the op.
    pub fn set_number_param(&mut self, key: f64, name: &str, value: f64) {
        let op = self
            .replica
            .set_param(key as FeatureKey, name, ParamValue::Number(value));
        self.pending.push(op);
    }

    /// Set a text parameter locally, queueing the op.
    pub fn set_text_param(&mut self, key: f64, name: &str, value: &str) {
        let op =
            self.replica
                .set_param(key as FeatureKey, name, ParamValue::Text(value.to_string()));
        self.pending.push(op);
    }

    /// Remove a feature locally, queueing the op.
    pub fn remove_feature(&mut self, key: f64) {
        let op = self.replica.remove_feature(key as FeatureKey);
        self.pending.push(op);
    }

    /// Drain queued local ops as a JSON `ClientMessage::Ops`, or `None` when
    /// there is nothing to flush.
    pub fn take_pending_ops(&mut self) -> Option<String> {
        if self.pending.is_empty() {
            return None;
        }
        let ops = std::mem::take(&mut self.pending);
        Some(encode(&ClientMessage::Ops { ops }))
    }

    /// Replace the local selection (feature keys) reported through presence.
    pub fn set_selection(&mut self, keys: Vec<f64>) {
        self.selection = keys.into_iter().map(|k| k as FeatureKey).collect();
    }

    /// JSON `ClientMessage::Presence` carrying the local cursor and selection.
    /// Each call bumps the epoch so late-arriving updates are discarded.
    pub fn presence_message(&mut self, x: f64, y: f64, z: f64) -> String {
        self.epoch += 1;
        encode(&ClientMessage::Presence {
            presence: Presence {
                replica: self.replica.id,
                display_name: self.display_name.clone(),
                cursor: Some([x, y, z]),
                selection: self.selection.clone(),
                epoch: self.epoch,
            },
        })
    }

    /// Fold one inbound JSON `ServerMessage` into local state. Returns `true`
    /// when the document or presence map changed (i.e. the UI should redraw).
    pub fn apply_server_message(&mut self, json: &str) -> bool {
        let Ok(msg) = serde_json::from_str::<ServerMessage>(json) else {
            return false;
        };
        match msg {
            ServerMessage::Welcome {
                snapshot, presence, ..
            } => {
                self.replica.doc.merge(&snapshot);
                for p in presence {
                    if p.replica != self.replica.id {
                        self.remote.update(p);
                    }
                }
                true
            }
            ServerMessage::Snapshot { snapshot } => {
                self.replica.doc.merge(&snapshot);
                true
            }
            ServerMessage::Ops { ops, .. } => {
                let mut changed = false;
                for op in &ops {
                    changed |= self.replica.receive(op);
                }
                changed
            }
            ServerMessage::Presence { presence } => {
                presence.replica != self.replica.id && self.remote.update(presence)
            }
            ServerMessage::Left { replica } => self.remote.remove(replica),
            ServerMessage::Rejected { .. } => false,
        }
    }

    /// Remote presence as a JSON array (drives the multi-cursor overlay).
    pub fn remote_presence_json(&self) -> String {
        let users: Vec<&Presence> = self.remote.users().collect();
        serde_json::to_string(&users).unwrap_or_else(|_| "[]".to_string())
    }

    /// The local document as JSON (same shape as a `Snapshot` payload).
    pub fn document_json(&self) -> String {
        serde_json::to_string(&self.replica.doc).unwrap_or_else(|_| "null".to_string())
    }

    /// Number of live features in the local document.
    pub fn feature_count(&self) -> usize {
        self.replica.doc.len()
    }

    /// JSON `ClientMessage::Resync`, for reconnecting after an offline spell.
    pub fn resync_message(&self) -> String {
        encode(&ClientMessage::Resync)
    }

    /// JSON `ClientMessage::Leave`, for a graceful disconnect.
    pub fn leave_message(&self) -> String {
        encode(&ClientMessage::Leave)
    }
}

/// Encode a client message, falling back to a `Resync` no-op string on the
/// (unreachable) serialization failure so the JS boundary never panics.
fn encode(msg: &ClientMessage) -> String {
    serde_json::to_string(msg).unwrap_or_else(|_| "\"Resync\"".to_string())
}

/// Library entry point: ensure the WASM panic hook is installed.
#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
}
