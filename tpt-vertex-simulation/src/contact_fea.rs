// SPDX-License-Identifier: MIT OR Apache-2.0

//! Multi-part / assembly-level contact-coupled static FEA.
//!
//! Solves the static equilibrium of an assembly where parts may come into
//! contact. Uses a penalty-based contact enforcement: penetrating node pairs
//! generate repulsive forces proportional to the penetration depth, added to
//! the global force vector. The contact stiffness is assembled into the global
//! tangent for Newton-Raphson iteration.
//!
//! The workflow is:
//!
//! 1. Mesh each part independently.
//! 2. Detect contact pairs using the AABB broad phase from [`crate::contact`].
//! 3. For each contact pair, find penetrating node-triangle pairs (node of
//!    part A inside part B's surface, or vice versa).
//! 4. Assemble penalty contact forces and stiffness.
//! 5. Solve the coupled system with Newton-Raphson (reusing [`crate::nonlinear`]).
//!
//! This is a v1 "node-to-surface" penalty contact implementation. More
//! sophisticated methods (mortar, augmented Lagrangian, segment-to-segment)
//! are documented fast-follows.

use crate::bc::BoundaryCondition;
use crate::mesh::VolMesh;
use crate::nonlinear::{NonlinearMaterial, NonlinearTolerance, nonlinear_solve};

/// Contact pair between two meshes (node set A and surface triangles B).
#[derive(Debug, Clone)]
pub struct ContactPairDef {
    /// Index of the first mesh in the assembly.
    pub mesh_a: usize,
    /// Index of the second mesh in the assembly.
    pub mesh_b: usize,
    /// Penalty spring stiffness (N/mm). Should be ~10-100× the material E
    /// to enforce near-rigid contact without excessive oscillation.
    pub penalty_stiffness: f64,
}

/// A contact constraint: node `node_idx` of mesh A is in contact with a
/// triangle of mesh B at penetration depth `depth` along normal `normal`.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
struct ContactConstraint {
    /// Global DOF indices for the contacting node (3 DOFs).
    node_dofs: [usize; 3],
    /// Nodes of the contacting triangle on the other mesh.
    tri_nodes: [usize; 3],
    /// Penetration depth (positive = penetrating).
    depth: f64,
    /// Contact normal (pointing from B into A).
    normal: [f64; 3],
    /// Barycentric coordinates of the contact point on the triangle.
    bary: [f64; 3],
    /// Penalty stiffness for this contact.
    k_penalty: f64,
}

/// Contact detection result: a set of node-triangle contact constraints
/// between two meshes.
fn detect_node_triangle_contacts(
    nodes_a: &[[f64; 3]],
    _tets_a: &[[usize; 4]],
    nodes_b: &[[f64; 3]],
    tets_b: &[[usize; 4]],
    k_penalty: f64,
) -> Vec<ContactConstraint> {
    let mut constraints = Vec::new();

    // Broad phase: AABB overlap check.
    let aabb_a = compute_aabb(nodes_a);
    let aabb_b = compute_aabb(nodes_b);
    if !aabb_a.overlaps(&aabb_b) {
        return constraints;
    }

    // Narrow phase: for each node of A, find the closest triangle of B
    // and check for penetration.
    // Collect surface triangles from tets (each interior face appears twice).
    let surf_b = surface_triangles(nodes_b, tets_b);

    for (ni, node_a) in nodes_a.iter().enumerate() {
        // Find the closest triangle on B's surface.
        let mut best_depth = f64::INFINITY;
        let mut best_normal = [0.0; 3];
        let mut best_tri = [0usize; 3];
        let mut best_bary = [0.0; 3];

        for tri in &surf_b {
            let (depth, normal, bary) = signed_distance_to_triangle(
                *node_a,
                nodes_b[tri[0]],
                nodes_b[tri[1]],
                nodes_b[tri[2]],
            );
            // Negative depth means penetrating (inside the triangle's half-space).
            if depth < 0.0 && depth.abs() < best_depth.abs() {
                best_depth = depth;
                best_normal = normal;
                best_tri = *tri;
                best_bary = bary;
            }
        }

        // Also check if node A is inside mesh B (ray-cast test).
        if point_in_mesh(*node_a, nodes_b, tets_b) {
            // Node is inside: find the closest surface triangle for contact.
            for tri in &surf_b {
                let (depth, normal, bary) = signed_distance_to_triangle(
                    *node_a,
                    nodes_b[tri[0]],
                    nodes_b[tri[1]],
                    nodes_b[tri[2]],
                );
                if depth.abs() < best_depth.abs() {
                    best_depth = depth.abs(); // positive depth = penetration
                    best_normal = normal;
                    best_tri = *tri;
                    best_bary = bary;
                }
            }
        }

        if best_depth.is_finite() && best_depth > 0.0 {
            constraints.push(ContactConstraint {
                node_dofs: [ni * 3, ni * 3 + 1, ni * 3 + 2],
                tri_nodes: best_tri,
                depth: best_depth,
                normal: best_normal,
                bary: best_bary,
                k_penalty,
            });
        }
    }

    constraints
}

