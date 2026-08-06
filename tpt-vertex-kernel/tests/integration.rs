//! Integration tests for feature-tree rebuild correctness and the BSP
//! boolean/fillet engine (ADR-0013).
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0

use tpt_vertex_kernel::feature_tree::{Feature, FeatureTree};
use tpt_vertex_kernel::geometry::csg_bsp::{self, box_solid, bsp_subtract, bsp_union};
use tpt_vertex_kernel::geometry::edges::{chamfer_edges, classify_edges, fillet_edges, EdgeKind};
use tpt_vertex_kernel::geometry::sketch::Sketch;
use tpt_vertex_kernel::geometry::solid::Solid;
use tpt_vertex_kernel::math::{Vec2, Vec3};

fn rect_sketch(x0: f64, y0: f64, x1: f64, y1: f64) -> Sketch {
    let mut s = Sketch::new();
    s.line(Vec2::new(x0, y0), Vec2::new(x1, y0));
    s.line(Vec2::new(x1, y0), Vec2::new(x1, y1));
    s.line(Vec2::new(x1, y1), Vec2::new(x0, y1));
    s.line(Vec2::new(x0, y1), Vec2::new(x0, y0));
    s
}

/// A unit cube with its minimum corner at `(o, o, o)`.
fn unit_cube(o: f64) -> Solid {
    box_solid(Vec3::new(o, o, o), Vec3::new(o + 1.0, o + 1.0, o + 1.0))
}

fn has_nan(solid: &Solid) -> bool {
    solid
        .vertices
        .iter()
        .any(|v| !v.x.is_finite() || !v.y.is_finite() || !v.z.is_finite())
}

/// Vector area `∮ n dA` of a mesh. It is exactly zero for any geometrically
/// closed surface — including one with T-junctions, which mesh booleans
/// routinely produce — so it is a good watertightness proxy for BSP output.
fn vector_area(solid: &Solid) -> Vec3 {
    solid.faces.iter().fold(Vec3::ZERO, |acc, f| {
        let a = solid.vertices[f.a as usize];
        let b = solid.vertices[f.b as usize];
        let c = solid.vertices[f.c as usize];
        acc + (b - a).cross(c - a) * 0.5
    })
}

fn assert_closed(solid: &Solid, what: &str) {
    let area = solid.surface_area().max(1e-12);
    let residual = vector_area(solid).length() / area;
    assert!(
        residual < 1e-9,
        "{what} is not closed: |∮n dA| / area = {residual}"
    );
}

// ---- feature tree ----------------------------------------------------------

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
    // Real CSG union (ADR-0013): each prism is volume 4 and they overlap in a
    // 1x1x1 corner, so the union is 4 + 4 - 1 = 7 (the old placeholder simply
    // concatenated the meshes and reported 8).
    let vol = eval.features[&union].volume().abs();
    assert!((vol - 7.0).abs() < 1e-6, "union volume was {vol}");
}

#[test]
fn feature_tree_boolean_ops_use_the_bsp_engine() {
    for (op, expected) in [
        (tpt_vertex_kernel::feature_tree::BooleanOp::Union, 7.0),
        (tpt_vertex_kernel::feature_tree::BooleanOp::Subtract, 3.0),
        (tpt_vertex_kernel::feature_tree::BooleanOp::Intersect, 1.0),
    ] {
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
        let id = tree.add(Feature::Boolean { op, a, b }, None);
        let eval = tree.evaluate().unwrap();
        let vol = eval.features[&id].volume();
        assert!(
            (vol - expected).abs() < 1e-6,
            "{op:?} volume was {vol}, expected {expected}"
        );
    }
}

