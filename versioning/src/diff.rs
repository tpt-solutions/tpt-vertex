//! Diffing between two design revisions.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! A [`Diff`] summarizes what changed between two [`FeatureManifest`]s: which
//! features were added, removed, or had their parameters modified (detected via
//! per-feature `param_hash`). Geometry blobs can be compared separately via
//! their content hashes when a visual diff is needed.

use crate::{FeatureEntry, FeatureManifest};

/// The kind of change applied to a single feature between revisions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// A feature present in `other` but not in `base`.
    Added(u64),
    /// A feature present in `base` but not in `other`.
    Removed(u64),
    /// A feature present in both whose parameter hash differs.
    Modified(u64),
}

/// A set of per-feature changes between two manifests.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Diff {
    pub changes: Vec<Change>,
}

impl Diff {
    /// Compute the per-feature diff between `base` and `other`.
    pub fn between(base: &FeatureManifest, other: &FeatureManifest) -> Self {
        let mut changes = Vec::new();

        let base_map: std::collections::HashMap<u64, &FeatureEntry> =
            base.entries.iter().map(|e| (e.id, e)).collect();
        let other_map: std::collections::HashMap<u64, &FeatureEntry> =
            other.entries.iter().map(|e| (e.id, e)).collect();

        for e in &other.entries {
            match base_map.get(&e.id) {
                None => changes.push(Change::Added(e.id)),
                Some(b) if b.param_hash != e.param_hash => changes.push(Change::Modified(e.id)),
                _ => {}
            }
        }
        for e in &base.entries {
            if !other_map.contains_key(&e.id) {
                changes.push(Change::Removed(e.id));
            }
        }

        changes.sort_by_key(|c| match c {
            Change::Added(id) | Change::Removed(id) | Change::Modified(id) => *id,
        });
        Diff { changes }
    }

    /// True if the two revisions are identical at the manifest level.
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }
}