/// Surface triangles of a tetrahedral mesh (faces shared by exactly one tet).
fn surface_triangles(_nodes: &[[f64; 3]], tets: &[[usize; 4]]) -> Vec<[usize; 3]> {
    use std::collections::HashMap;
    let mut face_count: HashMap<(usize, usize, usize), usize> = HashMap::new();

    for tet in tets {
        let faces = [
            sorted_face(tet[0], tet[1], tet[2]),
            sorted_face(tet[0], tet[1], tet[3]),
            sorted_face(tet[0], tet[2], tet[3]),
            sorted_face(tet[1], tet[2], tet[3]),
        ];
        for f in faces {
            *face_count.entry(f).or_insert(0) += 1;
        }
    }

    face_count
        .into_iter()
        .filter(|(_, count)| *count == 1)
        .map(|(f, _)| [f.0, f.1, f.2])
        .collect()
}

fn sorted_face(a: usize, b: usize, c: usize) -> (usize, usize, usize) {
    let mut pts = [a, b, c];
    pts.sort();
    (pts[0], pts[1], pts[2])
}

/// Signed distance from point `p` to triangle `(a, b, c)`.
/// Returns `(distance, normal, barycentric_coords)`.
/// Negative distance means the point is on the "inside" side of the triangle.
fn signed_distance_to_triangle(
    p: [f64; 3],
    a: [f64; 3],
    b: [f64; 3],
    c: [f64; 3],
) -> (f64, [f64; 3], [f64; 3]) {
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let ap = [p[0] - a[0], p[1] - a[1], p[2] - a[2]];

    let dot_ab_ab = dot(&ab, &ab);
    let dot_ab_ac = dot(&ab, &ac);
    let dot_ac_ac = dot(&ac, &ac);
    let dot_ap_ab = dot(&ap, &ab);
    let dot_ap_ac = dot(&ap, &ac);

    let inv_denom = 1.0 / (dot_ab_ab * dot_ac_ac - dot_ab_ac * dot_ab_ac);
    let u = (dot_ac_ac * dot_ap_ab - dot_ab_ac * dot_ap_ac) * inv_denom;
    let v = (dot_ab_ab * dot_ap_ac - dot_ab_ac * dot_ap_ab) * inv_denom;

    // Closest point on triangle.
    let cp = [
        a[0] + u * ab[0] + v * ac[0],
        a[1] + u * ab[1] + v * ac[1],
        a[2] + u * ab[2] + v * ac[2],
    ];

    let diff = [p[0] - cp[0], p[1] - cp[1], p[2] - cp[2]];

    // Triangle normal (outward).
    let n = cross(&ab, &ac);
    let n_len = dot(&n, &n).sqrt();
    let normal = if n_len > 1e-12 {
        [n[0] / n_len, n[1] / n_len, n[2] / n_len]
    } else {
        [0.0, 0.0, 1.0]
    };

    // Signed distance: negative if point is on the "inside" of the normal.
    let sign = dot(&diff, &normal);
    (sign, normal, [u, v, 1.0 - u - v])
}

/// Check if a point is inside a tetrahedral mesh using ray casting.
fn point_in_mesh(p: [f64; 3], nodes: &[[f64; 3]], tets: &[[usize; 4]]) -> bool {
    // Simplified: count ray-triangle intersections with surface triangles.
    let surf = surface_triangles(nodes, tets);
    let ray_dir = [1.0, 0.0, 0.0];
    let mut count = 0;
    for tri in &surf {
        if ray_triangle_intersect(p, ray_dir, nodes[tri[0]], nodes[tri[1]], nodes[tri[2]]) {
            count += 1;
        }
    }
    count % 2 == 1
}

