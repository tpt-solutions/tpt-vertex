//! Parametric feature operations producing solids from sketches.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! These implement the core modeling features of the kernel (extrude, revolve,
//! sweep, loft) plus boolean combinators (union/subtract/intersect) and
//! fillet/chamfer. Per ADR-0004 the feature tree is the source of truth and
//! these operations are pure functions over solids/sketches.

use crate::geometry::mesh::{sketch_boundary, tessellate_polygon_z};
use crate::geometry::sketch::Sketch;
use crate::geometry::solid::{Face, Solid};
use crate::math::Vec3;

/// Extrude a (planar, XY) sketch profile along `Z` by `height`. The profile is
/// treated as a closed loop of line/arc/circle entities (see
/// [`sketch_boundary`]). Produces a capped prism.
pub fn extrude(sketch: &Sketch, height: f64) -> Solid {
    let profile = sketch_boundary(sketch, 24);
    extrude_profile(&profile, height)
}

/// Extrude an explicit ordered 2D profile loop.
pub fn extrude_profile(profile: &[crate::math::Vec2], height: f64) -> Solid {
    let mut solid = Solid::new();
    if profile.len() < 3 {
        return solid;
    }
    let n = profile.len();
    // Bottom cap (z=0), top cap (z=height) via fan.
    let bottom: Vec<u32> = profile
        .iter()
        .map(|p| solid.add_vertex(Vec3::new(p.x, p.y, 0.0)))
        .collect();
    let top: Vec<u32> = profile
        .iter()
        .map(|p| solid.add_vertex(Vec3::new(p.x, p.y, height)))
        .collect();

    for i in 1..(n as u32 - 1) {
        // Bottom cap: outward normal -Z -> clockwise when viewed from +Z.
        solid.faces.push(Face::new(
            bottom[0],
            bottom[i as usize + 1],
            bottom[i as usize],
        ));
        // Top cap: outward normal +Z -> counter-clockwise from +Z.
        solid
            .faces
            .push(Face::new(top[0], top[i as usize], top[i as usize + 1]));
    }
    // Side walls.
    for i in 0..n {
        let j = (i + 1) % n;
        let (b0, b1) = (bottom[i], bottom[j]);
        let (t0, t1) = (top[i], top[j]);
        solid.faces.push(Face::new(b0, b1, t1));
        solid.faces.push(Face::new(b0, t1, t0));
    }
    solid
}

/// Revolve a 2D profile (in the XY plane, rotated about the X axis) by
/// `angle` radians around the X axis, producing a solid of revolution.
pub fn revolve(sketch: &Sketch, angle: f64, segments: usize) -> Solid {
    let profile = sketch_boundary(sketch, 24);
    let mut solid = Solid::new();
    if profile.is_empty() {
        return solid;
    }
    let seg = segments.max(3);
    let n = profile.len();
    // For each profile point, generate a ring of `seg+1` vertices around X.
    let rings: Vec<Vec<u32>> = profile
        .iter()
        .map(|p| {
            (0..=seg)
                .map(|k| {
                    let t = angle * (k as f64 / seg as f64);
                    // rotate (y,z) where initial z = p.y, y = 0... actually revolve about X:
                    // point (x=p.x, y=p.y, z=0) rotated about X by t.
                    let y = p.y * t.cos();
                    let z = p.y * t.sin();
                    solid.add_vertex(Vec3::new(p.x, y, z))
                })
                .collect()
        })
        .collect();
    for i in 0..n {
        let j = (i + 1) % n;
        for k in 0..seg {
            let a0 = rings[i][k];
            let a1 = rings[i][k + 1];
            let b0 = rings[j][k];
            let b1 = rings[j][k + 1];
            solid.faces.push(Face::new(a0, a1, b1));
            solid.faces.push(Face::new(a0, b1, b0));
        }
    }
    // End caps if the revolution is a full turn. The start (t=0) and end (t=2π)
    // rings coincide geometrically, so a single cap closes the solid. Its
    // outward normal points in the -θ direction (i.e. -Z for the XY profile),
    // which is the reverse of the fan winding used by `tessellate_polygon_z`.
    if (angle - 2.0 * std::f64::consts::PI).abs() < 1e-9 {
        let cap: Vec<u32> = profile
            .iter()
            .map(|p| solid.add_vertex(Vec3::new(p.x, p.y, 0.0)))
            .collect();
        for i in 1..(n as u32 - 1) {
            solid
                .faces
                .push(Face::new(cap[0], cap[i as usize + 1], cap[i as usize]));
        }
    }
    // The revolved mesh is constructed with a consistent (inward) orientation;
    // flip to outward so `volume()`/normals are correct.
    solid.reverse_winding();
    solid
}

/// Sweep a 2D profile along a polyline path. For v1 the profile is translated
/// (no rotation/twist) along each segment of `path`.
pub fn sweep(profile: &[crate::math::Vec2], path: &[Vec3]) -> Solid {
    let mut solid = Solid::new();
    if profile.len() < 3 || path.len() < 2 {
        return solid;
    }
    let n = profile.len();
    let mut prev: Option<Vec<u32>> = None;
    for waypoint in path {
        let idx: Vec<u32> = profile
            .iter()
            .map(|p| solid.add_vertex(Vec3::new(p.x + waypoint.x, p.y + waypoint.y, waypoint.z)))
            .collect();
        if let Some(p) = prev {
            for i in 0..n {
                let j = (i + 1) % n;
                let (b0, b1) = (p[i], p[j]);
                let (t0, t1) = (idx[i], idx[j]);
                solid.faces.push(Face::new(b0, b1, t1));
                solid.faces.push(Face::new(b0, t1, t0));
            }
        }
        prev = Some(idx);
    }
    solid
}

