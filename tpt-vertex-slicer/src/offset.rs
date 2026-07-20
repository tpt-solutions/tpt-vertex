//! Polygon offset/inset for perimeter and wall generation.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! A hand-rolled miter offset: each vertex is displaced by the standard miter
//! vector `d * (n1 + n2) / (1 + n1·n2)`, where `n1`/`n2` are the unit *outward*
//! normals of the incoming and outgoing edges. This is exact for straight/mildly
//! concave corners, degrades gracefully at collinear points (no coordinate
//! blow-up), and clamps sharp spikes. A dedicated polygon-offset library is a
//! documented fast-follow (see Phase 10 fast-follows).

use crate::layers::{Contour, P2};

/// Offset a contour by signed distance `d`: negative insets (shrinks), positive
/// offsets (grows). Assumes a counter-clockwise (outer) winding; the outward
/// normal is taken as the right normal of each directed edge.
pub fn offset_contour(c: &Contour, d: f64) -> Contour {
    let n = c.points.len();
    if n < 3 {
        return c.clone();
    }
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let prev = c.points[(i + n - 1) % n];
        let cur = c.points[i];
        let next = c.points[(i + 1) % n];

        let e1 = sub(cur, prev);
        let e2 = sub(next, cur);
        if len(e1) < 1e-12 || len(e2) < 1e-12 {
            out.push(cur);
            continue;
        }
        // Outward (right) normals of the incoming and outgoing edges for a CCW
        // polygon: rotate the edge direction -90° => (dy, -dx).
        let n1 = norm((e1.1, -e1.0));
        let n2 = norm((e2.1, -e2.0));

        // Miter vector. The denominator `1 + n1·n2` collapses toward 0 only for a
        // near-180° reversal (a spike); clamp it to bound the miter length.
        let denom = (1.0 + dot(n1, n2)).max(0.2);
        let mx = (n1.0 + n2.0) / denom;
        let my = (n1.1 + n2.1) / denom;

        out.push(P2::new(cur.x + d * mx, cur.y + d * my));
    }
    Contour { points: out }
}

/// Produce `count` nested insets (walls) for a contour, each offset by the
/// extrusion width. The first inset is the inner boundary of the outer wall;
/// subsequent insets form inner walls.
pub fn walls_for(c: &Contour, count: usize, width: f64) -> Vec<Contour> {
    let mut out = Vec::new();
    // Offset assumes CCW winding; orient the source contour accordingly so
    // negative distances reliably inset.
    let cur = oriented_ccw(c);
    for i in 0..count {
        let inset = offset_contour(&cur, -(width * (i as f64 + 0.5)));
        if inset.points.len() < 3 || inset.signed_area().abs() < width * width {
            break;
        }
        out.push(inset);
    }
    out
}

/// Return a copy of the contour wound counter-clockwise.
pub fn oriented_ccw(c: &Contour) -> Contour {
    if c.signed_area() < 0.0 {
        let mut pts = c.points.clone();
        pts.reverse();
        Contour { points: pts }
    } else {
        c.clone()
    }
}

fn sub(a: P2, b: P2) -> (f64, f64) {
    (a.x - b.x, a.y - b.y)
}
fn len(v: (f64, f64)) -> f64 {
    (v.0 * v.0 + v.1 * v.1).sqrt()
}
fn norm(v: (f64, f64)) -> (f64, f64) {
    let l = len(v);
    if l < 1e-12 {
        (0.0, 0.0)
    } else {
        (v.0 / l, v.1 / l)
    }
}
fn dot(a: (f64, f64), b: (f64, f64)) -> f64 {
    a.0 * b.0 + a.1 * b.1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(s: f64) -> Contour {
        Contour {
            points: vec![
                P2::new(-s, -s),
                P2::new(s, -s),
                P2::new(s, s),
                P2::new(-s, s),
            ],
        }
    }

    #[test]
    fn inset_shrinks_area() {
        let c = square(2.0);
        let inset = offset_contour(&c, -0.5);
        // 4x4 square (area 16) inset by 0.5 on each side => 3x3 (area 9).
        assert!((inset.signed_area().abs() - 9.0).abs() < 1e-6, "area {}", inset.signed_area());
    }

    #[test]
    fn walls_nest_inward() {
        let c = square(2.0);
        let walls = walls_for(&c, 2, 0.5);
        assert_eq!(walls.len(), 2);
        assert!(walls[1].signed_area().abs() < walls[0].signed_area().abs());
    }
}
