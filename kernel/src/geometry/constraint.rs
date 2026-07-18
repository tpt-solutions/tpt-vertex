//! 2D sketch constraint solver.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! A lightweight iterative constraint solver for 2D sketches. It uses a
//! Gauss-Seidel / projection relaxation scheme: each constraint computes a
//! correction that moves the involved points toward satisfaction, and we
//! iterate until the residual falls below a tolerance or a max iteration
//! count is reached. This is robust, simple, and well-suited to interactive
//! sketch editing (it degrades gracefully rather than failing hard).
//!
//! The solver treats every [`Point`](crate::geometry::sketch::Point) as a free
//! variable (2 DOF). Fixed/reference geometry can be pinned via
//! [`Constraint::Fixed`].

use crate::geometry::sketch::{Sketch, VertexId};
use crate::math::Vec2;
use crate::tolerance::EPSILON;

/// Strength of a constraint. `value` constraints (dimensions) are stiff; soft
/// constraints are relaxed for stability.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Constraint {
    pub kind: ConstraintKind,
    /// Relative weight; higher = satisfied more aggressively.
    pub weight: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConstraintKind {
    /// Two points coincide.
    Coincident(VertexId, VertexId),
    /// Two lines (by their endpoint ids) are parallel.
    Parallel(VertexId, VertexId, VertexId, VertexId),
    /// Two lines are perpendicular.
    Perpendicular(VertexId, VertexId, VertexId, VertexId),
    /// A point lies a fixed distance from another point (dimensional).
    Distance(VertexId, VertexId, f64),
    /// A line is horizontal.
    Horizontal(VertexId, VertexId),
    /// A line is vertical.
    Vertical(VertexId, VertexId),
    /// Two line segments have equal length.
    EqualLength(VertexId, VertexId, VertexId, VertexId),
    /// A point lies on a circle (center + radius point).
    PointOnCircle(VertexId, VertexId, VertexId),
    /// A point is pinned to a fixed location.
    Fixed(VertexId, Vec2),
    /// Two circles (center + radius point each) have equal radius.
    EqualRadius(VertexId, VertexId, VertexId, VertexId),
}

impl Constraint {
    pub fn coincident(a: VertexId, b: VertexId) -> Self {
        Constraint {
            kind: ConstraintKind::Coincident(a, b),
            weight: 1.0,
        }
    }
    pub fn distance(a: VertexId, b: VertexId, d: f64) -> Self {
        Constraint {
            kind: ConstraintKind::Distance(a, b, d),
            weight: 1.0,
        }
    }
    pub fn fixed(a: VertexId, p: Vec2) -> Self {
        Constraint {
            kind: ConstraintKind::Fixed(a, p),
            weight: 1.0,
        }
    }
    pub fn horizontal(a: VertexId, b: VertexId) -> Self {
        Constraint {
            kind: ConstraintKind::Horizontal(a, b),
            weight: 1.0,
        }
    }
    pub fn vertical(a: VertexId, b: VertexId) -> Self {
        Constraint {
            kind: ConstraintKind::Vertical(a, b),
            weight: 1.0,
        }
    }
    pub fn parallel(a: VertexId, b: VertexId, c: VertexId, d: VertexId) -> Self {
        Constraint {
            kind: ConstraintKind::Parallel(a, b, c, d),
            weight: 1.0,
        }
    }
    pub fn perpendicular(a: VertexId, b: VertexId, c: VertexId, d: VertexId) -> Self {
        Constraint {
            kind: ConstraintKind::Perpendicular(a, b, c, d),
            weight: 1.0,
        }
    }
    pub fn equal_length(a: VertexId, b: VertexId, c: VertexId, d: VertexId) -> Self {
        Constraint {
            kind: ConstraintKind::EqualLength(a, b, c, d),
            weight: 1.0,
        }
    }
    pub fn point_on_circle(p: VertexId, c: VertexId, r: VertexId) -> Self {
        Constraint {
            kind: ConstraintKind::PointOnCircle(p, c, r),
            weight: 1.0,
        }
    }
    pub fn equal_radius(c1: VertexId, r1: VertexId, c2: VertexId, r2: VertexId) -> Self {
        Constraint {
            kind: ConstraintKind::EqualRadius(c1, r1, c2, r2),
            weight: 1.0,
        }
    }
}

