//! Sheet-metal module: flat-pattern unfolding, bend allowances, bend order.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! A [`FlatPattern`] describes a flat pattern derived from a 3D sheet-metal
//! model: the 2D outline, bend lines (hinge lines), bend angles, radii, and
//! a recommended bend-order sequence.  The unfold algorithm works from the
//! kernel's [`Solid`] by identifying planar faces connected at bend edges.

use tpt_vertex_kernel::geometry::solid::Solid;
use tpt_vertex_kernel::math::Vec3;

/// A 2D point in the flat-pattern coordinate system.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlatPt {
    pub x: f64,
    pub y: f64,
}

impl FlatPt {
    pub fn new(x: f64, y: f64) -> Self {
        FlatPt { x, y }
    }
}

/// A bend line (hinge) in the flat pattern.
#[derive(Debug, Clone, PartialEq)]
pub struct BendLine {
    /// Start of the hinge line in flat-pattern coordinates.
    pub start: FlatPt,
    /// End of the hinge line.
    pub end: FlatPt,
    /// Bend angle in radians (positive = bend up).
    pub angle_rad: f64,
    /// Bend radius in millimetres (inside radius).
    pub radius_mm: f64,
    /// Bend direction: `true` = bend toward the viewer, `false` = away.
    pub bend_up: bool,
}

/// Bend allowance strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BendAllowance {
    /// K-factor method: `ba = angle * (radius + k * thickness)`.
    KFactor,
    /// Bend deduction: subtract a fixed length per bend.
    BendDeduction,
    /// Y-factor (similar to K-factor but with a different constant).
    YFactor,
}

/// Configuration for flat-pattern generation.
#[derive(Debug, Clone, PartialEq)]
pub struct SheetMetalConfig {
    /// Material thickness in millimetres.
    pub thickness: f64,
    /// Bend allowance strategy.
    pub allowance: BendAllowance,
    /// K-factor (0.0–1.0, typically 0.33–0.50 for steel).
    pub k_factor: f64,
    /// Minimum inside bend radius (mm); bends sharper than this are clamped.
    pub min_bend_radius: f64,
    /// Dihedral angle tolerance (radians) for detecting bends between faces.
    /// Faces sharing an edge whose dihedral angle differs from PI by more
    /// than this tolerance are classified as a bend.
    pub bend_angle_tolerance: f64,
}

impl Default for SheetMetalConfig {
    fn default() -> Self {
        SheetMetalConfig {
            thickness: 1.0,
            allowance: BendAllowance::KFactor,
            k_factor: 0.44,
            min_bend_radius: 0.5,
            bend_angle_tolerance: 0.01, // ~0.6 degrees
        }
    }
}

/// The output of flat-pattern generation.
#[derive(Debug, Clone, PartialEq)]
pub struct FlatPattern {
    /// 2D outline of the unfolded part (closed polygon).
    pub outline: Vec<FlatPt>,
    /// Bend lines in order.
    pub bend_lines: Vec<BendLine>,
    /// Suggested bend sequence (indices into `bend_lines`).
    pub bend_order: Vec<usize>,
}

/// Compute the bend allowance arc length for a single bend.
pub fn bend_allowance_length(angle_rad: f64, radius: f64, thickness: f64, k_factor: f64) -> f64 {
    // Neutral-axis arc length: ba = angle * (R + k * T)
    angle_rad.abs() * (radius + k_factor * thickness)
}

/// An internal edge shared by two faces, with face normals.
#[derive(Debug, Clone)]
struct SharedEdge {
    a: Vec3,
    b: Vec3,
    normal_a: Vec3,
    normal_b: Vec3,
}

/// Compute the dihedral angle between two face normals at a shared edge.
/// Returns the angle in radians from PI (flat = PI, bend < PI).
fn dihedral_angle(n1: Vec3, n2: Vec3) -> f64 {
    let dot = n1.dot(n2).clamp(-1.0, 1.0);
    std::f64::consts::PI - dot.acos()
}

/// Compute the normal of a triangle face from its three vertices.
fn face_normal(v0: Vec3, v1: Vec3, v2: Vec3) -> Vec3 {
    let e1 = v1 - v0;
    let e2 = v2 - v0;
    e1.cross(e2).normalize()
}

/// Project a 3D point onto a 2D coordinate system defined by a face normal.
/// Returns (u, v) coordinates in the face's plane.
fn project_to_face_plane(p: Vec3, origin: Vec3, normal: Vec3) -> (f64, f64) {
    let d = p - origin;
    // Build an orthonormal basis from the normal.
    let up = if normal.x.abs() < 0.9 {
        Vec3::new(1.0, 0.0, 0.0)
    } else {
        Vec3::new(0.0, 1.0, 0.0)
    };
    let u_axis = normal.cross(up).normalize();
    let v_axis = normal.cross(u_axis).normalize();
    (d.dot(u_axis), d.dot(v_axis))
}