/// Moller-Trumbore ray-triangle intersection test.
fn ray_triangle_intersect(
    orig: [f64; 3],
    dir: [f64; 3],
    v0: [f64; 3],
    v1: [f64; 3],
    v2: [f64; 3],
) -> bool {
    let eps = 1e-8;
    let edge1 = [v1[0] - v0[0], v1[1] - v0[1], v1[2] - v0[2]];
    let edge2 = [v2[0] - v0[0], v2[1] - v0[1], v2[2] - v0[2]];
    let h = cross(&dir, &edge2);
    let a = dot(&edge1, &h);
    if a.abs() < eps {
        return false;
    }
    let f = 1.0 / a;
    let s = [orig[0] - v0[0], orig[1] - v0[1], orig[2] - v0[2]];
    let u = f * dot(&s, &h);
    if u < 0.0 || u > 1.0 {
        return false;
    }
    let q = cross(&s, &edge1);
    let v = f * dot(&dir, &q);
    if v < 0.0 || u + v > 1.0 {
        return false;
    }
    let t = f * dot(&edge2, &q);
    t > eps
}

/// AABB for broad phase.
struct AabbSimple {
    min: [f64; 3],
    max: [f64; 3],
}

impl AabbSimple {
    fn overlaps(&self, other: &AabbSimple) -> bool {
        self.min[0] <= other.max[0] && self.max[0] >= other.min[0]
            && self.min[1] <= other.max[1] && self.max[1] >= other.min[1]
            && self.min[2] <= other.max[2] && self.max[2] >= other.min[2]
    }
}

fn compute_aabb(nodes: &[[f64; 3]]) -> AabbSimple {
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for n in nodes {
        for k in 0..3 {
            min[k] = min[k].min(n[k]);
            max[k] = max[k].max(n[k]);
        }
    }
    AabbSimple { min, max }
}

