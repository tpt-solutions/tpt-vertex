// SPDX-License-Identifier: MIT OR Apache-2.0

//! Adaptive mesh refinement (h-refinement) for tetrahedral meshes.
//!
//! Provides a Zienkiewicz-Zhu (ZZ) superconvergent patch recovery error
//! estimator and element-wise refinement by longest-edge bisection. The
//! workflow is:
//!
//! 1. Solve the FEA problem on the current mesh.
//! 2. Estimate the error per element via [`zz_error_estimator`].
//! 3. Mark elements for refinement via [`mark_elements`].
//! 4. Refine the marked elements via [`refine_mesh`].
//! 5. Re-solve and repeat until the estimated error is below a tolerance
//!    or the maximum refinement level is reached.

use crate::mesh::VolMesh;
use crate::post::{element_stress, StressTensor};

/// Per-element error estimate (scalar, in stress units).
#[derive(Debug, Clone)]
pub struct ElementError {
    /// Element index in the mesh.
    pub element: usize,
    /// Estimated energy-norm error for this element.
    pub error: f64,
    /// Relative error (error / reference stress norm).
    pub relative_error: f64,
}

/// Zienkiewicz-Zhu (ZZ) error estimator.
///
/// For each element, computes the stress from the FE solution, then
/// estimates the "true" stress by smoothing (nodal averaging of element
/// stresses). The difference between the raw and smoothed stress gives
/// the error estimate.
///
/// Returns per-element errors sorted by magnitude (descending).
#[allow(clippy::needless_range_loop)] // fixed-size 6-component stress-vector indexing is clearest with range loops
pub fn zz_error_estimator(vol: &VolMesh, e: f64, nu: f64, u: &[f64]) -> Vec<ElementError> {
    let n_tets = vol.tet_count();
    let n_nodes = vol.node_count();

    // Step 1: Compute element stresses.
    let elem_stresses: Vec<StressTensor> = (0..n_tets)
        .map(|t| element_stress(vol, t, e, nu, u))
        .collect();

    // Step 2: Compute nodal average stresses (smoothed field).
    // Each node accumulates the stresses of its adjacent elements.
    let mut node_stress_sum = vec![[0.0; 6]; n_nodes];
    let mut node_count = vec![0usize; n_nodes];
    for (t, tet) in vol.tets.iter().enumerate() {
        let s = &elem_stresses[t];
        let vals = [s.sx, s.sy, s.sz, s.txy, s.tyz, s.tzx];
        for &n in tet {
            for k in 0..6 {
                node_stress_sum[n][k] += vals[k];
            }
            node_count[n] += 1;
        }
    }
    // Average.
    let node_smoothed: Vec<[f64; 6]> = (0..n_nodes)
        .map(|n| {
            if node_count[n] > 0 {
                let c = node_count[n] as f64;
                let mut out = [0.0; 6];
                for k in 0..6 {
                    out[k] = node_stress_sum[n][k] / c;
                }
                out
            } else {
                [0.0; 6]
            }
        })
        .collect();

    // Step 3: For each element, interpolate the smoothed stress at the
    // element centroid and compute the difference from the raw FE stress.
    // For linear tets, the centroid average is just the mean of the 4 nodal
    // smoothed values.
    let mut errors: Vec<ElementError> = (0..n_tets)
        .map(|t| {
            let tet = vol.tets[t];
            let raw = &elem_stresses[t];
            let raw_vals = [raw.sx, raw.sy, raw.sz, raw.txy, raw.tyz, raw.tzx];

            // Smoothed stress at centroid = average of node values.
            let mut smoothed = [0.0; 6];
            for &n in &tet {
                for k in 0..6 {
                    smoothed[k] += node_smoothed[n][k];
                }
            }
            for k in 0..6 {
                smoothed[k] /= 4.0;
            }

            // Energy-norm error: ||σ_FE - σ_smoothed||_D
            // Simplified: L2 norm of the stress difference.
            let mut diff_sq: f64 = 0.0;
            for k in 0..6 {
                diff_sq += (raw_vals[k] - smoothed[k]).powi(2);
            }
            let error = diff_sq.sqrt();

            // Reference norm: the element's own stress magnitude.
            let mut ref_sq: f64 = 0.0;
            for k in 0..6 {
                ref_sq += raw_vals[k].powi(2);
            }
            let ref_norm = ref_sq.sqrt().max(1e-12);
            let relative_error = error / ref_norm;

            ElementError {
                element: t,
                error,
                relative_error,
            }
        })
        .collect();

    errors.sort_by(|a, b| {
        b.error
            .partial_cmp(&a.error)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    errors
}

/// Mark elements for refinement based on an error threshold.
///
/// Marks elements whose relative error exceeds `threshold` (default 0.1 =
/// 10%). Returns the set of element indices to refine.
pub fn mark_elements(errors: &[ElementError], threshold: f64) -> Vec<usize> {
    errors
        .iter()
        .filter(|e| e.relative_error > threshold)
        .map(|e| e.element)
        .collect()
}

/// Refine a tetrahedral mesh by bisecting marked elements along their
/// longest edge. Each marked tet is split into two tets; shared edges are
/// also split to maintain conformity (1-irregular mesh rule).
///
/// Returns the refined mesh. Elements that share a refined edge with a
/// marked element are also refined to avoid hanging nodes.
pub fn refine_mesh(vol: &VolMesh, marked: &[usize]) -> VolMesh {
    if marked.is_empty() {
        return vol.clone();
    }

    // Build edge-to-elements adjacency.
    let mut edge_map: std::collections::HashMap<(usize, usize), Vec<usize>> =
        std::collections::HashMap::new();
    for (t, tet) in vol.tets.iter().enumerate() {
        let edges = tet_edges(tet);
        for e in &edges {
            edge_map.entry(*e).or_default().push(t);
        }
    }

    // Find all edges that need splitting: edges of marked elements.
    let mut split_edges: std::collections::HashSet<(usize, usize)> =
        std::collections::HashSet::new();
    for &t in marked {
        let tet = vol.tets[t];
        for e in tet_edges(&tet) {
            split_edges.insert(e);
        }
    }

    // Also split edges of elements adjacent to marked elements (conformity).
    let mut frontier: Vec<(usize, usize)> = split_edges.iter().copied().collect();
    while let Some(e) = frontier.pop() {
        if let Some(adjacent) = edge_map.get(&e) {
            for &t in adjacent {
                let tet = vol.tets[t];
                for te in tet_edges(&tet) {
                    if !split_edges.contains(&te) {
                        split_edges.insert(te);
                        frontier.push(te);
                    }
                }
            }
        }
    }

    // Create midpoint nodes for each split edge.
    let mut new_nodes = vol.nodes.clone();
    let mut edge_midpoint: std::collections::HashMap<(usize, usize), usize> =
        std::collections::HashMap::new();
    for &(a, b) in &split_edges {
        let mid = [
            (vol.nodes[a][0] + vol.nodes[b][0]) / 2.0,
            (vol.nodes[a][1] + vol.nodes[b][1]) / 2.0,
            (vol.nodes[a][2] + vol.nodes[b][2]) / 2.0,
        ];
        let idx = new_nodes.len();
        new_nodes.push(mid);
        edge_midpoint.insert((a, b), idx);
        // Also insert the reverse direction.
        edge_midpoint.insert((b, a), idx);
    }

    // Split each tet. If it has any split edge, bisect along the longest
    // split edge. Otherwise, keep it as-is.
    let mut new_tets = Vec::new();
    for tet in vol.tets.iter() {
        let edges = tet_edges(tet);
        let has_split = edges.iter().any(|e| split_edges.contains(e));

        if !has_split {
            new_tets.push(*tet);
            continue;
        }

        // Find the longest split edge.
        let mut best_edge = edges[0];
        let mut best_len = 0.0;
        for &e in &edges {
            if split_edges.contains(&e) {
                let len = edge_length(&vol.nodes[e.0], &vol.nodes[e.1]);
                if len > best_len {
                    best_len = len;
                    best_edge = e;
                }
            }
        }

        let mid = edge_midpoint[&best_edge];
        // Bisect: split the tet into two by replacing the two non-edge
        // vertices with the midpoint.
        let (a, b) = best_edge;
        let others: Vec<usize> = tet.iter().filter(|&&n| n != a && n != b).copied().collect();

        if others.len() == 2 {
            let (c, d) = (others[0], others[1]);
            // Two new tets: (a, c, d, mid) and (b, c, d, mid).
            new_tets.push([a, c, d, mid]);
            new_tets.push([b, c, d, mid]);
        } else {
            // Fallback: keep the original tet.
            new_tets.push(*tet);
        }
    }

    VolMesh {
        nodes: new_nodes,
        tets: new_tets,
    }
}

/// Get the 6 edges of a tetrahedron as sorted (min, max) pairs.
fn tet_edges(tet: &[usize; 4]) -> [(usize, usize); 6] {
    let edges = [
        (tet[0], tet[1]),
        (tet[0], tet[2]),
        (tet[0], tet[3]),
        (tet[1], tet[2]),
        (tet[1], tet[3]),
        (tet[2], tet[3]),
    ];
    edges.map(|(a, b)| if a <= b { (a, b) } else { (b, a) })
}

/// Euclidean distance between two 3D points.
fn edge_length(a: &[f64; 3], b: &[f64; 3]) -> f64 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}

/// Run adaptive mesh refinement until the maximum relative error drops
/// below `tolerance` or `max_levels` refinement levels are reached.
///
/// Returns the final mesh and the error history per level.
pub fn adaptive_refine(
    solid: &tpt_vertex_kernel::geometry::solid::Solid,
    e: f64,
    nu: f64,
    bc: &crate::bc::BoundaryCondition,
    initial_edge: f64,
    tolerance: f64,
    max_levels: usize,
) -> Result<(VolMesh, Vec<Vec<ElementError>>), String> {
    let mut vol = crate::mesh::tetrahedralize(solid, initial_edge)?;
    let mut history = Vec::new();

    for _level in 0..max_levels {
        let system = crate::assembly::assemble(&vol, e, nu, bc);
        let u = system.solve();
        let errors = zz_error_estimator(&vol, e, nu, &u);
        history.push(errors.clone());

        let max_rel = errors
            .iter()
            .map(|e| e.relative_error)
            .fold(0.0f64, f64::max);
        if max_rel <= tolerance {
            break;
        }

        let marked = mark_elements(&errors, tolerance);
        if marked.is_empty() {
            break;
        }
        vol = refine_mesh(&vol, &marked);
    }

    Ok((vol, history))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube() -> VolMesh {
        let nodes = vec![
            [-1.0, -1.0, -1.0],
            [1.0, -1.0, -1.0],
            [1.0, 1.0, -1.0],
            [-1.0, 1.0, -1.0],
            [-1.0, -1.0, 1.0],
            [1.0, -1.0, 1.0],
            [1.0, 1.0, 1.0],
            [-1.0, 1.0, 1.0],
        ];
        // Simple 5-tet decomposition of the cube.
        let tets = vec![
            [0, 1, 2, 6],
            [0, 1, 5, 6],
            [0, 2, 3, 6],
            [0, 3, 7, 6],
            [0, 5, 7, 6],
        ];
        VolMesh { nodes, tets }
    }

    #[test]
    fn zz_estimator_returns_nonempty() {
        let vol = cube();
        let e = 200_000.0;
        let nu = 0.3;
        // Solve a trivial system (no loads, no fixed nodes — zero solution).
        let n_dofs = vol.node_count() * 3;
        let u = vec![0.0; n_dofs];
        let errors = zz_error_estimator(&vol, e, nu, &u);
        assert_eq!(errors.len(), vol.tet_count());
    }

    #[test]
    fn mark_elements_selects_high_error() {
        let errors = vec![
            ElementError {
                element: 0,
                error: 10.0,
                relative_error: 0.5,
            },
            ElementError {
                element: 1,
                error: 1.0,
                relative_error: 0.01,
            },
            ElementError {
                element: 2,
                error: 5.0,
                relative_error: 0.2,
            },
        ];
        let marked = mark_elements(&errors, 0.1);
        assert!(marked.contains(&0));
        assert!(!marked.contains(&1));
        assert!(marked.contains(&2));
    }

    #[test]
    fn refine_mesh_increases_element_count() {
        let vol = cube();
        let marked = vec![0]; // mark first tet
        let refined = refine_mesh(&vol, &marked);
        assert!(refined.tet_count() > vol.tet_count());
        assert!(refined.node_count() >= vol.node_count());
    }

    #[test]
    fn tet_edges_are_sorted() {
        let tet = [3, 0, 2, 1];
        let edges = tet_edges(&tet);
        for (a, b) in &edges {
            assert!(a <= b, "edge ({a}, {b}) not sorted");
        }
        assert_eq!(edges.len(), 6);
    }
}
