// SPDX-License-Identifier: MIT OR Apache-2.0

//! WASM-compatible simulation API for in-browser execution.
//!
//! This module provides thin wrappers around the core simulation functions
//! that are safe to call from JavaScript via `wasm-bindgen`. It uses the
//! dense LU solver (no `faer`/`rayon` dependency) and operates on flat
//! arrays for easy FFI.
//!
//! # Usage from JavaScript
//!
//! ```javascript
//! import init, { run_linear_analysis } from 'tpt-vertex-simulation';
//! await init();
//! const result = run_linear_analysis(nodes, tets, youngs_modulus, poisson_ratio, ...);
//! ```

use crate::assembly::GlobalSystem;
use crate::bc::{BoundaryCondition, PointLoad};
use crate::element::tet_stiffness;
use crate::material::elastic_matrix;
use crate::mesh::VolMesh;
use crate::solve::solve_dense;

/// Result of a linear analysis, returned as flat arrays for WASM interop.
#[derive(Debug, Clone)]
pub struct WasmAnalysisResult {
    /// Displacement vector (length = n_nodes * 3).
    pub displacements: Vec<f64>,
    /// Per-element von Mises stress (length = n_tets).
    pub von_mises: Vec<f64>,
    /// Maximum displacement magnitude.
    pub max_displacement: f64,
    /// Maximum von Mises stress.
    pub max_von_mises: f64,
}

/// Run a linear static analysis from flat arrays.
///
/// - `nodes`: flat `[x0,y0,z0, x1,y1,z1, ...]` (length = n_nodes * 3)
/// - `tets`: flat `[n0,n1,n2,n3, ...]` (length = n_tets * 4)
/// - `fixed_nodes`: indices of constrained nodes
/// - `loads`: flat `[node_idx, fx, fy, fz, ...]` (length = n_loads * 4)
///
/// Returns `None` if the mesh is invalid.
pub fn run_linear_analysis(
    nodes: &[f64],
    tets: &[usize],
    e: f64,
    nu: f64,
    fixed_nodes: &[usize],
    loads: &[f64],
) -> Option<WasmAnalysisResult> {
    // Parse flat arrays into VolMesh.
    if nodes.len() % 3 != 0 || tets.len() % 4 != 0 {
        return None;
    }
    let n_nodes = nodes.len() / 3;
    let n_tets = tets.len() / 4;

    let mesh_nodes: Vec<[f64; 3]> = (0..n_nodes)
        .map(|i| [nodes[i * 3], nodes[i * 3 + 1], nodes[i * 3 + 2]])
        .collect();
    let mesh_tets: Vec<[usize; 4]> = (0..n_tets)
        .map(|i| [tets[i * 4], tets[i * 4 + 1], tets[i * 4 + 2], tets[i * 4 + 3]])
        .collect();

    let vol = VolMesh { nodes: mesh_nodes, tets: mesh_tets };

    // Parse boundary conditions.
    let mut bc = BoundaryCondition::new();
    for &n in fixed_nodes {
        bc = bc.fix_node(n);
    }
    // Loads: [node_idx, fx, fy, fz, ...]
    for chunk in loads.chunks(4) {
        if chunk.len() < 4 {
            break;
        }
        bc = bc.with_load(PointLoad {
            node: chunk[0] as usize,
            fx: chunk[1],
            fy: chunk[2],
            fz: chunk[3],
        });
    }

    // Assemble and solve.
    let d = elastic_matrix(e, nu);
    let n_dofs = n_nodes * 3;
    let mut k = vec![0.0; n_dofs * n_dofs];
    let mut f = vec![0.0; n_dofs];

    for tet in &vol.tets {
        let nodes = [
            vol.nodes[tet[0]], vol.nodes[tet[1]],
            vol.nodes[tet[2]], vol.nodes[tet[3]],
        ];
        let ke = tet_stiffness(&nodes, &d);
        let gdof = [
            GlobalSystem::dof(tet[0], 0), GlobalSystem::dof(tet[0], 1), GlobalSystem::dof(tet[0], 2),
            GlobalSystem::dof(tet[1], 0), GlobalSystem::dof(tet[1], 1), GlobalSystem::dof(tet[1], 2),
            GlobalSystem::dof(tet[2], 0), GlobalSystem::dof(tet[2], 1), GlobalSystem::dof(tet[2], 2),
            GlobalSystem::dof(tet[3], 0), GlobalSystem::dof(tet[3], 1), GlobalSystem::dof(tet[3], 2),
        ];
        for i in 0..12 {
            for j in 0..12 {
                k[gdof[i] * n_dofs + gdof[j]] += ke[i][j];
            }
        }
    }

    for load in &bc.loads {
        let gdof = GlobalSystem::dof(load.node, 0);
        f[gdof] += load.fx;
        f[gdof + 1] += load.fy;
        f[gdof + 2] += load.fz;
    }

    // Penalty constraints.
    let mut kmax: f64 = 0.0;
    for &v in &k {
        kmax = kmax.max(v.abs());
    }
    let penalty = kmax * 1e12 + 1.0;
    for &node in &bc.fixed_nodes {
        for a in 0..3 {
            let d0 = GlobalSystem::dof(node, a);
            for j in 0..n_dofs {
                k[d0 * n_dofs + j] = 0.0;
                k[j * n_dofs + d0] = 0.0;
            }
            k[d0 * n_dofs + d0] = penalty;
            f[d0] = 0.0;
        }
    }

    let u = solve_dense(&k, &f, n_dofs);
    let von_mises = crate::post::von_mises_field(&vol, e, nu, &u);

    let max_displacement = (0..n_nodes)
        .map(|n| {
            let d = crate::post::displacement_at(&vol, &u, n);
            (d[0].powi(2) + d[1].powi(2) + d[2].powi(2)).sqrt()
        })
        .fold(0.0, f64::max);
    let max_von_mises = von_mises.iter().cloned().fold(0.0, f64::max);

    Some(WasmAnalysisResult {
        displacements: u,
        von_mises,
        max_displacement,
        max_von_mises,
    })
}
