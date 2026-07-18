//! Integration tests for feature-tree rebuild correctness.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0

use tpt_vertex_kernel::feature_tree::{Feature, FeatureTree};
use tpt_vertex_kernel::geometry::sketch::Sketch;
use tpt_vertex_kernel::math::Vec2;

fn rect_sketch(x0: f64, y0: f64, x1: f64, y1: f64) -> Sketch {
    let mut s = Sketch::new();
    s.line(Vec2::new(x0, y0), Vec2::new(x1, y0));
    s.line(Vec2::new(x1, y0), Vec2::new(x1, y1));
    s.line(Vec2::new(x1, y1), Vec2::new(x0, y1));
    s.line(Vec2::new(x0, y1), Vec2::new(x0, y0));
    s
}

#[test]
fn chained_extrude_then_boolean_union() {
    let mut tree = FeatureTree::new();
    let a = tree.add(
        Feature::Extrude {
            sketch: rect_sketch(0.0, 0.0, 2.0, 2.0),
            height: 1.0,
        },
        None,
    );
    let b = tree.add(
        Feature::Extrude {
            sketch: rect_sketch(1.0, 1.0, 3.0, 3.0),
            height: 1.0,
        },
        None,
    );
    let union = tree.add(
        Feature::Boolean {
            op: tpt_vertex_kernel::feature_tree::BooleanOp::Union,
            a,
            b,
        },
        None,
    );
    let eval = tree.evaluate().unwrap();
    // NOTE: the v1 boolean `union` is a documented placeholder that
    // concatenates the two meshes (exact CSG union is a later refinement, see
    // ADR-0004). Each prism is volume 4, so the concatenated result is 8.
    let vol = eval.features[&union].volume().abs();
    assert!((vol - 8.0).abs() < 1e-6, "union volume was {vol}");
}

#[test]
fn parameter_change_rebuilds_dependent_subgraph() {
    let mut tree = FeatureTree::new();
    let base = tree.add(
        Feature::Extrude {
            sketch: rect_sketch(0.0, 0.0, 2.0, 2.0),
            height: 1.0,
        },
        None,
    );
    let moved = tree.add(
        Feature::Transform {
            parent: base,
            translation: tpt_vertex_kernel::math::Vec3::new(0.0, 0.0, 0.0),
            rotation: tpt_vertex_kernel::math::Vec3::ZERO,
        },
        Some(base),
    );
    let before = tree.evaluate().unwrap().features[&moved].bounds().unwrap();
    // Change the base height; the moved feature must reflect new geometry.
    tree.update(
        base,
        Feature::Extrude {
            sketch: rect_sketch(0.0, 0.0, 2.0, 2.0),
            height: 5.0,
        },
    );
    let after = tree.evaluate().unwrap().features[&moved].bounds().unwrap();
    assert!((before.1.z - 1.0).abs() < 1e-9);
    assert!((after.1.z - 5.0).abs() < 1e-9);
}

#[test]
fn missing_parent_is_error() {
    let mut tree = FeatureTree::new();
    let ghost = tpt_vertex_kernel::feature_tree::FeatureId(999);
    tree.add(
        Feature::Boolean {
            op: tpt_vertex_kernel::feature_tree::BooleanOp::Union,
            a: ghost,
            b: ghost,
        },
        None,
    );
    assert!(tree.evaluate().is_err());
}
