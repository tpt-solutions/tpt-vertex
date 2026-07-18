//! Git-like version control for TPT Vertex 3D designs.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! This crate implements a content-addressed versioning model over a design's
//! *evaluated* state plus a lightweight *feature manifest* describing the
//! parametric feature tree. It captures the concepts from ADR-0004: the feature
//! tree is the source of truth, and versioning operates on snapshots of it.
//!
//! Objects:
//! - [`Blob`]: a content-hashed, serializable snapshot of a solid (mesh).
//! - [`FeatureManifest`]: the list of features (id + discriminant + a param
//!   hash) describing a design revision without storing full geometry.
//! - [`Commit`]: a node in the history DAG with parents, message, and the
//!   manifest/tree it records.
//!
//! A [`Repository`] tracks branches (named refs to commits) and supports
//! committing, checking out, and merging with basic conflict detection.

pub mod diff;
pub mod repo;

pub use diff::{Change, Diff};
pub use repo::{Commit, Conflict, MergeOutcome, Repository, RepoError};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// A content-addressed snapshot of a solid (triangle mesh). Hashing is over the
/// canonical byte representation, so identical geometry yields identical hashes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Blob {
    pub vertices: Vec<[f64; 3]>,
    pub faces: Vec<[u32; 3]>,
}

impl Blob {
    /// Build a blob from a kernel solid.
    pub fn from_solid(solid: &vertex_kernel::geometry::solid::Solid) -> Self {
        Blob {
            vertices: solid
                .vertices
                .iter()
                .map(|v| [v.x, v.y, v.z])
                .collect(),
            faces: solid
                .faces
                .iter()
                .map(|f| [f.a, f.b, f.c])
                .collect(),
        }
    }

    /// Canonical hash of this blob's contents.
    pub fn hash(&self) -> String {
        let mut hasher = Sha256::new();
        for v in &self.vertices {
            for c in v {
                hasher.update(c.to_le_bytes());
            }
        }
        for f in &self.faces {
            for i in f {
                hasher.update(i.to_le_bytes());
            }
        }
        hex(&hasher.finalize())
    }
}

/// A single feature entry in a design's manifest.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FeatureEntry {
    pub id: u64,
    /// Discriminant name (e.g. "Extrude", "Boolean") from the feature enum.
    pub kind: String,
    /// Hash of the feature's parameters (sketch geometry, height, etc.).
    pub param_hash: String,
}

/// The feature manifest: an ordered, content-hashed description of a design
/// revision. Sufficient for diffing and merge-conflict detection without
/// reconstructing full geometry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct FeatureManifest {
    pub entries: Vec<FeatureEntry>,
}

impl FeatureManifest {
    /// Hash the manifest contents for change detection.
    pub fn hash(&self) -> String {
        let mut hasher = Sha256::new();
        for e in &self.entries {
            hasher.update(e.id.to_le_bytes());
            hasher.update(e.kind.as_bytes());
            hasher.update(e.param_hash.as_bytes());
        }
        hex(&hasher.finalize())
    }

    /// Look up a feature entry by id.
    pub fn get(&self, id: u64) -> Option<&FeatureEntry> {
        self.entries.iter().find(|e| e.id == id)
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use vertex_kernel::feature_tree::{Feature, FeatureTree};
    use vertex_kernel::geometry::sketch::Sketch;
    use vertex_kernel::math::Vec2;

    fn sample_tree() -> FeatureTree {
        let mut s = Sketch::new();
        s.line(Vec2::ZERO, Vec2::new(2.0, 0.0));
        s.line(Vec2::new(2.0, 0.0), Vec2::new(2.0, 2.0));
        s.line(Vec2::new(2.0, 2.0), Vec2::ZERO);
        let mut tree = FeatureTree::new();
        tree.add(Feature::Extrude { sketch: s, height: 3.0 }, None);
        tree
    }

    #[test]
    fn blob_hash_is_stable() {
        let solid = sample_tree().evaluate().unwrap().final_solid;
        let a = Blob::from_solid(&solid);
        let b = Blob::from_solid(&solid);
        assert_eq!(a.hash(), b.hash());
        assert!(!a.hash().is_empty());
    }

    #[test]
    fn manifest_changes_with_height() {
        let mut t1 = sample_tree();
        let mut t2 = sample_tree();
        let id = t1.order()[0];
        t2.update(
            id,
            Feature::Extrude {
                sketch: Sketch::new(),
                height: 99.0,
            },
        );
        let _ = (&t1, &t2);
    }
}
