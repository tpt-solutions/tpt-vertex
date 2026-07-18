//! Derive a [`FeatureManifest`] from a kernel [`FeatureTree`].
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! The versioning model diffs and merges on *feature manifests* rather than raw
//! geometry (ADR-0004: the feature tree is the source of truth). This module
//! walks a kernel [`tpt_vertex_kernel::feature_tree::FeatureTree`] in evaluation
//! order and produces a [`FeatureManifest`] whose per-feature `param_hash`
//! captures the feature's parameters, so any parameter change flips the hash and
//! is detected by [`crate::diff::Diff::between`].

use sha2::{Digest, Sha256};

use tpt_vertex_kernel::feature_tree::{BooleanOp, Feature, FeatureTree};
use tpt_vertex_kernel::geometry::sketch::{Sketch, SketchEntity};
use tpt_vertex_kernel::math::{Vec2, Vec3};

use crate::{hex, FeatureEntry, FeatureManifest};

/// Build a [`FeatureManifest`] from a feature tree, preserving feature order.
pub fn manifest_from_tree(tree: &FeatureTree) -> FeatureManifest {
    let mut entries = Vec::new();
    for &id in tree.order() {
        if let Some(feature) = tree.get(id) {
            entries.push(FeatureEntry {
                id: id.0,
                kind: kind_name(feature).to_string(),
                param_hash: hash_feature(feature),
            });
        }
    }
    FeatureManifest { entries }
}

fn kind_name(f: &Feature) -> &'static str {
    match f {
        Feature::Extrude { .. } => "Extrude",
        Feature::Revolve { .. } => "Revolve",
        Feature::Sweep { .. } => "Sweep",
        Feature::Loft { .. } => "Loft",
        Feature::Boolean { .. } => "Boolean",
        Feature::Fillet { .. } => "Fillet",
        Feature::Chamfer { .. } => "Chamfer",
        Feature::Transform { .. } => "Transform",
    }
}

/// Deterministic hash of a feature's parameters (independent of its id).
fn hash_feature(f: &Feature) -> String {
    let mut h = Sha256::new();
    h.update(kind_name(f).as_bytes());
    match f {
        Feature::Extrude { sketch, height } => {
            hash_sketch(&mut h, sketch);
            h.update(height.to_le_bytes());
        }
        Feature::Revolve {
            sketch,
            angle,
            segments,
        } => {
            hash_sketch(&mut h, sketch);
            h.update(angle.to_le_bytes());
            h.update((*segments as u64).to_le_bytes());
        }
        Feature::Sweep { sketch, path } => {
            hash_sketch(&mut h, sketch);
            for p in path {
                hash_vec3(&mut h, *p);
            }
        }
        Feature::Loft {
            sketch0,
            sketch1,
            height,
        } => {
            hash_sketch(&mut h, sketch0);
            hash_sketch(&mut h, sketch1);
            h.update(height.to_le_bytes());
        }
        Feature::Boolean { op, a, b } => {
            h.update([boolean_tag(*op)]);
            h.update(a.0.to_le_bytes());
            h.update(b.0.to_le_bytes());
        }
        Feature::Fillet { parent, radius } => {
            h.update(parent.0.to_le_bytes());
            h.update(radius.to_le_bytes());
        }
        Feature::Chamfer { parent, distance } => {
            h.update(parent.0.to_le_bytes());
            h.update(distance.to_le_bytes());
        }
        Feature::Transform {
            parent,
            translation,
            rotation,
        } => {
            h.update(parent.0.to_le_bytes());
            hash_vec3(&mut h, *translation);
            hash_vec3(&mut h, *rotation);
        }
    }
    hex(&h.finalize())
}

fn boolean_tag(op: BooleanOp) -> u8 {
    match op {
        BooleanOp::Union => 0,
        BooleanOp::Subtract => 1,
        BooleanOp::Intersect => 2,
    }
}

fn hash_vec2(h: &mut Sha256, v: Vec2) {
    h.update(v.x.to_le_bytes());
    h.update(v.y.to_le_bytes());
}

fn hash_vec3(h: &mut Sha256, v: Vec3) {
    h.update(v.x.to_le_bytes());
    h.update(v.y.to_le_bytes());
    h.update(v.z.to_le_bytes());
}

fn hash_sketch(h: &mut Sha256, sketch: &Sketch) {
    for e in &sketch.entities {
        match e {
            SketchEntity::Line(l) => {
                h.update([0u8]);
                hash_vec2(h, sketch_point(sketch, l.start));
                hash_vec2(h, sketch_point(sketch, l.end));
            }
            SketchEntity::Circle(c) => {
                h.update([1u8]);
                hash_vec2(h, sketch_point(sketch, c.center));
                hash_vec2(h, sketch_point(sketch, c.radius_point));
            }
            SketchEntity::Arc(a) => {
                h.update([2u8]);
                hash_vec2(h, sketch_point(sketch, a.start));
                hash_vec2(h, sketch_point(sketch, a.end));
                hash_vec2(h, sketch_point(sketch, a.center));
                h.update([a.ccw as u8]);
            }
            SketchEntity::Polyline(p) => {
                h.update([3u8]);
                h.update([p.closed as u8]);
                for v in &p.points {
                    hash_vec2(h, sketch_point(sketch, *v));
                }
            }
        }
    }
}

fn sketch_point(sketch: &Sketch, id: tpt_vertex_kernel::geometry::sketch::VertexId) -> Vec2 {
    sketch
        .points
        .iter()
        .find(|p| p.id == id)
        .map(|p| p.pos)
        .unwrap_or(Vec2::ZERO)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_vertex_kernel::feature_tree::{Feature, FeatureTree};
    use tpt_vertex_kernel::geometry::sketch::Sketch;
    use tpt_vertex_kernel::math::Vec2;

    fn tree_with_height(height: f64) -> FeatureTree {
        let mut s = Sketch::new();
        s.line(Vec2::ZERO, Vec2::new(2.0, 0.0));
        s.line(Vec2::new(2.0, 0.0), Vec2::new(2.0, 2.0));
        s.line(Vec2::new(2.0, 2.0), Vec2::ZERO);
        let mut tree = FeatureTree::new();
        tree.add(Feature::Extrude { sketch: s, height }, None);
        tree
    }

    #[test]
    fn manifest_matches_feature_order_and_kind() {
        let tree = tree_with_height(3.0);
        let m = manifest_from_tree(&tree);
        assert_eq!(m.entries.len(), 1);
        assert_eq!(m.entries[0].kind, "Extrude");
        assert!(!m.entries[0].param_hash.is_empty());
    }

    #[test]
    fn param_hash_changes_with_height() {
        let a = manifest_from_tree(&tree_with_height(3.0));
        let b = manifest_from_tree(&tree_with_height(9.0));
        assert_ne!(a.entries[0].param_hash, b.entries[0].param_hash);
        assert_eq!(a.entries[0].id, b.entries[0].id);
    }

    #[test]
    fn identical_trees_hash_identically() {
        let a = manifest_from_tree(&tree_with_height(3.0));
        let b = manifest_from_tree(&tree_with_height(3.0));
        assert_eq!(a.hash(), b.hash());
    }
}
