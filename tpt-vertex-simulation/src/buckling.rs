// SPDX-License-Identifier: MIT OR Apache-2.0

//! Linear eigenvalue buckling analysis.
//!
//! Solves the generalized eigenvalue problem `(K + λ K_geo) φ = 0` to find
//! critical buckling load factors `λ`. The smallest positive `λ` is the
//! critical multiplier: the structure buckles at `λ × F_applied`.
//!
//! `K` is the standard elastic stiffness matrix (same as static FEA). `K_geo`
//! is the geometric (initial-stress) stiffness matrix assembled from the
//! membrane stresses of a pre-buckling linear-static solution. The eigenvalue
//! problem is reduced to the free DOFs (fixed nodes removed) and solved with
//! the same Jacobi eigensolver used for modal analysis.

use crate::assembly::GlobalSystem;
use crate::bc::BoundaryCondition;
use crate::element::{shape_gradients, tet_stiffness, tet_volume};
use crate::material::elastic_matrix;
use crate::mesh::VolMesh;
use crate::modal::jacobi_eigen;
use crate::post::element_stress;

/// Result of a buckling analysis.
#[derive(Debug, Clone)]
pub struct BucklingResult {
    /// Critical buckling load factors (ascending). The first positive value
    /// is the Euler buckling multiplier.
    pub load_factors: Vec<f64>,
    /// Corresponding buckling mode shapes (displacement over free DOFs).
    pub mode_shapes: Vec<Vec<f64>>,
    /// Global DOF indices that each mode-shape entry corresponds to.
    pub free_dofs: Vec<usize>,
}

/// Compute the 12×12 geometric stiffness matrix for a tetrahedron given its
/// nodal positions and the element stress tensor (Voigt: `[σxx, σyy, σzz,
/// τxy, τyz, τzx]`). Uses the standard formula for constant-stress tets.
///
/// The geometric stiffness relates incremental rotations to the existing
/// membrane stress state: `K_geo u = (σ · ∇u) · u` in the weak form.
#[allow(clippy::needless_range_loop)]
pub fn geometric_stiffness(nodes: &[[f64; 3]; 4], sigma: [f64; 6]) -> [[f64; 12]; 12] {
    let g = shape_gradients(nodes);
    let vol = tet_volume(nodes).abs();

    // B-bar (3×12) maps nodal displacements to the rotation gradient tensor
    // (antisymmetric part of ∇u). For a linear tet the gradients are constant.
    // We use the 6×12 B matrix (strain-displacement) and extract the rotation
    // coupling through the stress array.
    //
    // Simplified approach: K_geo = V · Gᵀ σ̂ G, where G is a 3×12 matrix of
    // shape-function gradients arranged for the rotation operator and σ̂ is the
    // 3×3 stress tensor.
    //
    // For each node pair (i,j), the 3×3 block of K_geo is the scalar
    // bilinear form `g_iᵀ σ̂ g_j` times the 3×3 identity — *not* the outer
    // product `(σ̂ · g_i) g_jᵀ`. The outer-product form is asymmetric under
    // swapping (i,a)<->(j,b) in general (only its trace equals the correct
    // scalar), which breaks the physically-required symmetry of K_geo.
    //   K_geo_{ij} = V * (g_iᵀ σ̂ g_j) * I_3
    let sx = sigma[0];
    let sy = sigma[1];
    let sz = sigma[2];
    let txy = sigma[3];
    let tyz = sigma[4];
    let tzx = sigma[5];

    // 3×3 stress tensor (symmetric).
    let s = [[sx, txy, tzx], [txy, sy, tyz], [tzx, tyz, sz]];

    let mut k_geo = [[0.0; 12]; 12];
    for i in 0..4 {
        for j in 0..4 {
            // Scalar bilinear form g_iᵀ σ̂ g_j, applied to the 3×3 identity
            // block for node pair (i, j) so K_geo is symmetric.
            let mut s_gj = [0.0; 3];
            for a in 0..3 {
                for c in 0..3 {
                    s_gj[a] += s[a][c] * g[j][c];
                }
            }
            let mut scalar = 0.0;
            for a in 0..3 {
                scalar += g[i][a] * s_gj[a];
            }
            for a in 0..3 {
                k_geo[i * 3 + a][j * 3 + a] += vol * scalar;
            }
        }
    }
    k_geo
}