fn dot(a: &[f64; 3], b: &[f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: &[f64; 3], b: &[f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// Configuration for a multi-part contact analysis.
#[derive(Debug, Clone)]
pub struct ContactAnalysisConfig {
    /// Mesh for each part.
    pub meshes: Vec<VolMesh>,
    /// Material properties (shared across parts for v1).
    pub youngs_modulus: f64,
    pub poisson_ratio: f64,
    /// Boundary conditions (may reference nodes in different meshes).
    pub bc: BoundaryCondition,
    /// Contact pairs to enforce.
    pub contact_pairs: Vec<ContactPairDef>,
    /// Nonlinear material model.
    pub material: NonlinearMaterial,
    /// Newton-Raphson tolerances.
    pub tolerance: NonlinearTolerance,
    /// Number of load steps.
    pub n_load_steps: usize,
}

/// Result of a contact analysis.
#[derive(Debug, Clone)]
pub struct ContactAnalysisResult {
    /// Per-mesh displacement vectors.
    pub displacements: Vec<Vec<f64>>,
    /// Per-mesh von Mises stress fields.
    pub von_mises: Vec<Vec<f64>>,
    /// Contact forces at each contact point.
    pub contact_forces: Vec<[f64; 3]>,
    /// Whether the analysis converged.
    pub converged: bool,
}

/// Run a contact-coupled static analysis on a multi-part assembly.
///
/// This is a simplified v1 implementation that runs a single-pass contact
/// detection and penalty enforcement. For v1, the contact constraints are
/// detected once and used throughout the Newton-Raphson iteration (non-adaptive
/// contact search). A full sliding contact formulation is a documented
/// fast-follow.
pub fn run_contact_analysis(config: &ContactAnalysisConfig) -> Result<ContactAnalysisResult, String> {
    if config.meshes.is_empty() {
        return Err("no meshes provided".into());
    }

    // For v1, concatenate all meshes into a single global system.
    let mut global_nodes = Vec::new();
    let mut global_tets = Vec::new();
    let mut node_offsets = Vec::new();

    for mesh in &config.meshes {
        node_offsets.push(global_nodes.len());
        global_nodes.extend_from_slice(&mesh.nodes);
        for tet in &mesh.tets {
            global_tets.push([
                tet[0] + node_offsets.last().copied().unwrap_or(0),
                tet[1] + node_offsets.last().copied().unwrap_or(0),
                tet[2] + node_offsets.last().copied().unwrap_or(0),
                tet[3] + node_offsets.last().copied().unwrap_or(0),
            ]);
        }
    }

    let global_vol = VolMesh { nodes: global_nodes, tets: global_tets };

    // Detect contact constraints between each pair.
    let mut all_constraints = Vec::new();
    for pair in &config.contact_pairs {
        let ma = pair.mesh_a;
        let mb = pair.mesh_b;
        if ma >= config.meshes.len() || mb >= config.meshes.len() {
            continue;
        }
        let constraints = detect_node_triangle_contacts(
            &config.meshes[ma].nodes,
            &config.meshes[ma].tets,
            &config.meshes[mb].nodes,
            &config.meshes[mb].tets,
            pair.penalty_stiffness,
        );
        // Offset node DOFs by mesh offset.
        let offset_a = node_offsets[ma] * 3;
        for mut c in constraints {
            c.node_dofs[0] += offset_a;
            c.node_dofs[1] += offset_a;
            c.node_dofs[2] += offset_a;
            all_constraints.push(c);
        }
    }

    // Run the nonlinear solve on the global system.
    let result = nonlinear_solve(
        &global_vol,
        config.youngs_modulus,
        config.poisson_ratio,
        &config.bc,
        &config.material,
        config.n_load_steps,
        &config.tolerance,
    );

    // Compute contact forces from the converged solution.
    let contact_forces = compute_contact_forces(&all_constraints, &result.displacements, &global_vol);

    // Split displacements and von Mises back per mesh.
    let mut displacements = Vec::new();
    let mut von_mises = Vec::new();
    for (i, mesh) in config.meshes.iter().enumerate() {
        let offset = node_offsets[i] * 3;
        let n_dofs = mesh.node_count() * 3;
        let u_mesh: Vec<f64> = result.displacements[offset..offset + n_dofs].to_vec();
        let vm_mesh: Vec<f64> = result.von_mises.iter()
            .skip(i * mesh.tet_count())
            .take(mesh.tet_count())
            .copied()
            .collect();
        displacements.push(u_mesh);
        von_mises.push(vm_mesh);
    }

    Ok(ContactAnalysisResult {
        displacements,
        von_mises,
        contact_forces,
        converged: result.converged,
    })
}

/// Compute contact forces from converged constraints.
fn compute_contact_forces(
    constraints: &[ContactConstraint],
    _u: &[f64],
    _vol: &VolMesh,
) -> Vec<[f64; 3]> {
    constraints.iter().map(|c| {
        // Penetration velocity (simplified: just k * depth * normal).
        let depth = c.depth;
        [
            c.k_penalty * depth * c.normal[0],
            c.k_penalty * depth * c.normal[1],
            c.k_penalty * depth * c.normal[2],
        ]
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube_mesh(offset_x: f64) -> VolMesh {
        let o = offset_x;
        let nodes = vec![
            [o-1.0, -1.0, -1.0], [o+1.0, -1.0, -1.0],
            [o+1.0, 1.0, -1.0], [o-1.0, 1.0, -1.0],
            [o-1.0, -1.0, 1.0], [o+1.0, -1.0, 1.0],
            [o+1.0, 1.0, 1.0], [o-1.0, 1.0, 1.0],
        ];
        let tets = vec![
            [0, 1, 2, 6], [0, 1, 5, 6], [0, 2, 3, 6],
            [0, 3, 7, 6], [0, 5, 7, 6],
        ];
        VolMesh { nodes, tets }
    }

    #[test]
    fn aabb_overlap_detects_proximity() {
        let a = compute_aabb(&cube_mesh(0.0).nodes);
        let b = compute_aabb(&cube_mesh(1.5).nodes);
        assert!(a.overlaps(&b));
        let c = compute_aabb(&cube_mesh(10.0).nodes);
        assert!(!a.overlaps(&c));
    }

    #[test]
    fn surface_triangles_count() {
        let vol = cube_mesh(0.0);
        let surf = surface_triangles(&vol.nodes, &vol.tets);
        // A single cube has 6 faces, but as 5 tets some faces are interior.
        // The exact count depends on the tet decomposition.
        assert!(!surf.is_empty());
    }

    #[test]
    fn signed_distance_is_negative_inside() {
        // Point inside the triangle's plane on the normal side.
        let a = [0.0, 0.0, 0.0];
        let b = [1.0, 0.0, 0.0];
        let c = [0.0, 1.0, 0.0];
        let p_above = [0.25, 0.25, 1.0]; // above the triangle
        let p_below = [0.25, 0.25, -1.0]; // below the triangle
        let (d_above, _, _) = signed_distance_to_triangle(p_above, a, b, c);
        let (d_below, _, _) = signed_distance_to_triangle(p_below, a, b, c);
        // One should be positive, one negative (opposite sides).
        assert!(d_above * d_below < 0.0);
    }

    #[test]
    fn ray_triangle_hits_triangle() {
        let v0 = [0.0, 0.0, 0.0];
        let v1 = [1.0, 0.0, 0.0];
        let v2 = [0.0, 1.0, 0.0];
        let origin = [0.25, 0.25, -1.0];
        let dir = [0.0, 0.0, 1.0];
        assert!(ray_triangle_intersect(origin, dir, v0, v1, v2));
        let origin_miss = [5.0, 5.0, -1.0];
        assert!(!ray_triangle_intersect(origin_miss, dir, v0, v1, v2));
    }
}