#[test]
fn feature_tree_fillet_and_chamfer_modify_geometry() {
    let mut tree = FeatureTree::new();
    let base = tree.add(
        Feature::Extrude {
            sketch: rect_sketch(0.0, 0.0, 2.0, 2.0),
            height: 2.0,
        },
        None,
    );
    let rounded = tree.add(
        Feature::Fillet {
            parent: base,
            radius: 0.2,
        },
        Some(base),
    );
    let eval = tree.evaluate().unwrap();
    let before = &eval.features[&base];
    let after = &eval.features[&rounded];
    assert!(after.volume() < before.volume());
    assert!(after.volume() > before.volume() * 0.9);
    assert!(after.vertex_count() > before.vertex_count());
    assert!(!has_nan(after));
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

// ---- BSP boolean engine (ADR-0013) -----------------------------------------

#[test]
fn bsp_union_two_cubes_volume() {
    let a = unit_cube(0.0);
    let b = unit_cube(0.5);
    let u = bsp_union(&a, &b);

    assert!(u.triangle_count() > 0);
    // Strictly between "one cube" and "two disjoint cubes": the overlap
    // (0.5^3 = 0.125) must be counted once, not twice.
    assert!(
        u.volume() > 1.0 && u.volume() < 2.0,
        "union volume was {}",
        u.volume()
    );
    assert!(
        (u.volume() - 1.875).abs() < 1e-6,
        "union volume was {}",
        u.volume()
    );
    // The union bounds must span both operands.
    let (min, max) = u.bounds().unwrap();
    assert!((min.x - 0.0).abs() < 1e-9 && (max.x - 1.5).abs() < 1e-9);
    assert_closed(&u, "union");
}

#[test]
fn bsp_subtract_cube_from_cube() {
    let a = unit_cube(0.0);

    // Partial overlap: volume is reduced by exactly the overlap.
    let partial = bsp_subtract(&a, &unit_cube(0.5));
    assert!(
        partial.volume() < a.volume(),
        "subtract did not reduce volume: {}",
        partial.volume()
    );
    assert!(
        (partial.volume() - 0.875).abs() < 1e-6,
        "A-B volume was {}",
        partial.volume()
    );
    assert_closed(&partial, "partial difference");

    // Fully enclosed by the tool: nothing is left.
    let swallowed = bsp_subtract(
        &a,
        &box_solid(Vec3::new(-1.0, -1.0, -1.0), Vec3::new(2.0, 2.0, 2.0)),
    );
    assert!(
        swallowed.volume().abs() < 1e-9,
        "fully-subtracted volume was {}",
        swallowed.volume()
    );

    // Tool fully inside the target: a closed internal void, so the volume drops
    // by exactly the void.
    let hollow = bsp_subtract(
        &box_solid(Vec3::ZERO, Vec3::new(4.0, 4.0, 4.0)),
        &box_solid(Vec3::new(1.0, 1.0, 1.0), Vec3::new(2.0, 2.0, 2.0)),
    );
    assert!(
        (hollow.volume() - 63.0).abs() < 1e-6,
        "hollowed volume was {}",
        hollow.volume()
    );
    assert_closed(&hollow, "hollowed block");
}

#[test]
fn bsp_intersect() {
    let a = unit_cube(0.0);
    let b = unit_cube(0.5);
    let i = csg_bsp::bsp_intersect(&a, &b);

    assert!(i.volume() > 0.0, "intersection was empty");
    assert!(i.volume() <= a.volume() + 1e-12);
    assert!(i.volume() <= b.volume() + 1e-12);
    assert!(
        (i.volume() - 0.125).abs() < 1e-6,
        "intersection volume was {}",
        i.volume()
    );
    // The intersection lives entirely inside the overlap region.
    let (min, max) = i.bounds().unwrap();
    assert!(min.x >= 0.5 - 1e-9 && max.x <= 1.0 + 1e-9);
    assert_closed(&i, "intersection");

    // Non-overlapping operands intersect to nothing.
    assert_eq!(
        csg_bsp::bsp_intersect(&a, &unit_cube(5.0)).triangle_count(),
        0
    );
}

#[test]
fn bsp_union_of_disjoint_solids_keeps_both() {
    let a = unit_cube(0.0);
    let b = unit_cube(5.0);
    let u = bsp_union(&a, &b);
    assert_eq!(u.triangle_count(), a.triangle_count() + b.triangle_count());
    assert!((u.volume() - 2.0).abs() < 1e-9);
}

#[test]
fn fillet_chamfer_returns_valid_solid() {
    let cube = unit_cube(0.0);
    let edges = classify_edges(&cube);
    assert_eq!(
        edges.iter().filter(|e| e.kind == EdgeKind::Convex).count(),
        12,
        "a cube should expose 12 convex edges"
    );

    for (label, out) in [
        ("fillet", fillet_edges(&cube, 0.1, &[])),
        ("chamfer", chamfer_edges(&cube, 0.1, &[])),
    ] {
        assert!(!has_nan(&out), "{label} produced NaN positions");
        assert!(out.triangle_count() > 0, "{label} produced no triangles");
        assert!(
            out.vertex_count() > cube.vertex_count(),
            "{label} did not add vertices ({} -> {})",
            cube.vertex_count(),
            out.vertex_count()
        );
        let v = out.volume();
        assert!(v.is_finite(), "{label} volume was not finite");
        assert!(
            v < cube.volume() && v > 0.9 * cube.volume(),
            "{label} volume {v} is out of the expected range"
        );
        assert!(out.surface_area().is_finite());
        assert_closed(&out, label);
    }

    // Filleting a single selected edge only affects that edge.
    let one = fillet_edges(&cube, 0.1, &[0]);
    assert!(one.volume().is_finite() && one.volume() > 0.98);

    // Pathological inputs must not panic.
    assert!(fillet_edges(&cube, 0.0, &[]).volume().is_finite());
    assert!(chamfer_edges(&cube, -1.0, &[]).volume().is_finite());
    assert!(fillet_edges(&Solid::new(), 1.0, &[]).triangle_count() == 0);
    assert!(fillet_edges(&cube, 100.0, &[]).volume().is_finite());
}

#[test]
fn booleans_watertight_or_nan_free() {
    let a = unit_cube(0.0);
    // Deliberately awkward operands: an offset cube (generic position), a cube
    // sharing a face plane (coplanar case), and a fully-enclosing cube.
    let cases: Vec<(&str, Solid)> = vec![
        ("offset", unit_cube(0.37)),
        (
            "coplanar-face",
            box_solid(Vec3::new(0.25, 0.25, 0.0), Vec3::new(0.75, 0.75, 0.5)),
        ),
        (
            "enclosing",
            box_solid(Vec3::new(-2.0, -2.0, -2.0), Vec3::new(3.0, 3.0, 3.0)),
        ),
    ];

    for (label, b) in cases {
        for (op, out) in [
            ("union", bsp_union(&a, &b)),
            ("subtract", bsp_subtract(&a, &b)),
            ("intersect", csg_bsp::bsp_intersect(&a, &b)),
        ] {
            assert!(
                !has_nan(&out),
                "{op} with {label} produced non-finite vertex positions"
            );
            assert!(
                out.volume().is_finite(),
                "{op} with {label} produced a non-finite volume"
            );
            assert!(
                out.surface_area().is_finite(),
                "{op} with {label} produced a non-finite area"
            );
            for f in &out.faces {
                let n = out.vertex_count() as u32;
                assert!(
                    f.a < n && f.b < n && f.c < n,
                    "{op} with {label} produced an out-of-range face index"
                );
            }
        }
    }

    // Fillet/chamfer outputs are held to the same standard.
    for s in [fillet_edges(&a, 0.05, &[]), chamfer_edges(&a, 0.05, &[])] {
        assert!(!has_nan(&s));
        assert!(s.volume().is_finite());
    }
}
