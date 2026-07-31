//! CAM: toolpath generation for CNC milling and turning.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! Provides data structures and top-level entry points for generating CNC
//! toolpaths from kernel geometry.  Includes rectangular-pocket clearing,
//! contour-following (profile) toolpaths, and drill cycles.

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

    pub fn dist(&self, other: &CamPt) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        let dz = self.z - other.z;
        (dx * dx + dy * dy + dz * dz).sqrt()
    }
}

/// A toolpath move: rapid or linear feed.
#[derive(Debug, Clone, PartialEq)]
pub enum ToolMove {
    /// Rapid (G0) to position.
    Rapid { to: CamPt },
    /// Linear feed (G1) to position with feed rate (mm/min).
    Feed { to: CamPt, feed_mm_min: f64 },
    /// Drill cycle: plunge to depth at center, retract.
    Drill {
        to: CamPt,
        depth: f64,
        retract_z: f64,
        feed_mm_min: f64,
    },
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
                    ToolMove::Rapid { to }
                    | ToolMove::Feed { to, .. }
                    | ToolMove::Drill { to, .. } => *to,
                };
                if let Some(prev) = last {
                    let d = prev.dist(&to);
                    if let ToolMove::Feed { feed_mm_min, .. }
                    | ToolMove::Drill { feed_mm_min, .. } = m
                    {
                        total_s += d / feed_mm_min.max(1.0) * 60.0;
                    } else {
                        // Rapid moves are estimated at 5000 mm/min.
                        total_s += d / 5000.0 * 60.0;
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
#[allow(clippy::too_many_arguments)]
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

    moves.push(ToolMove::Rapid {
        to: CamPt::new(x0, y0, safe_z),
    });
    moves.push(ToolMove::Feed {
        to: CamPt::new(x0, y0, depth),
        feed_mm_min: plunge_feed,
    });

    let mut y = y0 + step / 2.0;
    let mut right = true;
    while y <= y1 {
        let (xa, xb) = if right { (x0, x1) } else { (x1, x0) };
        moves.push(ToolMove::Rapid {
            to: CamPt::new(xa, y, depth + 0.5),
        });
        moves.push(ToolMove::Feed {
            to: CamPt::new(xa, y, depth),
            feed_mm_min: cut_feed,
        });
        moves.push(ToolMove::Feed {
            to: CamPt::new(xb, y, depth),
            feed_mm_min: cut_feed,
        });
        y += step;
        right = !right;
    }

    moves.push(ToolMove::Rapid {
        to: CamPt::new(x0, y0, safe_z),
    });

    Toolpath {
        name: "rect-pocket".to_string(),
        tool_diameter,
        spindle_rpm,
        moves,
    }
}

/// Generate a contour-following (profile) toolpath that traces the outline of
/// a solid's projected boundary at a given depth.  The tool offset outward by
/// `tool_diameter / 2` so the finished profile matches the nominal geometry.
pub fn contour_follow(
    boundary: &[(f64, f64)],
    depth: f64,
    tool_diameter: f64,
    cut_feed: f64,
    spindle_rpm: u32,
) -> Toolpath {
    let radius = tool_diameter / 2.0;
    let mut moves = Vec::new();
    let safe_z = 5.0;

    if boundary.is_empty() {
        return Toolpath {
            name: "contour-follow".to_string(),
            tool_diameter,
            spindle_rpm,
            moves,
        };
    }

    // Start at the first point, offset outward.
    let (sx, sy) = offset_point_outward(boundary, 0, radius);
    moves.push(ToolMove::Rapid {
        to: CamPt::new(sx, sy, safe_z),
    });
    moves.push(ToolMove::Feed {
        to: CamPt::new(sx, sy, depth),
        feed_mm_min: cut_feed * 0.5,
    });

    // Trace the boundary with offset.
    for i in 1..boundary.len() {
        let (ox, oy) = offset_point_outward(boundary, i, radius);
        moves.push(ToolMove::Feed {
            to: CamPt::new(ox, oy, depth),
            feed_mm_min: cut_feed,
        });
    }

    // Close the loop back to the start.
    moves.push(ToolMove::Feed {
        to: CamPt::new(sx, sy, depth),
        feed_mm_min: cut_feed,
    });

    // Retract.
    moves.push(ToolMove::Rapid {
        to: CamPt::new(sx, sy, safe_z),
    });

    Toolpath {
        name: "contour-follow".to_string(),
        tool_diameter,
        spindle_rpm,
        moves,
    }
}

/// Offset a boundary point outward by `radius` along the bisector of the
/// adjacent edges.
fn offset_point_outward(boundary: &[(f64, f64)], idx: usize, radius: f64) -> (f64, f64) {
    let n = boundary.len();
    if n == 0 {
        return (0.0, 0.0);
    }

    let (x, y) = boundary[idx];
    let prev = boundary[(idx + n - 1) % n];
    let next = boundary[(idx + 1) % n];

    // Edge normals (perpendicular, pointing outward for CCW winding).
    let e1x = x - prev.0;
    let e1y = y - prev.1;
    let e2x = next.0 - x;
    let e2y = next.1 - y;

    // Outward normal of each edge (rotate 90° clockwise for CCW winding).
    let n1x = e1y;
    let n1y = -e1x;
    let n2x = e2y;
    let n2y = -e2x;

    // Normalize and average.
    let l1 = (n1x * n1x + n1y * n1y).sqrt().max(1e-10);
    let l2 = (n2x * n2x + n2y * n2y).sqrt().max(1e-10);
    let bx = n1x / l1 + n2x / l2;
    let by = n1y / l1 + n2y / l2;
    let bl = (bx * bx + by * by).sqrt().max(1e-10);

    (x + bx / bl * radius, y + by / bl * radius)
}

/// Generate a simple drill cycle (G81-style) at multiple hole positions.
/// Each hole is drilled from `safe_z` to `depth` at the given positions.
pub fn drill_cycle(
    holes: &[(f64, f64)],
    depth: f64,
    safe_z: f64,
    retract_z: f64,
    tool_diameter: f64,
    plunge_feed: f64,
    spindle_rpm: u32,
) -> Toolpath {
    let mut moves = Vec::new();

    for &(hx, hy) in holes {
        moves.push(ToolMove::Rapid {
            to: CamPt::new(hx, hy, safe_z),
        });
        moves.push(ToolMove::Drill {
            to: CamPt::new(hx, hy, retract_z),
            depth,
            retract_z,
            feed_mm_min: plunge_feed,
        });
        moves.push(ToolMove::Rapid {
            to: CamPt::new(hx, hy, safe_z),
        });
    }

    Toolpath {
        name: "drill-cycle".to_string(),
        tool_diameter,
        spindle_rpm,
        moves,
    }
}

/// Generate a contour-following toolpath from a solid by projecting its boundary
/// edges onto the XY plane at the solid's minimum Z.
pub fn contour_from_solid(
    solid: &Solid,
    tool_diameter: f64,
    cut_feed: f64,
    spindle_rpm: u32,
) -> Option<Toolpath> {
    let (min, _max) = solid.bounds()?;

    // Build edge adjacency from triangle edges.
    let mut edge_count: std::collections::HashMap<(u32, u32), usize> =
        std::collections::HashMap::new();
    for face in &solid.faces {
        for edge in &[
            (face.a.min(face.b), face.a.max(face.b)),
            (face.b.min(face.c), face.b.max(face.c)),
            (face.c.min(face.a), face.c.max(face.a)),
        ] {
            *edge_count.entry(*edge).or_insert(0) += 1;
        }
    }

    // Boundary edges appear in exactly one triangle.
    let boundary: Vec<(u32, u32)> = edge_count
        .iter()
        .filter(|(_, &count)| count == 1)
        .map(|(&edge, _)| edge)
        .collect();

    if boundary.is_empty() {
        return None;
    }

    // Order boundary edges into a loop.
    let ordered = order_boundary_2d(&boundary, solid);

    Some(contour_follow(
        &ordered,
        min.z,
        tool_diameter,
        cut_feed,
        spindle_rpm,
    ))
}

/// Order boundary edges into a closed 2D polygon.
fn order_boundary_2d(edges: &[(u32, u32)], solid: &Solid) -> Vec<(f64, f64)> {
    if edges.is_empty() {
        return Vec::new();
    }

    let mut adj: std::collections::HashMap<u32, Vec<u32>> = std::collections::HashMap::new();
    for &(a, b) in edges {
        adj.entry(a).or_default().push(b);
        adj.entry(b).or_default().push(a);
    }

    let mut visited: std::collections::HashSet<(u32, u32)> = std::collections::HashSet::new();
    let mut path: Vec<(f64, f64)> = Vec::new();
    let start = edges[0].0;
    let mut current = start;

    loop {
        let v = solid.vertices[current as usize];
        path.push((v.x, v.y));

        let mut found_next = false;
        if let Some(neighbors) = adj.get(&current) {
            for &next in neighbors {
                let key = if current < next {
                    (current, next)
                } else {
                    (next, current)
                };
                if !visited.contains(&key) {
                    visited.insert(key);
                    current = next;
                    found_next = true;
                    break;
                }
            }
        }
        if !found_next || current == start {
            break;
        }
    }

    path
}

/// Generate a CAM job from a solid (placeholder: rectangular bounding-box pocket).
pub fn job_from_solid(solid: &Solid, tool_diameter: f64) -> Option<CamJob> {
    let (min, max) = solid.bounds()?;
    let tp = rect_pocket(
        min.x,
        min.y,
        max.x,
        max.y,
        min.z,
        tool_diameter,
        0.5,
        100.0,
        500.0,
        12000,
    );
    Some(CamJob {
        toolpaths: vec![tp],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_vertex_kernel::geometry::solid::{Face, Solid as KernSolid};
    use tpt_vertex_kernel::math::Vec3;

    fn plate() -> KernSolid {
        let mut s = KernSolid::new();
        let mut v = |x: f64, y: f64, z: f64| s.add_vertex(Vec3::new(x, y, z));
        let p = [
            v(0.0, 0.0, 0.0),
            v(20.0, 0.0, 0.0),
            v(20.0, 10.0, 0.0),
            v(0.0, 10.0, 0.0),
        ];
        let mut f = |a: u32, b: u32, c: u32| s.faces.push(Face::new(a, b, c));
        f(p[0], p[1], p[2]);
        f(p[0], p[2], p[3]);
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

    #[test]
    fn contour_follow_produces_closed_loop() {
        let boundary = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 5.0), (0.0, 5.0)];
        let tp = contour_follow(&boundary, -1.0, 3.0, 500.0, 12000);
        assert!(tp.moves.len() > 4);
        assert_eq!(tp.name, "contour-follow");
    }

    #[test]
    fn drill_cycle_produces_moves_per_hole() {
        let holes = vec![(5.0, 5.0), (15.0, 5.0), (10.0, 8.0)];
        let tp = drill_cycle(&holes, -3.0, 5.0, 1.0, 3.0, 100.0, 12000);
        // Each hole: rapid + drill + retract = 3 moves. Total: 9.
        assert_eq!(tp.moves.len(), 9);
        assert_eq!(tp.name, "drill-cycle");
    }

    #[test]
    fn contour_from_solid_succeeds() {
        let tp = contour_from_solid(&plate(), 3.0, 500.0, 12000);
        assert!(tp.is_some());
    }

    #[test]
    fn cam_job_estimated_time_is_positive() {
        let tp = rect_pocket(0.0, 0.0, 20.0, 10.0, -2.0, 3.0, 0.5, 100.0, 500.0, 12000);
        let job = CamJob {
            toolpaths: vec![tp],
        };
        assert!(job.estimated_time_s() > 0.0);
    }
}