/// Assemble the global geometric stiffness matrix from the pre-buckling stress
/// state. The stress in each element is computed from the linear-static
/// displacement field `u`.
pub fn assemble_geometric(vol: &VolMesh, e: f64, nu: f64, u: &[f64]) -> Vec<f64> {
    let n_dofs = vol.node_count() * 3;
    let mut k_geo = vec![0.0; n_dofs * n_dofs];

    for (t, tet) in vol.tets.iter().enumerate() {
        let nodes = [
            vol.nodes[tet[0]],
            vol.nodes[tet[1]],
            vol.nodes[tet[2]],
            vol.nodes[tet[3]],
        ];
        let stress = element_stress(vol, t, e, nu, u);
        let sigma = [
            stress.sx, stress.sy, stress.sz, stress.txy, stress.tyz, stress.tzx,
        ];
        let ke_geo = geometric_stiffness(&nodes, sigma);

        let gdof = [
            GlobalSystem::dof(tet[0], 0),
            GlobalSystem::dof(tet[0], 1),
            GlobalSystem::dof(tet[0], 2),
            GlobalSystem::dof(tet[1], 0),
            GlobalSystem::dof(tet[1], 1),
            GlobalSystem::dof(tet[1], 2),
            GlobalSystem::dof(tet[2], 0),
            GlobalSystem::dof(tet[2], 1),
            GlobalSystem::dof(tet[2], 2),
            GlobalSystem::dof(tet[3], 0),
            GlobalSystem::dof(tet[3], 1),
            GlobalSystem::dof(tet[3], 2),
        ];
        for i in 0..12 {
            for j in 0..12 {
                k_geo[gdof[i] * n_dofs + gdof[j]] += ke_geo[i][j];
            }
        }
    }
    k_geo
}