/// Result of a solve.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SolveStats {
    pub iterations: usize,
    /// Max per-constraint residual after solving (0 = fully satisfied).
    pub residual: f64,
    pub converged: bool,
}

/// Solve `constraints` against the points in `sketch`, mutating positions.
///
/// `max_iters` bounds the relaxation; `tol` is the convergence threshold on the
/// maximum residual. Returns stats describing the outcome.
pub fn solve(
    sketch: &mut Sketch,
    constraints: &[Constraint],
    max_iters: usize,
    tol: f64,
) -> SolveStats {
    let mut residual = f64::MAX;
    let mut iters = 0;
    for it in 0..max_iters {
        iters = it + 1;
        residual = 0.0;
        for c in constraints {
            residual = residual.max(apply_constraint(sketch, c));
        }
        if residual <= tol {
            return SolveStats {
                iterations: iters,
                residual,
                converged: true,
            };
        }
    }
    SolveStats {
        iterations: iters,
        residual,
        converged: false,
    }
}

/// Apply one constraint projection, returning the residual (unsatisfied amount).
fn apply_constraint(sketch: &mut Sketch, c: &Constraint) -> f64 {
    let w = c.weight;
    let res = match c.kind {
        ConstraintKind::Fixed(a, target) => {
            if let Some(p) = sketch.points.iter_mut().find(|p| p.id == a) {
                let d = target - p.pos;
                p.pos = p.pos + d * w;
                d.length()
            } else {
                0.0
            }
        }
        ConstraintKind::Coincident(a, b) => {
            let (pa, pb) = get_two(sketch, a, b);
            if let (Some(pa), Some(pb)) = (pa, pb) {
                let mid = (pa + pb) * 0.5;
                move_point(sketch, a, mid, w);
                move_point(sketch, b, mid, w);
                pa.distance(pb)
            } else {
                0.0
            }
        }
        ConstraintKind::Distance(a, b, target) => {
            let (pa, pb) = get_two(sketch, a, b);
            if let (Some(pa), Some(pb)) = (pa, pb) {
                let d = pb - pa;
                let len = d.length().max(EPSILON);
                let diff = len - target;
                let dir = d * (1.0 / len);
                move_point(sketch, a, pa + dir * (diff * 0.5 * w), 1.0);
                move_point(sketch, b, pb - dir * (diff * 0.5 * w), 1.0);
                diff.abs()
            } else {
                0.0
            }
        }
        ConstraintKind::Horizontal(a, b) => {
            let (pa, pb) = get_two(sketch, a, b);
            if let (Some(pa), Some(pb)) = (pa, pb) {
                let midy = (pa.y + pb.y) * 0.5;
                move_point(sketch, a, Vec2::new(pa.x, midy), w);
                move_point(sketch, b, Vec2::new(pb.x, midy), w);
                (pa.y - pb.y).abs()
            } else {
                0.0
            }
        }
        ConstraintKind::Vertical(a, b) => {
            let (pa, pb) = get_two(sketch, a, b);
            if let (Some(pa), Some(pb)) = (pa, pb) {
                let midx = (pa.x + pb.x) * 0.5;
                move_point(sketch, a, Vec2::new(midx, pa.y), w);
                move_point(sketch, b, Vec2::new(midx, pb.y), w);
                (pa.x - pb.x).abs()
            } else {
                0.0
            }
        }
        ConstraintKind::Parallel(a, b, c, d) => {
            // Make direction (c-d) parallel to (a-b) by rotating (c-d) onto (a-b).
            let (pa, pb, pc, pd) = get_four(sketch, a, b, c, d);
            if let (Some(pa), Some(pb), Some(pc), Some(pd)) = (pa, pb, pc, pd) {
                let v1 = (pb - pa).normalize();
                let v2 = pd - pc;
                let len2 = v2.length().max(EPSILON);
                let target = v1 * len2;
                // rotate v2 onto v1 preserving length; project midpoint fixed.
                let mid = (pc + pd) * 0.5;
                move_point(sketch, c, mid - target * 0.5, w);
                move_point(sketch, d, mid + target * 0.5, w);
                let new_v = (pd - pc).normalize();
                1.0 - new_v.dot(v1).abs()
            } else {
                0.0
            }
        }
        ConstraintKind::Perpendicular(a, b, c, d) => {
            let (pa, pb, pc, pd) = get_four(sketch, a, b, c, d);
            if let (Some(pa), Some(pb), Some(pc), Some(pd)) = (pa, pb, pc, pd) {
                let v1 = (pb - pa).normalize();
                let perp = Vec2::new(-v1.y, v1.x);
                let len2 = (pd - pc).length().max(EPSILON);
                let mid = (pc + pd) * 0.5;
                move_point(sketch, c, mid - perp * (len2 * 0.5), w);
                move_point(sketch, d, mid + perp * (len2 * 0.5), w);
                let new_v = (pd - pc).normalize();
                new_v.dot(v1).abs()
            } else {
                0.0
            }
        }
        ConstraintKind::EqualLength(a, b, c, d) => {
            let (pa, pb, pc, pd) = get_four(sketch, a, b, c, d);
            if let (Some(pa), Some(pb), Some(pc), Some(pd)) = (pa, pb, pc, pd) {
                let l1 = (pb - pa).length();
                let l2 = (pd - pc).length().max(EPSILON);
                let avg = (l1 + l2) * 0.5;
                let dir1 = (pb - pa).normalize();
                let dir2 = (pd - pc).normalize();
                let m1 = (pa + pb) * 0.5;
                let m2 = (pc + pd) * 0.5;
                move_point(sketch, a, m1 - dir1 * (avg * 0.5), w);
                move_point(sketch, b, m1 + dir1 * (avg * 0.5), w);
                move_point(sketch, c, m2 - dir2 * (avg * 0.5), w);
                move_point(sketch, d, m2 + dir2 * (avg * 0.5), w);
                (l1 - l2).abs()
            } else {
                0.0
            }
        }
        ConstraintKind::PointOnCircle(p, c, r) => {
            let (pp, pc, pr) = get_three(sketch, p, c, r);
            if let (Some(pp), Some(pc), Some(pr)) = (pp, pc, pr) {
                let radius = (pr - pc).length().max(EPSILON);
                let d = pp - pc;
                let len = d.length().max(EPSILON);
                let target = pc + d * (radius / len);
                move_point(sketch, p, target, w);
                (len - radius).abs()
            } else {
                0.0
            }
        }
        ConstraintKind::EqualRadius(c1, r1, c2, r2) => {
            let (pc1, pr1, pc2, pr2) = get_four(sketch, c1, r1, c2, r2);
            if let (Some(pc1), Some(pr1), Some(pc2), Some(pr2)) = (pc1, pr1, pc2, pr2) {
                let rad1 = (pr1 - pc1).length();
                let rad2 = (pr2 - pc2).length().max(EPSILON);
                let avg = (rad1 + rad2) * 0.5;
                let dir1 = (pr1 - pc1).normalize();
                let dir2 = (pr2 - pc2).normalize();
                move_point(sketch, r1, pc1 + dir1 * avg, w);
                move_point(sketch, r2, pc2 + dir2 * avg, w);
                (rad1 - rad2).abs()
            } else {
                0.0
            }
        }
    };
    res
}