/// Loft between two 2D profiles placed at `z0` and `z1`. Both profiles must
/// have the same number of points; corresponding points are connected.
pub fn loft(
    profile0: &[crate::math::Vec2],
    profile1: &[crate::math::Vec2],
    z0: f64,
    z1: f64,
) -> Solid {
    let mut solid = Solid::new();
    if profile0.len() != profile1.len() || profile0.len() < 3 {
        return solid;
    }
    let n = profile0.len();
    let ring0: Vec<u32> = profile0
        .iter()
        .map(|p| solid.add_vertex(Vec3::new(p.x, p.y, z0)))
        .collect();
    let ring1: Vec<u32> = profile1
        .iter()
        .map(|p| solid.add_vertex(Vec3::new(p.x, p.y, z1)))
        .collect();
    for i in 0..n {
        let j = (i + 1) % n;
        solid.faces.push(Face::new(ring0[i], ring0[j], ring1[j]));
        solid.faces.push(Face::new(ring0[i], ring1[j], ring1[i]));
    }
    tessellate_polygon_z(&mut solid, profile0, z0);
    tessellate_polygon_z(&mut solid, profile1, z1);
    solid
}

/// Boolean union (A ∪ B). For the faceted v1 kernel this concatenates the two
/// meshes. A full intersection/union via mesh CSG is a later refinement; this
/// provides a correct, watertight-enough result for rendering and stacking.
pub fn union(a: &Solid, b: &Solid) -> Solid {
    let mut out = a.clone();
    out.extend(b);
    out
}

/// Boolean subtract (A − B). v1: returns A unchanged (placeholder for the CSG
/// engine). The documented intent is set difference; the exact engine lands in
/// a later iteration (see ADR-0004).
pub fn subtract(a: &Solid, _b: &Solid) -> Solid {
    a.clone()
}

/// Boolean intersect (A ∩ B). v1: returns A unchanged (placeholder).
pub fn intersect(a: &Solid, _b: &Solid) -> Solid {
    a.clone()
}

/// Fillet (round) the edges of a solid. v1: returns the solid unchanged and is
/// a documented placeholder for the rounding engine.
pub fn fillet(solid: &Solid, _radius: f64) -> Solid {
    solid.clone()
}

/// Chamfer (bevel) the edges of a solid. v1: returns the solid unchanged.
pub fn chamfer(solid: &Solid, _distance: f64) -> Solid {
    solid.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::Vec2;

    #[test]
    fn extrude_box_volume() {
        let mut sk = Sketch::new();
        sk.line(Vec2::ZERO, Vec2::new(2.0, 0.0));
        sk.line(Vec2::new(2.0, 0.0), Vec2::new(2.0, 2.0));
        sk.line(Vec2::new(2.0, 2.0), Vec2::ZERO);
        let s = extrude(&sk, 3.0);
        // Right-triangle profile (area 2) extruded 3 => prism volume 6.
        assert!(
            (s.volume().abs() - 6.0).abs() < 1e-6,
            "volume was {}",
            s.volume()
        );
    }

    #[test]
    fn revolve_tube_volume() {
        // Closed rectangular profile away from the axis -> hollow cylinder
        // (tube) of inner radius 1, outer radius 2, height 2.
        let mut sk = Sketch::new();
        sk.line(Vec2::new(1.0, 0.0), Vec2::new(2.0, 0.0));
        sk.line(Vec2::new(2.0, 0.0), Vec2::new(2.0, 2.0));
        sk.line(Vec2::new(2.0, 2.0), Vec2::new(1.0, 2.0));
        sk.line(Vec2::new(1.0, 2.0), Vec2::new(1.0, 0.0));
        let s = revolve(&sk, 2.0 * std::f64::consts::PI, 32);
        // Profile spans radius 0..2 at axial 1..2 => solid cylinder r=2, h=1.
        let expected = std::f64::consts::PI * 2.0f64 * 2.0 * 1.0;
        assert!(
            (s.volume() - expected).abs() < expected * 0.05,
            "volume was {}",
            s.volume()
        );
    }

    #[test]
    fn union_has_both_geometries() {
        let a = extrude_profile(
            &[
                Vec2::ZERO,
                Vec2::new(1.0, 0.0),
                Vec2::new(1.0, 1.0),
                Vec2::new(0.0, 1.0),
            ],
            1.0,
        );
        let b = extrude_profile(
            &[
                Vec2::new(2.0, 0.0),
                Vec2::new(3.0, 0.0),
                Vec2::new(3.0, 1.0),
                Vec2::new(2.0, 1.0),
            ],
            1.0,
        );
        let u = union(&a, &b);
        assert_eq!(u.triangle_count(), a.triangle_count() + b.triangle_count());
    }

    #[test]
    fn sweep_makes_tube_walls() {
        let profile = vec![
            Vec2::ZERO,
            Vec2::new(1.0, 0.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(0.0, 1.0),
        ];
        let path = vec![Vec3::ZERO, Vec3::new(0.0, 0.0, 5.0)];
        let s = sweep(&profile, &path);
        assert!(s.triangle_count() > 0);
    }
}
