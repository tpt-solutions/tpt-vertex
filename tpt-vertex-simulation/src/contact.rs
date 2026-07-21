//! Contact / interference detection between assembly parts during motion.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! For each pair of parts in an [`Assembly`] at its current pose, an AABB
//! broad phase quickly rules out far-apart pairs; a brute-force
//! triangle-triangle narrow phase (Möller's 1997 test) then decides whether
//! the meshes actually interpenetrate. This is a v1 "any contact yes/no"
//! detector intended for motion-study collision flags, not a
//! penetration-depth/response solver — a documented fast-follow, in the same
//! spirit as the crate's other v1 geometric kernels.

use tpt_vertex_kernel::assembly::{Assembly, PartId};
use tpt_vertex_kernel::geometry::solid::Solid;
use tpt_vertex_kernel::math::Vec3;

/// Axis-aligned bounding box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    pub fn of_solid(solid: &Solid) -> Option<Aabb> {
        solid.bounds().map(|(min, max)| Aabb { min, max })
    }

    /// True when the two boxes share positive volume, not merely a touching
    /// boundary. Two boxes that meet exactly at a face (zero-width contact,
    /// e.g. two mating parts) have zero overlap volume and are *not*
    /// considered interfering — this is the common, legitimate "touching"
    /// assembly case, not a clash. Using `<=`/`>=` here would also flag that
    /// as an overlap and send it into the narrow phase, where two exactly
    /// coincident boundary faces register as a full-area triangle
    /// intersection even though the solids don't actually interpenetrate.
    pub fn overlaps(&self, other: &Aabb) -> bool {
        self.min.x < other.max.x
            && self.max.x > other.min.x
            && self.min.y < other.max.y
            && self.max.y > other.min.y
            && self.min.z < other.max.z
            && self.max.z > other.min.z
    }

    /// Gap (0 if overlapping) between two boxes along the separating axes.
    pub fn gap(&self, other: &Aabb) -> f64 {
        let dx = (self.min.x - other.max.x).max(other.min.x - self.max.x).max(0.0);
        let dy = (self.min.y - other.max.y).max(other.min.y - self.max.y).max(0.0);
        let dz = (self.min.z - other.max.z).max(other.min.z - self.max.z).max(0.0);
        (dx * dx + dy * dy + dz * dz).sqrt()
    }
}

/// Interference result for one pair of parts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContactPair {
    pub a: PartId,
    pub b: PartId,
    pub interfering: bool,
    /// Approximate clearance (mm) when not interfering; `0.0` when
    /// interfering. This is a coarse bound (AABB gap, or nearest-vertex
    /// distance once bounding boxes overlap), not an exact minimum distance.
    pub clearance: f64,
}

/// Detect interference between every pair of parts in `assembly` at its
/// current pose (i.e. call this once per motion-study frame).
pub fn detect_interference(assembly: &Assembly) -> Vec<ContactPair> {
    let solids: Vec<(PartId, Solid)> = assembly
        .parts()
        .iter()
        .map(|(id, p)| (*id, p.solid_in_assembly()))
        .collect();

    let mut out = Vec::new();
    for i in 0..solids.len() {
        for j in (i + 1)..solids.len() {
            let (ida, sa) = &solids[i];
            let (idb, sb) = &solids[j];
            out.push(check_pair(*ida, sa, *idb, sb));
        }
    }
    out
}

fn check_pair(a: PartId, sa: &Solid, b: PartId, sb: &Solid) -> ContactPair {
    let (Some(ba), Some(bb)) = (Aabb::of_solid(sa), Aabb::of_solid(sb)) else {
        return ContactPair { a, b, interfering: false, clearance: f64::INFINITY };
    };
    if !ba.overlaps(&bb) {
        return ContactPair { a, b, interfering: false, clearance: ba.gap(&bb) };
    }
    for fa in &sa.faces {
        let (a0, a1, a2) = (
            sa.vertices[fa.a as usize],
            sa.vertices[fa.b as usize],
            sa.vertices[fa.c as usize],
        );
        for fb in &sb.faces {
            let (b0, b1, b2) = (
                sb.vertices[fb.a as usize],
                sb.vertices[fb.b as usize],
                sb.vertices[fb.c as usize],
            );
            if tri_tri_intersect(a0, a1, a2, b0, b1, b2) {
                return ContactPair { a, b, interfering: true, clearance: 0.0 };
            }
        }
    }
    ContactPair { a, b, interfering: false, clearance: min_vertex_distance(sa, sb) }
}

fn min_vertex_distance(sa: &Solid, sb: &Solid) -> f64 {
    let mut best = f64::INFINITY;
    for va in &sa.vertices {
        for vb in &sb.vertices {
            best = best.min(va.distance(*vb));
        }
    }
    best
}

