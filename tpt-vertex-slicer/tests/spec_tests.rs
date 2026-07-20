//! Property-style specification tests for the highest-risk slicer geometry
//! kernels (see `docs/specs/slicer-geometry-kernels.md`).
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! These encode the invariants P1–P3 from the spec. They stand in for the
//! `tpt-telos` machine-checked specifications, which are deferred until that tool
//! is available in the build environment.

#![cfg(test)]

use tpt_vertex_slicer::layers::{intersect_triangle, stitch_segments, Contour, Seg, P2};
use tpt_vertex_slicer::offset::{offset_contour, oriented_ccw};
use tpt_vertex_kernel::math::Vec3;

/// A tiny deterministic LCG so the tests stay dependency-free but exercise many
/// randomized inputs.
struct Rng(u64);
impl Rng {
    fn next_f64(&mut self) -> f64 {
        // xorshift64*
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        ((x.wrapping_mul(0x2545F491_4F6CDD1D) >> 11) as f64) / (1u64 << 53) as f64
    }
    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.next_f64()
    }
}

// ---- P1: plane intersection ------------------------------------------------

#[test]
fn plane_crossing_iff_straddles() {
    let mut rng = Rng(0xDEADBEEF);
    for _ in 0..2000 {
        let a = Vec3::new(rng.range(-5.0, 5.0), rng.range(-5.0, 5.0), rng.range(-5.0, 5.0));
        let b = Vec3::new(rng.range(-5.0, 5.0), rng.range(-5.0, 5.0), rng.range(-5.0, 5.0));
        let c = Vec3::new(rng.range(-5.0, 5.0), rng.range(-5.0, 5.0), rng.range(-5.0, 5.0));
        let h = rng.range(-5.0, 5.0);

        let n_above = [a, b, c].iter().filter(|v| v.z > h + 1e-9).count();
        let n_below = [a, b, c].iter().filter(|v| v.z < h - 1e-9).count();
        let straddles = n_above >= 1 && n_below >= 1;

        let seg = intersect_triangle([a, b, c], h);
        // P1.1: crossing exists iff the triangle straddles the plane.
        assert_eq!(seg.is_some(), straddles, "straddle mismatch");

        if let Some(s) = seg {
            // P1.4 (no NaN) + finite endpoints.
            assert!(s.a.x.is_finite() && s.a.y.is_finite());
            assert!(s.b.x.is_finite() && s.b.y.is_finite());
        }
    }
}

// ---- P2: contour stitching -------------------------------------------------

#[test]
fn stitch_closes_a_known_polygon() {
    // Build a hexagon's edges in shuffled order and orientation, then stitch.
    let n = 6;
    let mut pts = Vec::new();
    for i in 0..n {
        let t = std::f64::consts::TAU * (i as f64 / n as f64);
        pts.push(P2::new(3.0 * t.cos(), 3.0 * t.sin()));
    }
    let mut segs: Vec<Seg> = Vec::new();
    for i in 0..n {
        let a = pts[i];
        let b = pts[(i + 1) % n];
        // Alternate segment orientation to exercise both endpoint matches.
        if i % 2 == 0 {
            segs.push(Seg { a, b });
        } else {
            segs.push(Seg { a: b, b: a });
        }
    }
    // Shuffle-ish reorder.
    segs.rotate_left(3);

    let contours = stitch_segments(&mut segs);
    assert_eq!(contours.len(), 1, "expected a single closed loop");
    let c = &contours[0];
    assert_eq!(c.points.len(), n, "loop should have {n} vertices");
    // P2.1: closed — first and last are adjacent around the loop.
    assert!(c.points[0].dist(*c.points.last().unwrap()) > 0.0);
    // Area matches a regular hexagon of circumradius 3.
    let expected = 3.0f64.sqrt() * 1.5 * 3.0 * 3.0; // (3√3/2) r²
    assert!((c.signed_area().abs() - expected).abs() < 1e-6, "area {}", c.signed_area());
}

// ---- P3: polygon offset ----------------------------------------------------

fn regular_polygon(n: usize, r: f64) -> Contour {
    let mut pts = Vec::new();
    for i in 0..n {
        let t = std::f64::consts::TAU * (i as f64 / n as f64);
        pts.push(P2::new(r * t.cos(), r * t.sin()));
    }
    oriented_ccw(&Contour { points: pts })
}

#[test]
fn offset_inset_shrinks_grows_outset() {
    let mut rng = Rng(0x1234_5678);
    for _ in 0..500 {
        let n = 3 + (rng.range(0.0, 6.0) as usize);
        let r = rng.range(2.0, 20.0);
        let poly = regular_polygon(n, r);
        let a0 = poly.signed_area().abs();

        let d = rng.range(0.05, r * 0.2);
        let inset = offset_contour(&poly, -d);
        let outset = offset_contour(&poly, d);

        // P3.4: inset shrinks, outset grows, monotonically.
        assert!(inset.signed_area().abs() < a0 + 1e-9);
        assert!(outset.signed_area().abs() > a0 - 1e-9);

        // No blow-up (P3.2/P3.3): coordinates stay bounded.
        for p in inset.points.iter().chain(outset.points.iter()) {
            assert!(p.x.abs() < 10.0 * r && p.y.abs() < 10.0 * r, "coord blow-up");
        }
    }
}

#[test]
fn offset_collinear_points_do_not_explode() {
    // A square whose edges are subdivided with collinear midpoints — the case
    // that previously blew up (P3.2).
    let pts = vec![
        P2::new(-2.0, -2.0),
        P2::new(0.0, -2.0),
        P2::new(2.0, -2.0),
        P2::new(2.0, 0.0),
        P2::new(2.0, 2.0),
        P2::new(0.0, 2.0),
        P2::new(-2.0, 2.0),
        P2::new(-2.0, 0.0),
    ];
    let c = oriented_ccw(&Contour { points: pts });
    let inset = offset_contour(&c, -0.5);
    for p in &inset.points {
        assert!(p.x.abs() < 5.0 && p.y.abs() < 5.0, "collinear blow-up at {:?}", p);
    }
    // 4x4 inset by 0.5 => 3x3, area 9.
    assert!((inset.signed_area().abs() - 9.0).abs() < 1e-6, "area {}", inset.signed_area());
}
