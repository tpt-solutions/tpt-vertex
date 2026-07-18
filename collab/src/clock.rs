//! Logical clocks for CRDT causality.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! A [`HybridClock`] pairs a Lamport counter with a wall-clock millisecond
//! reading and a [`ReplicaId`]. Comparisons are total and deterministic across
//! replicas: higher wall-clock wins, ties break on the Lamport counter, and
//! finally on replica id. This gives last-writer-wins (LWW) registers a stable,
//! commutative ordering independent of message arrival order.

use serde::{Deserialize, Serialize};

/// Unique identifier for a participating replica (client or server session).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ReplicaId(pub u64);

/// A hybrid logical timestamp: `(wall_ms, lamport, replica)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HybridClock {
    /// Wall-clock milliseconds (best-effort; only used as the primary ordering key).
    pub wall_ms: u64,
    /// Monotonic Lamport counter, incremented on every local event.
    pub lamport: u64,
    /// Originating replica id (final tiebreak, guarantees total order).
    pub replica: ReplicaId,
}

impl HybridClock {
    pub fn new(replica: ReplicaId) -> Self {
        HybridClock {
            wall_ms: 0,
            lamport: 0,
            replica,
        }
    }

    /// Produce the next timestamp for a local event, folding in an external
    /// wall-clock reading (pass `0` in tests for determinism).
    pub fn tick(&mut self, wall_ms: u64) -> HybridClock {
        self.lamport += 1;
        self.wall_ms = self.wall_ms.max(wall_ms);
        HybridClock {
            wall_ms: self.wall_ms,
            lamport: self.lamport,
            replica: self.replica,
        }
    }

    /// Advance this clock upon observing a remote timestamp (Lamport merge).
    pub fn observe(&mut self, other: &HybridClock) {
        self.lamport = self.lamport.max(other.lamport);
        self.wall_ms = self.wall_ms.max(other.wall_ms);
    }
}

impl PartialOrd for HybridClock {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for HybridClock {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.wall_ms
            .cmp(&other.wall_ms)
            .then(self.lamport.cmp(&other.lamport))
            .then(self.replica.0.cmp(&other.replica.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_is_monotonic() {
        let mut c = HybridClock::new(ReplicaId(1));
        let a = c.tick(0);
        let b = c.tick(0);
        assert!(b > a);
    }

    #[test]
    fn total_order_breaks_ties_on_replica() {
        let a = HybridClock {
            wall_ms: 5,
            lamport: 2,
            replica: ReplicaId(1),
        };
        let b = HybridClock {
            wall_ms: 5,
            lamport: 2,
            replica: ReplicaId(2),
        };
        assert!(b > a);
    }

    #[test]
    fn observe_advances_lamport() {
        let mut c = HybridClock::new(ReplicaId(1));
        c.observe(&HybridClock {
            wall_ms: 10,
            lamport: 42,
            replica: ReplicaId(9),
        });
        let next = c.tick(0);
        assert!(next.lamport > 42);
    }
}