/// Möller's triangle-triangle intersection test: reject via each triangle's
/// plane, then check for overlap of the two triangles' intervals along the
/// line where the two planes meet.
fn tri_tri_intersect(v0: Vec3, v1: Vec3, v2: Vec3, u0: Vec3, u1: Vec3, u2: Vec3) -> bool {
    const EPS: f64 = 1e-9;

    let n2 = (u1 - u0).cross(u2 - u0);
    let d2 = -n2.dot(u0);
    let dv0 = n2.dot(v0) + d2;
    let dv1 = n2.dot(v1) + d2;
    let dv2 = n2.dot(v2) + d2;
    if same_sign_nonzero(dv0, dv1, dv2, EPS) {
        return false;
    }

    let n1 = (v1 - v0).cross(v2 - v0);
    let d1 = -n1.dot(v0);
    let du0 = n1.dot(u0) + d1;
    let du1 = n1.dot(u1) + d1;
    let du2 = n1.dot(u2) + d1;
    if same_sign_nonzero(du0, du1, du2, EPS) {
        return false;
    }

    let dvec = n1.cross(n2);
    let (ax, ay, az) = (dvec.x.abs(), dvec.y.abs(), dvec.z.abs());
    let maxc = ax.max(ay).max(az);
    if maxc < 1e-12 {
        return coplanar_tri_tri(v0, v1, v2, u0, u1, u2, n1);
    }
    let proj = |p: Vec3| -> f64 {
        if maxc == ax {
            p.x
        } else if maxc == ay {
            p.y
        } else {
            p.z
        }
    };

    let (t1a, t1b) = interval(
        [proj(v0), proj(v1), proj(v2)],
        [dv0, dv1, dv2],
    );
    let (t2a, t2b) = interval(
        [proj(u0), proj(u1), proj(u2)],
        [du0, du1, du2],
    );
    let (t1min, t1max) = (t1a.min(t1b), t1a.max(t1b));
    let (t2min, t2max) = (t2a.min(t2b), t2a.max(t2b));
    t1min <= t2max + EPS && t2min <= t1max + EPS
}

fn same_sign_nonzero(a: f64, b: f64, c: f64, eps: f64) -> bool {
    let s = |x: f64| -> i32 {
        if x > eps {
            1
        } else if x < -eps {
            -1
        } else {
            0
        }
    };
    let (sa, sb, sc) = (s(a), s(b), s(c));
    sa != 0 && sa == sb && sb == sc
}

/// Find the vertex alone on one side of the other triangle's plane (the
/// "apex"), and interpolate the two edges from apex to the base vertices at
/// their zero-crossings, returning the projected interval endpoints.
fn interval(p: [f64; 3], d: [f64; 3]) -> (f64, f64) {
    for apex in 0..3 {
        let b1 = (apex + 1) % 3;
        let b2 = (apex + 2) % 3;
        if (d[apex] >= 0.0) != (d[b1] >= 0.0) && (d[apex] >= 0.0) != (d[b2] >= 0.0) {
            let t1 = p[apex] + (p[b1] - p[apex]) * (d[apex] / (d[apex] - d[b1]));
            let t2 = p[apex] + (p[b2] - p[apex]) * (d[apex] / (d[apex] - d[b2]));
            return (t1, t2);
        }
    }
    (p[0], p[0])
}

/// Coplanar fallback: project both triangles onto the plane with normal
/// `n` (dropping the dominant axis) and test 2D triangle overlap via
/// vertex-in-triangle and edge-edge intersection checks.
fn coplanar_tri_tri(v0: Vec3, v1: Vec3, v2: Vec3, u0: Vec3, u1: Vec3, u2: Vec3, n: Vec3) -> bool {
    let (ax, ay, az) = (n.x.abs(), n.y.abs(), n.z.abs());
    let drop_z = ax.max(ay).max(az) == az;
    let drop_y = !drop_z && ay >= ax;
    let p2 = |p: Vec3| -> (f64, f64) {
        if drop_z {
            (p.x, p.y)
        } else if drop_y {
            (p.x, p.z)
        } else {
            (p.y, p.z)
        }
    };
    let t1 = [p2(v0), p2(v1), p2(v2)];
    let t2 = [p2(u0), p2(u1), p2(u2)];

    for p in t1.iter() {
        if point_in_tri_2d(*p, &t2) {
            return true;
        }
    }
    for p in t2.iter() {
        if point_in_tri_2d(*p, &t1) {
            return true;
        }
    }
    for i in 0..3 {
        for j in 0..3 {
            if seg_seg_intersect_2d(t1[i], t1[(i + 1) % 3], t2[j], t2[(j + 1) % 3]) {
                return true;
            }
        }
    }
    false
}