fn get_two(s: &Sketch, a: VertexId, b: VertexId) -> (Option<Vec2>, Option<Vec2>) {
    let pa = s.point(a).map(|p| p.pos);
    let pb = s.point(b).map(|p| p.pos);
    (pa, pb)
}

fn get_three(
    s: &Sketch,
    a: VertexId,
    b: VertexId,
    c: VertexId,
) -> (Option<Vec2>, Option<Vec2>, Option<Vec2>) {
    (
        s.point(a).map(|p| p.pos),
        s.point(b).map(|p| p.pos),
        s.point(c).map(|p| p.pos),
    )
}

fn get_four(
    s: &Sketch,
    a: VertexId,
    b: VertexId,
    c: VertexId,
    d: VertexId,
) -> (Option<Vec2>, Option<Vec2>, Option<Vec2>, Option<Vec2>) {
    (
        s.point(a).map(|p| p.pos),
        s.point(b).map(|p| p.pos),
        s.point(c).map(|p| p.pos),
        s.point(d).map(|p| p.pos),
    )
}

fn move_point(s: &mut Sketch, id: VertexId, to: Vec2, w: f64) {
    if let Some(p) = s.points.iter_mut().find(|p| p.id == id) {
        p.pos = p.pos + (to - p.pos) * w;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::sketch::Sketch;

    #[test]
    fn distance_constraint() {
        let mut s = Sketch::new();
        let a = s.add_point(Vec2::ZERO);
        let b = s.add_point(Vec2::new(2.0, 0.0));
        let cons = [Constraint::distance(a, b, 5.0)];
        let stats = solve(&mut s, &cons, 100, 1e-9);
        assert!(stats.converged);
        assert!((s.point(a).unwrap().pos.distance(s.point(b).unwrap().pos) - 5.0).abs() < 1e-6);
    }

    #[test]
    fn coincident_constraint() {
        let mut s = Sketch::new();
        let a = s.add_point(Vec2::new(1.0, 1.0));
        let b = s.add_point(Vec2::new(4.0, 2.0));
        let cons = [Constraint::coincident(a, b)];
        let stats = solve(&mut s, &cons, 50, 1e-9);
        assert!(stats.converged);
        let pa = s.point(a).unwrap().pos;
        let pb = s.point(b).unwrap().pos;
        assert!(pa.distance(pb) < 1e-6);
    }

    #[test]
    fn perpendicular_lines() {
        let mut s = Sketch::new();
        let a = s.add_point(Vec2::ZERO);
        let b = s.add_point(Vec2::new(1.0, 0.0));
        let c = s.add_point(Vec2::new(0.0, 0.0));
        let d = s.add_point(Vec2::new(0.0, 1.0));
        let cons = [Constraint::perpendicular(a, b, c, d)];
        let stats = solve(&mut s, &cons, 50, 1e-9);
        assert!(stats.converged);
    }

    #[test]
    fn fixed_pin_holds() {
        let mut s = Sketch::new();
        let a = s.add_point(Vec2::new(3.0, 3.0));
        let b = s.add_point(Vec2::new(3.0, 3.0));
        let cons = [
            Constraint::fixed(a, Vec2::new(0.0, 0.0)),
            Constraint::coincident(a, b),
        ];
        let stats = solve(&mut s, &cons, 50, 1e-9);
        assert!(stats.converged);
        // a is pinned to origin; b snaps to it.
        assert!(s.point(a).unwrap().pos.distance(Vec2::ZERO) < 1e-6);
    }

    #[test]
    fn equal_length() {
        let mut s = Sketch::new();
        let a = s.add_point(Vec2::ZERO);
        let b = s.add_point(Vec2::new(2.0, 0.0));
        let c = s.add_point(Vec2::new(0.0, 5.0));
        let d = s.add_point(Vec2::new(3.0, 5.0));
        let cons = [Constraint::equal_length(a, b, c, d)];
        let stats = solve(&mut s, &cons, 100, 1e-9);
        assert!(stats.converged);
        let l1 = s.point(a).unwrap().pos.distance(s.point(b).unwrap().pos);
        let l2 = s.point(c).unwrap().pos.distance(s.point(d).unwrap().pos);
        assert!((l1 - l2).abs() < 1e-6);
    }
}