/// Generate a flat pattern from a solid by identifying planar faces connected
/// at bend edges.  Each bend edge has a dihedral angle that differs from PI
/// (flat) by more than the configured tolerance.
pub fn unfold_solid(solid: &Solid, config: &SheetMetalConfig) -> Option<FlatPattern> {
    let (min, max) = solid.bounds()?;

    // Step 1: Build face normals.
    let face_normals: Vec<Vec3> = solid
        .faces
        .iter()
        .map(|f| {
            let v0 = solid.vertices[f.a as usize];
            let v1 = solid.vertices[f.b as usize];
            let v2 = solid.vertices[f.c as usize];
            face_normal(v0, v1, v2)
        })
        .collect();

    // Step 2: Build an edge → face adjacency map.
    // Each edge is represented as (min(a,b), max(a,b)) → list of face indices.
    let mut edge_faces: std::collections::HashMap<(u32, u32), Vec<usize>> =
        std::collections::HashMap::new();
    for (fi, face) in solid.faces.iter().enumerate() {
        for edge in &[
            (face.a.min(face.b), face.a.max(face.b)),
            (face.b.min(face.c), face.b.max(face.c)),
            (face.c.min(face.a), face.c.max(face.a)),
        ] {
            edge_faces.entry(*edge).or_default().push(fi);
        }
    }

    // Step 3: Find shared edges with dihedral angles indicating bends.
    let mut bend_edges: Vec<SharedEdge> = Vec::new();
    for ((va, vb), faces) in &edge_faces {
        if faces.len() != 2 {
            continue;
        }
        let n1 = face_normals[faces[0]];
        let n2 = face_normals[faces[1]];
        let angle = dihedral_angle(n1, n2);

        // A flat continuation has angle ≈ PI.  A bend deviates from PI.
        if (angle - std::f64::consts::PI).abs() > config.bend_angle_tolerance {
            bend_edges.push(SharedEdge {
                a: solid.vertices[*va as usize],
                b: solid.vertices[*vb as usize],
                normal_a: n1,
                normal_b: n2,
            });
        }
    }

    // Step 4: Find the largest face as the base (reference) face and project
    // it to 2D.
    let base_face_idx = solid
        .faces
        .iter()
        .enumerate()
        .max_by_key(|(_, f)| {
            let v0 = solid.vertices[f.a as usize];
            let v1 = solid.vertices[f.b as usize];
            let v2 = solid.vertices[f.c as usize];
            let e1 = v1 - v0;
            let e2 = v2 - v0;
            (e1.cross(e2).length() * 1e6) as u64
        })
        .map(|(i, _)| i)
        .unwrap_or(0);

    let base_face = &solid.faces[base_face_idx];
    let base_normal = face_normals[base_face_idx];
    let base_origin = solid.vertices[base_face.a as usize];

    // Project all unique vertices onto the base face's plane.
    let mut all_vertices_2d: std::collections::HashMap<u32, FlatPt> =
        std::collections::HashMap::new();
    for (vi, v) in solid.vertices.iter().enumerate() {
        let (u, pv) = project_to_face_plane(*v, base_origin, base_normal);
        all_vertices_2d.insert(vi as u32, FlatPt::new(u, pv));
    }

    // Step 5: Build the outline from face edges that are NOT shared (boundary edges).
    let mut boundary_edges: Vec<(u32, u32)> = Vec::new();
    for ((va, vb), faces) in &edge_faces {
        if faces.len() == 1 {
            boundary_edges.push((*va, *vb));
        }
    }

    // Order boundary edges into a closed polygon.
    let outline = order_boundary_edges(&boundary_edges, &all_vertices_2d);

    // Step 6: Generate bend lines from the bend edges, projected to 2D.
    let mut bend_lines = Vec::new();
    for be in &bend_edges {
        let (u1, v1) = project_to_face_plane(be.a, base_origin, base_normal);
        let (u2, v2) = project_to_face_plane(be.b, base_origin, base_normal);

        let angle = dihedral_angle(be.normal_a, be.normal_b);
        let bend_angle = std::f64::consts::PI - angle;
        let bend_radius = config.min_bend_radius;

        bend_lines.push(BendLine {
            start: FlatPt::new(u1, v1),
            end: FlatPt::new(u2, v2),
            angle_rad: bend_angle,
            radius_mm: bend_radius,
            bend_up: be.normal_a.z > 0.0,
        });
    }

    // Step 7: Generate a default bend order (sort by angle, largest first).
    let mut bend_order: Vec<usize> = (0..bend_lines.len()).collect();
    bend_order.sort_by(|a, b| {
        bend_lines[*b]
            .angle_rad
            .partial_cmp(&bend_lines[*a].angle_rad)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let _ = (min, max, config);

    Some(FlatPattern {
        outline,
        bend_lines,
        bend_order,
    })
}

/// Order boundary edges into a closed polygon by chaining them end-to-end.
fn order_boundary_edges(
    edges: &[(u32, u32)],
    verts: &std::collections::HashMap<u32, FlatPt>,
) -> Vec<FlatPt> {
    if edges.is_empty() {
        return Vec::new();
    }

    // Build adjacency: vertex → list of neighboring vertices.
    let mut adj: std::collections::HashMap<u32, Vec<u32>> = std::collections::HashMap::new();
    for &(a, b) in edges {
        adj.entry(a).or_default().push(b);
        adj.entry(b).or_default().push(a);
    }

    // Walk from the first edge's start vertex.
    let mut visited: std::collections::HashSet<(u32, u32)> = std::collections::HashSet::new();
    let mut path: Vec<u32> = Vec::new();
    let start = edges[0].0;
    let mut current = start;
    path.push(current);

    loop {
        let mut found_next = false;
        if let Some(neighbors) = adj.get(&current) {
            for &next in neighbors {
                let edge_key = if current < next {
                    (current, next)
                } else {
                    (next, current)
                };
                if !visited.contains(&edge_key) {
                    visited.insert(edge_key);
                    path.push(next);
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

    path.iter()
        .filter_map(|vi| verts.get(vi).copied())
        .collect()
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
            v(10.0, 0.0, 0.0),
            v(10.0, 5.0, 0.0),
            v(0.0, 5.0, 0.0),
        ];
        let mut f = |a: u32, b: u32, c: u32| s.faces.push(Face::new(a, b, c));
        f(p[0], p[1], p[2]);
        f(p[0], p[2], p[3]);
        s
    }

    /// A simple L-bracket: two faces at a 90-degree bend.
    fn l_bracket() -> KernSolid {
        let mut s = KernSolid::new();
        let mut v = |x: f64, y: f64, z: f64| s.add_vertex(Vec3::new(x, y, z));
        // Shared bend-line vertices at y=5: 2=(10,5,0) and 3=(0,5,0)
        let p = [
            v(0.0, 0.0, 0.0),
            v(10.0, 0.0, 0.0),
            v(10.0, 5.0, 0.0),
            v(0.0, 5.0, 0.0),
            v(10.0, 5.0, 5.0),
            v(0.0, 5.0, 5.0),
        ];
        let mut f = |a: u32, b: u32, c: u32| s.faces.push(Face::new(a, b, c));
        // Horizontal face (z=0 plane)
        f(p[0], p[1], p[2]);
        f(p[0], p[2], p[3]);
        // Vertical face (y=5 plane) — shares edge 2-3
        f(p[3], p[2], p[4]);
        f(p[3], p[4], p[5]);
        s
    }

    #[test]
    fn unfold_plate_produces_outline() {
        let config = SheetMetalConfig::default();
        let fp = unfold_solid(&plate(), &config).unwrap();
        assert!(!fp.outline.is_empty());
    }

    #[test]
    fn unfold_l_bracket_detects_bend() {
        let config = SheetMetalConfig::default();
        let fp = unfold_solid(&l_bracket(), &config).unwrap();
        assert!(
            !fp.bend_lines.is_empty(),
            "expected at least one bend line in the L-bracket"
        );
    }

    #[test]
    fn bend_allowance_kfactor() {
        let ba = bend_allowance_length(std::f64::consts::FRAC_PI_2, 1.0, 2.0, 0.44);
        let expected = std::f64::consts::FRAC_PI_2 * 1.88;
        assert!((ba - expected).abs() < 1e-10);
    }

    #[test]
    fn dihedral_angle_flat_is_pi() {
        let n1 = Vec3::new(0.0, 0.0, 1.0);
        let n2 = Vec3::new(0.0, 0.0, 1.0);
        let angle = dihedral_angle(n1, n2);
        assert!((angle - std::f64::consts::PI).abs() < 1e-6);
    }

    #[test]
    fn dihedral_angle_90_degrees() {
        let n1 = Vec3::new(0.0, 0.0, 1.0);
        let n2 = Vec3::new(0.0, 1.0, 0.0);
        let angle = dihedral_angle(n1, n2);
        assert!((angle - std::f64::consts::FRAC_PI_2).abs() < 1e-6);
    }
}
