//! Ephemeral presence / awareness state (cursors, selections).
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! Presence is *not* part of the persisted CRDT (ADR-0006). It is per-user
//! ephemeral state broadcast over the same channel and dropped when a user
//! disconnects. Each update is stamped with an incrementing epoch so stale
//! updates are ignored.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::clock::ReplicaId;
use crate::crdt::FeatureKey;

/// One user's live presence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Presence {
    pub replica: ReplicaId,
    pub display_name: String,
    /// Cursor position in world space `[x, y, z]`, if any.
    pub cursor: Option<[f64; 3]>,
    /// Currently selected feature keys.
    pub selection: Vec<FeatureKey>,
    /// Monotonic epoch; newer replaces older.
    pub epoch: u64,
}

/// Aggregated presence for a room.
#[derive(Debug, Clone, Default)]
pub struct PresenceMap {
    users: HashMap<ReplicaId, Presence>,
}

impl PresenceMap {
    pub fn new() -> Self {
        PresenceMap::default()
    }

    /// Apply an update, ignoring stale epochs. Returns `true` if state changed.
    pub fn update(&mut self, p: Presence) -> bool {
        match self.users.get(&p.replica) {
            Some(existing) if existing.epoch >= p.epoch => false,
            _ => {
                self.users.insert(p.replica, p);
                true
            }
        }
    }

    /// Drop a user (on disconnect).
    pub fn remove(&mut self, replica: ReplicaId) -> bool {
        self.users.remove(&replica).is_some()
    }

    pub fn users(&self) -> impl Iterator<Item = &Presence> {
        self.users.values()
    }

    pub fn len(&self) -> usize {
        self.users.len()
    }

    pub fn is_empty(&self) -> bool {
        self.users.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(replica: u64, epoch: u64) -> Presence {
        Presence {
            replica: ReplicaId(replica),
            display_name: format!("user{replica}"),
            cursor: Some([1.0, 2.0, 3.0]),
            selection: vec![],
            epoch,
        }
    }

    #[test]
    fn newer_epoch_wins_stale_ignored() {
        let mut m = PresenceMap::new();
        assert!(m.update(p(1, 2)));
        assert!(!m.update(p(1, 1))); // stale
        assert!(m.update(p(1, 3)));
        assert_eq!(m.len(), 1);
    }

    #[test]
    fn remove_drops_user() {
        let mut m = PresenceMap::new();
        m.update(p(1, 1));
        assert!(m.remove(ReplicaId(1)));
        assert!(m.is_empty());
    }
}