/// Run linear eigenvalue buckling analysis on `vol` under material `(e, nu)`
/// with the given boundary conditions.
///
/// The procedure is:
/// 1. Solve the linear-static problem `K u = F` to get the pre-buckling stress.
/// 2. Assemble `K_geo` from the pre-buckling stresses.
/// 3. Extract free-DOF submatrices of both `K` and `K_geo`.
/// 4. Solve `(K_ff + λ K_geo_ff) φ = 0` via the standard eigenvalue approach.
///
/// Returns the first `n_modes` (default 6) buckling load factors and mode shapes.
pub fn buckling_analysis(
    vol: &VolMesh,
    e: f64,
    nu: f64,
    bc: &BoundaryCondition,
    n_modes: usize,
) -> BucklingResult {
    let n_nodes = vol.node_count();
    let n_dofs = n_nodes * 3;
    let d = elastic_matrix(e, nu);

    // 1. Assemble and solve the pre-buckling linear-static problem.
    let mut k = vec![0.0; n_dofs * n_dofs];
    let mut f = vec![0.0; n_dofs];
    for tet in &vol.tets {
        let nodes = [
            vol.nodes[tet[0]],
            vol.nodes[tet[1]],
            vol.nodes[tet[2]],
            vol.nodes[tet[3]],
        ];
        let ke = tet_stiffness(&nodes, &d);
        let gdof = [
            GlobalSystem::dof(tet[0], 0),
            GlobalSystem::dof(tet[0], 1),
            GlobalSystem::dof(tet[0], 2),
            GlobalSystem::dof(tet[1], 0),
            GlobalSystem::dof(tet[1], 1),
            GlobalSystem::dof(tet[1], 2),
            GlobalSystem::dof(tet[2], 0),
            GlobalSystem::dof(tet[2], 1),
            GlobalSystem::dof(tet[2], 2),
            GlobalSystem::dof(tet[3], 0),
            GlobalSystem::dof(tet[3], 1),
            GlobalSystem::dof(tet[3], 2),
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
    // Penalty for fixed DOFs.
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
    let u = crate::solve::solve_dense(&k, &f, n_dofs);

    // 2. Assemble geometric stiffness from pre-buckling stresses.
    let k_geo = assemble_geometric(vol, e, nu, &u);

    // 3. Identify free DOFs and extract submatrices.
    let mut free_dofs = Vec::new();
    for n in 0..n_nodes {
        if !bc.fixed_nodes.contains(&n) {
            for a in 0..3 {
                free_dofs.push(GlobalSystem::dof(n, a));
            }
        }
    }
    let nf = free_dofs.len();

    let mut kff = vec![0.0; nf * nf];
    let mut gff = vec![0.0; nf * nf];
    for i in 0..nf {
        for j in 0..nf {
            kff[i * nf + j] = k[free_dofs[i] * n_dofs + free_dofs[j]];
            gff[i * nf + j] = k_geo[free_dofs[i] * n_dofs + free_dofs[j]];
        }
    }

    // 4. Solve the generalized eigenvalue problem K φ = -λ G φ
    //    => (K + λ G) φ = 0.
    //    Transform: K' = M^{-1/2} K M^{-1/2}, G' = M^{-1/2} G M^{-1/2}
    //    where M is the diagonal of K (or identity — for simplicity we use
    //    the standard Jacobi on K directly and extract eigenvalues of
    //    K^{-1} G, but since we have a Jacobi eigensolver for symmetric
    //    matrices, we solve K φ = λ (-G) φ by combining into a single
    //    symmetric system.
    //
    //    Simpler approach: solve the standard eigenvalue problem on
    //    K_ff^{-1} G_ff (which gives -1/λ), but K^{-1} is expensive.
    //
    //    Practical approach for the dense/coarse scale: form A = K_ff + σ G_ff
    //    for a shift σ and use the Jacobi solver on the combined matrix.
    //    Actually, the simplest approach that works with our Jacobi solver:
    //    solve the generalized problem K x = λ M x where M = -G (since we
    //    want K + λG = 0, i.e. K = -λG, i.e. K x = λ(-G) x).
    //
    //    We use the Cholesky-like transform: L^{-1} K L^{-T} y = λ L^{-1}(-G) L^{-T} y
    //    where L L^T = K. But Cholesky is not implemented. Instead, we use
    //    the direct approach: form the combined matrix and use Jacobi.
    //
    //    For v1, we solve the standard eigenvalue problem on the matrix
    //    K_ff^{-1} · (-G_ff) by first computing K_ff^{-1} via the dense solve
    //    utility, then forming the product and running Jacobi on it.
    //
    //    Actually, the simplest correct approach: compute the matrix
    //    A = K_ff^{-1} G_ff, then its eigenvalues are -1/λ.
    //    We compute K_ff^{-1} by solving K_ff X = G_ff column by column.

    let mut a = vec![0.0; nf * nf];
    for col in 0..nf {
        // Extract column `col` of G_ff.
        let g_col: Vec<f64> = (0..nf).map(|i| gff[i * nf + col]).collect();
        // Solve K_ff x = g_col.
        let x = crate::solve::solve_dense(&kff, &g_col, nf);
        for i in 0..nf {
            a[i * nf + col] = x[i];
        }
    }
    // Now A = K^{-1} G. Eigenvalues of A are -1/λ, so λ = -1/eigenvalue.
    let (eigvals, eigvecs) = jacobi_eigen(&a, nf);

    // Convert eigenvalues to buckling load factors and sort ascending.
    let mut pairs: Vec<(f64, Vec<f64>)> = eigvals
        .iter()
        .zip(eigvecs.chunks(nf))
        .filter_map(|(&ev, vec)| {
            if ev.abs() > 1e-12 {
                let lambda = -1.0 / ev;
                if lambda > 0.0 {
                    Some((lambda, vec.to_vec()))
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect();
    pairs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    pairs.truncate(n_modes);

    BucklingResult {
        load_factors: pairs.iter().map(|(l, _)| *l).collect(),
        mode_shapes: pairs.iter().map(|(_, v)| v.clone()).collect(),
        free_dofs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_vertex_kernel::geometry::solid::{Face, Solid};
    use tpt_vertex_kernel::math::Vec3;

    fn column(height: f64) -> Solid {
        let mut s = Solid::new();
        let mut v = |x, y, z| s.add_vertex(Vec3::new(x, y, z));
        let p = [
            v(-0.5, -0.5, 0.0),
            v(0.5, -0.5, 0.0),
            v(0.5, 0.5, 0.0),
            v(-0.5, 0.5, 0.0),
            v(-0.5, -0.5, height),
            v(0.5, -0.5, height),
            v(0.5, 0.5, height),
            v(-0.5, 0.5, height),
        ];
        let mut f = |a, b, c| s.faces.push(Face::new(a, b, c));
        f(p[0], p[1], p[2]);
        f(p[0], p[2], p[3]);
        f(p[4], p[6], p[5]);
        f(p[4], p[7], p[6]);
        f(p[0], p[5], p[1]);
        f(p[0], p[4], p[5]);
        f(p[1], p[6], p[2]);
        f(p[1], p[5], p[6]);
        f(p[2], p[7], p[3]);
        f(p[2], p[6], p[7]);
        f(p[3], p[4], p[0]);
        f(p[3], p[7], p[4]);
        s
    }

    #[test]
    fn column_buckling_load_factor_is_positive() {
        let solid = column(10.0);
        let m = crate::mesh::tetrahedralize(&solid, 2.0).unwrap();
        let fixed: Vec<usize> = m
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n[2] < 1e-6)
            .map(|(i, _)| i)
            .collect();
        let bc = BoundaryCondition::new()
            .fix_all(&fixed)
            .with_load(crate::bc::PointLoad {
                node: m
                    .nodes
                    .iter()
                    .enumerate()
                    .max_by(|a, b| a.1[2].partial_cmp(&b.1[2]).unwrap())
                    .unwrap()
                    .0,
                fx: 0.0,
                fy: 0.0,
                fz: -100.0,
            });
        let res = buckling_analysis(&m, 200_000.0, 0.3, &bc, 3);
        assert!(
            !res.load_factors.is_empty(),
            "should have at least one buckling mode"
        );
        assert!(
            res.load_factors[0] > 0.0,
            "first load factor should be positive: {}",
            res.load_factors[0]
        );
    }

    #[test]
    #[allow(clippy::needless_range_loop)] // symmetric 12x12 matrix indexing is clearest with range loops
    fn geometric_stiffness_is_symmetric() {
        let nodes = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
        ];
        let sigma = [100.0, 50.0, 0.0, 10.0, 0.0, 0.0];
        let kg = geometric_stiffness(&nodes, sigma);
        for i in 0..12 {
            for j in 0..12 {
                assert!((kg[i][j] - kg[j][i]).abs() < 1e-10, "asym at {i},{j}");
            }
        }
    }
}
