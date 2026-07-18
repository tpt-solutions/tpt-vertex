//! Mesh tessellation helpers bridging 2D sketches to 3D solids.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0

use crate::geometry::sketch::Sketch;
use crate::geometry::solid::{Face, Solid};
use crate::math::Vec3;

/// Triangulate a simple convex (or star-shaped) 2D polygon via fan
/// triangulation, placing it on the XY plane at height `z`.
///
/// `polygon` is an ordered list of 2D points forming a closed loop. The result
/// is appended to `solid`. Returns the number of cap faces added.
pub fn tessellate_polygon_z(solid: &mut Solid, polygon: &[crate::math::Vec2], z: f64) -> usize {
    if polygon.len() < 3 {
        return 0;
    }
    let base: Vec<u32> = polygon
        .iter()
        .map(|p| solid.add_vertex(Vec3::new(p.x, p.y, z)))
        .collect();
    let start = solid.faces.len();
    for i in 1..(base.len() as u32 - 1) {
        solid
            .faces
            .push(Face::new(base[0], base[i as usize], base[i as usize + 1]));
    }
    solid.faces.len() - start
}

/// Extract the outer boundary of a sketch as an ordered list of 2D points.
///
/// For v1 this returns all line endpoints in entity order; arcs/circles are
/// sampled into polylines. A full profile-topology builder is a later
/// refinement (see ADR-0004).
pub fn sketch_boundary(sketch: &Sketch, arc_samples: usize) -> Vec<crate::math::Vec2> {
    use crate::math::Vec2;
    let mut pts: Vec<Vec2> = Vec::new();
    for e in &sketch.entities {
        match e {
            crate::geometry::sketch::SketchEntity::Line(l) => {
                if let (Some(a), Some(b)) = (sketch.point(l.start), sketch.point(l.end)) {
                    pts.push(a.pos);
                    pts.push(b.pos);
                }
            }
            crate::geometry::sketch::SketchEntity::Arc(a) => {
                if let (Some(s), Some(en), Some(c)) = (
                    sketch.point(a.start),
                    sketch.point(a.end),
                    sketch.point(a.center),
                ) {
                    pts.push(s.pos);
                    let r = s.pos.distance(c.pos);
                    let a0 = (s.pos - c.pos).angle();
                    let a1 = (en.pos - c.pos).angle();
                    let mut sweep = a1 - a0;
                    while sweep <= -std::f64::consts::PI {
                        sweep += 2.0 * std::f64::consts::PI;
                    }
                    while sweep > std::f64::consts::PI {
                        sweep -= 2.0 * std::f64::consts::PI;
                    }
                    if !a.ccw && sweep > 0.0 {
                        sweep -= 2.0 * std::f64::consts::PI;
                    }
                    if a.ccw && sweep < 0.0 {
                        sweep += 2.0 * std::f64::consts::PI;
                    }
                    let n = arc_samples.max(2);
                    for i in 1..n {
                        let t = sweep * (i as f64 / n as f64);
                        pts.push(c.pos + Vec2::new(r * t.cos(), r * t.sin()));
                    }
                    pts.push(en.pos);
                }
            }
            crate::geometry::sketch::SketchEntity::Circle(c) => {
                if let (Some(center), Some(rp)) =
                    (sketch.point(c.center), sketch.point(c.radius_point))
                {
                    let r = center.pos.distance(rp.pos);
                    let n = arc_samples.max(8);
                    for i in 0..n {
                        let t = 2.0 * std::f64::consts::PI * (i as f64 / n as f64);
                        pts.push(center.pos + Vec2::new(r * t.cos(), r * t.sin()));
                    }
                }
            }
            crate::geometry::sketch::SketchEntity::Polyline(_) => {}
        }
    }
    pts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::sketch::Sketch;
    use crate::math::Vec2;

    #[test]
    fn polygon_fan_triangulates_square() {
        let mut s = Solid::new();
        let sq = vec![
            Vec2::ZERO,
            Vec2::new(1.0, 0.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(0.0, 1.0),
        ];
        let count = tessellate_polygon_z(&mut s, &sq, 0.0);
        assert_eq!(count, 2);
        assert!((s.surface_area() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn boundary_samples_circle() {
        let mut sk = Sketch::new();
        sk.circle(Vec2::ZERO, 1.0);
        let b = sketch_boundary(&sk, 16);
        assert_eq!(b.len(), 16);
    }
}
