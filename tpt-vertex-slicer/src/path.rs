//! Toolpath ordering: arrange perimeters, infill, and travel moves into an
//! ordered print sequence with retraction and Z-hops.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0

use crate::infill::InfillLine;
use crate::layers::P2;

/// A single extrusion path (a polyline) at a given layer height.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtrusionPath {
    /// Ordered points of the path (closed loops repeat the first point at the
    /// end; open paths do not).
    pub points: Vec<P2>,
    /// True if the path is a closed loop (perimeter/wall).
    pub closed: bool,
    /// Extrusion width override in millimetres; `None` uses the printer's
    /// default nozzle-derived width. Set for variable-width thin-wall fill
    /// and support pillars sized to a non-default footprint.
    pub width: Option<f64>,
    /// True when this path bridges an unsupported gap and should use
    /// bridge-specific print speed and full cooling.
    pub is_bridge: bool,
    /// Index of the extruder/tool that should print this path (multi-material
    /// / multi-extruder toolpaths); `0` is the default single-extruder case.
    pub tool: usize,
}

impl ExtrusionPath {
    /// Construct a path with all non-geometric fields at their defaults
    /// (nozzle-default width, not a bridge, tool 0).
    pub fn new(points: Vec<P2>, closed: bool) -> Self {
        ExtrusionPath {
            points,
            closed,
            width: None,
            is_bridge: false,
            tool: 0,
        }
    }
}

/// A movement command emitted by the planner. Travel moves (no extrusion) carry
/// retraction/Z-hop intent; extrusion moves carry a per-mm flow.
#[derive(Debug, Clone, PartialEq)]
pub enum Move {
    /// Rapid/non-print move with optional retraction and Z-hop.
    Travel {
        to: P2,
        z: f64,
        retract: bool,
        z_hop: bool,
    },
    /// Print move along a path at the given Z.
    Extrude { path: ExtrusionPath, z: f64 },
}

/// Ordered plan for one layer.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LayerPlan {
    pub z: f64,
    pub moves: Vec<Move>,
}

impl LayerPlan {
    /// Total printed path length (extrusion moves only).
    pub fn printed_length(&self) -> f64 {
        let mut total = 0.0;
        for m in &self.moves {
            if let Move::Extrude { path, .. } = m {
                let n = path.points.len();
                for i in 1..n {
                    total += path.points[i].dist(path.points[i - 1]);
                }
            }
        }
        total
    }

    /// Total travel length (non-print moves).
    pub fn travel_length(&self) -> f64 {
        let mut total = 0.0;
        let mut prev: Option<P2> = None;
        for m in &self.moves {
            match m {
                Move::Travel { to, .. } => {
                    if let Some(p) = prev {
                        total += p.dist(*to);
                    }
                    prev = Some(*to);
                }
                Move::Extrude { path, .. } => {
                    prev = path.points.last().copied();
                }
            }
        }
        total
    }
}

/// Build a layer plan from its perimeters (outer-to-inner walls) and infill
/// lines. The planner greedily orders paths by nearest-neighbour from the last
/// position to minimise travel; a retraction + Z-hop is inserted before each
/// travel move.
pub fn plan_layer(z: f64, perimeters: Vec<ExtrusionPath>, infill: Vec<InfillLine>) -> LayerPlan {
    let mut paths: Vec<ExtrusionPath> = perimeters;
    for line in infill {
        let mut path = ExtrusionPath::new(vec![line.a, line.b], false);
        path.is_bridge = line.is_bridge;
        path.tool = line.tool;
        paths.push(path);
    }

    let mut moves = Vec::new();
    let mut pos: Option<P2> = None;
    let mut remaining: Vec<usize> = (0..paths.len()).collect();

    while !remaining.is_empty() {
        let cur = pos.unwrap_or_else(|| paths[remaining[0]].points[0]);
        // Nearest-neighbour selection.
        let mut best_idx = 0;
        let mut best_d = f64::INFINITY;
        for (k, &i) in remaining.iter().enumerate() {
            let start = paths[i].points[0];
            let d = cur.dist(start);
            if d < best_d {
                best_d = d;
                best_idx = k;
            }
        }
        let chosen = remaining.remove(best_idx);
        let path = &paths[chosen];

        if let Some(p) = pos {
            if p.dist(path.points[0]) > 1e-6 {
                moves.push(Move::Travel {
                    to: path.points[0],
                    z,
                    retract: true,
                    z_hop: true,
                });
            }
        }
        moves.push(Move::Extrude {
            path: path.clone(),
            z,
        });
        pos = path.points.last().copied();
    }

    LayerPlan { z, moves }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layers::Contour;

    fn loop_rect() -> ExtrusionPath {
        let c = Contour {
            points: vec![
                P2::new(0.0, 0.0),
                P2::new(10.0, 0.0),
                P2::new(10.0, 10.0),
                P2::new(0.0, 10.0),
            ],
        };
        ExtrusionPath::new(c.points.clone(), true)
    }

    #[test]
    fn plan_emits_extrusion_then_travel() {
        let plan = plan_layer(
            0.2,
            vec![loop_rect()],
            vec![
                InfillLine::new(P2::new(1.0, 1.0), P2::new(9.0, 1.0)),
                InfillLine::new(P2::new(1.0, 3.0), P2::new(9.0, 3.0)),
            ],
        );
        assert!(plan.printed_length() > 0.0);
        assert!(matches!(plan.moves[0], Move::Extrude { .. }));
    }
}