fn cross2(o: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0)
}

fn point_in_tri_2d(p: (f64, f64), t: &[(f64, f64); 3]) -> bool {
    let d1 = cross2(t[0], t[1], p);
    let d2 = cross2(t[1], t[2], p);
    let d3 = cross2(t[2], t[0], p);
    let has_neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let has_pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(has_neg && has_pos)
}

fn seg_seg_intersect_2d(p1: (f64, f64), p2: (f64, f64), p3: (f64, f64), p4: (f64, f64)) -> bool {
    let d1 = cross2(p3, p4, p1);
    let d2 = cross2(p3, p4, p2);
    let d3 = cross2(p1, p2, p3);
    let d4 = cross2(p1, p2, p4);
    ((d1 > 0.0) != (d2 > 0.0)) && ((d3 > 0.0) != (d4 > 0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separated_triangles_do_not_intersect() {
        let a = (Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0));
        let b = (Vec3::new(10.0, 0.0, 0.0), Vec3::new(11.0, 0.0, 0.0), Vec3::new(10.0, 1.0, 0.0));
        assert!(!tri_tri_intersect(a.0, a.1, a.2, b.0, b.1, b.2));
    }

    #[test]
    fn crossing_triangles_intersect() {
        // Triangle in the XY plane, another that pierces straight through it.
        let a = (
            Vec3::new(-1.0, -1.0, 0.0),
            Vec3::new(2.0, -1.0, 0.0),
            Vec3::new(-1.0, 2.0, 0.0),
        );
        let b = (
            Vec3::new(0.0, 0.0, -1.0),
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(0.5, 0.5, 0.0),
        );
        assert!(tri_tri_intersect(a.0, a.1, a.2, b.0, b.1, b.2));
    }

    #[test]
    fn aabb_overlap_and_gap() {
        let a = Aabb { min: Vec3::ZERO, max: Vec3::new(1.0, 1.0, 1.0) };
        let b = Aabb { min: Vec3::new(0.5, 0.5, 0.5), max: Vec3::new(2.0, 2.0, 2.0) };
        assert!(a.overlaps(&b));
        assert_eq!(a.gap(&b), 0.0);
        let c = Aabb { min: Vec3::new(5.0, 0.0, 0.0), max: Vec3::new(6.0, 1.0, 1.0) };
        assert!(!a.overlaps(&c));
        assert!((a.gap(&c) - 4.0).abs() < 1e-9);
    }

    fn cube(center: Vec3, half: f64) -> Solid {
        let mut s = Solid::new();
        let h = half;
        let mut v = |x: f64, y: f64, z: f64| s.add_vertex(center + Vec3::new(x, y, z));
        let p = [
            v(-h, -h, -h), v(h, -h, -h), v(h, h, -h), v(-h, h, -h),
            v(-h, -h, h), v(h, -h, h), v(h, h, h), v(-h, h, h),
        ];
        let mut f = |a: u32, b: u32, c: u32| s.faces.push(tpt_vertex_kernel::geometry::solid::Face::new(a, b, c));
        f(p[0], p[1], p[2]); f(p[0], p[2], p[3]);
        f(p[4], p[6], p[5]); f(p[4], p[7], p[6]);
        f(p[0], p[5], p[1]); f(p[0], p[4], p[5]);
        f(p[1], p[6], p[2]); f(p[1], p[5], p[6]);
        f(p[2], p[7], p[3]); f(p[2], p[6], p[7]);
        f(p[3], p[4], p[0]); f(p[3], p[7], p[4]);
        s
    }

    #[test]
    fn overlapping_cubes_interfere() {
        let a = cube(Vec3::ZERO, 1.0);
        let b = cube(Vec3::new(1.0, 0.0, 0.0), 1.0);
        assert!(check_pair(PartId(0), &a, PartId(1), &b).interfering);
    }

    #[test]
    fn far_cubes_do_not_interfere() {
        let a = cube(Vec3::ZERO, 1.0);
        let b = cube(Vec3::new(10.0, 0.0, 0.0), 1.0);
        let pair = check_pair(PartId(0), &a, PartId(1), &b);
        assert!(!pair.interfering);
        assert!(pair.clearance > 5.0);
    }

    #[test]
    fn touching_but_not_overlapping_cubes_do_not_interfere() {
        // Faces share the plane x=1 exactly but don't overlap volumetrically.
        let a = cube(Vec3::ZERO, 1.0);
        let b = cube(Vec3::new(2.0, 0.0, 0.0), 1.0);
        let pair = check_pair(PartId(0), &a, PartId(1), &b);
        assert!(!pair.interfering);
    }
}
