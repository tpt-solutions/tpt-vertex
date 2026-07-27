//! CAM: toolpath generation for CNC milling and turning.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! Provides data structures and top-level entry points for generating CNC
//! toolpaths from kernel geometry.  This scaffold defines the public API
//! surface and a minimal rectangular-pocket clearing implementation.

use tpt_vertex_kernel::geometry::solid::Solid;

/// A 3D point in work coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CamPt {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl CamPt {
    pub fn new(x: f64, y: f64, z: f64) -> Self {
        CamPt { x, y, z }
    }
}

/// A toolpath move: rapid or linear feed.
#[derive(Debug, Clone, PartialEq)]
pub enum ToolMove {
    /// Rapid (G0) to position.
    Rapid { to: CamPt },
    /// Linear feed (G1) to position with feed rate (mm/min).
    Feed { to: CamPt, feed_mm_min: f64 },
}

/// A sequence of toolpath moves for one cutting operation.
#[derive(Debug, Clone, PartialEq)]
pub struct Toolpath {
    pub name: String,
    pub tool_diameter: f64,
    pub spindle_rpm: u32,
    pub moves: Vec<ToolMove>,
}

/// A complete CAM job: one or more toolpaths.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CamJob {
    pub toolpaths: Vec<Toolpath>,
}

impl CamJob {
    pub fn estimated_time_s(&self) -> f64 {
        let mut total_s = 0.0;
        let mut last: Option<CamPt> = None;
        for tp in &self.toolpaths {
            for m in &tp.moves {
                let to = match m {
                    ToolMove::Rapid { to } | ToolMove::Feed { to, .. } => *to,
                };
                if let Some(prev) = last {
                    let d = ((to.x - prev.x).powi(2) + (to.y - prev.y).powi(2) + (to.z - prev.z).powi(2)).sqrt();
                    if let ToolMove::Feed { feed_mm_min, .. } = m {
                        total_s += d / feed_mm_min.max(1.0) * 60.0;
                    }
                }
                last = Some(to);
            }
        }
        total_s
    }

    pub fn total_moves(&self) -> usize {
        self.toolpaths.iter().map(|t| t.moves.len()).sum()
    }
}

/// Generate a simple rectangular-pocket clearing toolpath.
pub fn rect_pocket(
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    depth: f64,
    tool_diameter: f64,
    step_over_frac: f64,
    plunge_feed: f64,
    cut_feed: f64,
    spindle_rpm: u32,
) -> Toolpath {
    let step = tool_diameter * step_over_frac.max(0.1);
    let mut moves = Vec::new();
    let safe_z = 5.0;

    moves.push(ToolMove::Rapid { to: CamPt::new(x0, y0, safe_z) });
    moves.push(ToolMove::Feed { to: CamPt::new(x0, y0, depth), feed_mm_min: plunge_feed });

    let mut y = y0 + step / 2.0;
    let mut right = true;
    while y <= y1 {
        let (xa, xb) = if right { (x0, x1) } else { (x1, x0) };
        moves.push(ToolMove::Rapid { to: CamPt::new(xa, y, depth + 0.5) });
        moves.push(ToolMove::Feed { to: CamPt::new(xa, y, depth), feed_mm_min: cut_feed });
        moves.push(ToolMove::Feed { to: CamPt::new(xb, y, depth), feed_mm_min: cut_feed });
        y += step;
        right = !right;
    }

    moves.push(ToolMove::Rapid { to: CamPt::new(x0, y0, safe_z) });

    Toolpath {
        name: "rect-pocket".to_string(),
        tool_diameter,
        spindle_rpm,
        moves,
    }
}

/// Generate a CAM job from a solid (placeholder: rectangular bounding-box pocket).
pub fn job_from_solid(solid: &Solid, tool_diameter: f64) -> Option<CamJob> {
    let (min, max) = solid.bounds()?;
    let tp = rect_pocket(
        min.x, min.y, max.x, max.y,
        min.z,
        tool_diameter,
        0.5,
        100.0,
        500.0,
        12000,
    );
    Some(CamJob { toolpaths: vec![tp] })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_vertex_kernel::geometry::solid::{Face, Solid as KernSolid};
    use tpt_vertex_kernel::math::Vec3;

    fn plate() -> KernSolid {
        let mut s = KernSolid::new();
        let v = |x: f64, y: f64, z: f64| s.add_vertex(Vec3::new(x, y, z));
        let p = [v(0.0, 0.0, 0.0), v(20.0, 0.0, 0.0), v(20.0, 10.0, 0.0), v(0.0, 10.0, 0.0)];
        let mut f = |a: u32, b: u32, c: u32| s.faces.push(Face::new(a, b, c));
        f(p[0], p[1], p[2]); f(p[0], p[2], p[3]);
        s
    }

    #[test]
    fn rect_pocket_produces_moves() {
        let tp = rect_pocket(0.0, 0.0, 20.0, 10.0, -2.0, 3.0, 0.5, 100.0, 500.0, 12000);
        assert!(tp.moves.len() > 4);
        assert_eq!(tp.name, "rect-pocket");
    }

    #[test]
    fn job_from_solid_succeeds() {
        let job = job_from_solid(&plate(), 3.0).unwrap();
        assert_eq!(job.toolpaths.len(), 1);
        assert!(job.total_moves() > 0);
    }
}
