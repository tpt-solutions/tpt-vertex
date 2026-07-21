//! Modal / frequency analysis: consistent-with-static-FEA stiffness reuse,
//! a lumped mass matrix, and a self-contained symmetric eigensolver.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! Free (unforced) vibration satisfies the generalized eigenproblem
//! `K x = ω² M x`. Fixed nodes are removed from the system entirely (rather
//! than the static-analysis penalty method) so no spurious high-frequency
//! "fixed-DOF" modes appear. `M` is diagonal (lumped: each tet's mass is
//! split evenly across its 4 nodes), so the generalized problem reduces to a
//! standard one via `K' = M^{-1/2} K M^{-1/2}`, solved with a classic cyclic
//! Jacobi eigenvalue algorithm (dependency-free, adequate for the crate's
//! dense/coarse-mesh scale — see `solve.rs` for the same tradeoff on the
//! static solve).

use crate::assembly::GlobalSystem;
use crate::element::{tet_stiffness, tet_volume};
use crate::material::elastic_matrix;
use crate::mesh::VolMesh;

/// Result of a modal analysis: ascending natural frequencies (Hz) and their
/// mode shapes (displacement vectors over the *free* DOFs only, matching
/// `free_dofs`).
#[derive(Debug, Clone)]
pub struct ModalResult {
    pub frequencies_hz: Vec<f64>,
    pub mode_shapes: Vec<Vec<f64>>,
    /// Global DOF index (`node*3 + axis`) each entry of a mode shape refers to.
    pub free_dofs: Vec<usize>,
}

/// Run modal analysis on `vol` for isotropic material `(e, nu, density)`,
/// with `fixed_nodes` removed from the system (rigid supports).
pub fn modal_analysis(vol: &VolMesh, e: f64, nu: f64, density: f64, fixed_nodes: &[usize]) -> ModalResult {
    let n_nodes = vol.node_count();
    let n_dofs = n_nodes * 3;
    let d = elastic_matrix(e, nu);

    let mut k = vec![0.0; n_dofs * n_dofs];
    let mut mass_diag = vec![0.0; n_dofs];

    for tet in &vol.tets {
        let nodes = [vol.nodes[tet[0]], vol.nodes[tet[1]], vol.nodes[tet[2]], vol.nodes[tet[3]]];
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

        let mass_e = density * tet_volume(&nodes).abs();
        let nodal_mass = mass_e / 4.0;
        for &n in tet {
            for a in 0..3 {
                mass_diag[GlobalSystem::dof(n, a)] += nodal_mass;
            }
        }
    }

    let mut free_dofs = Vec::new();
    for n in 0..n_nodes {
        if !fixed_nodes.contains(&n) {
            for a in 0..3 {
                free_dofs.push(GlobalSystem::dof(n, a));
            }
        }
    }
    let nf = free_dofs.len();

    let mut kff = vec![0.0; nf * nf];
    for i in 0..nf {
        for j in 0..nf {
            kff[i * nf + j] = k[free_dofs[i] * n_dofs + free_dofs[j]];
        }
    }
    let mff: Vec<f64> = free_dofs.iter().map(|&dof| mass_diag[dof]).collect();
    let minv_sqrt: Vec<f64> = mff.iter().map(|&m| if m > 1e-15 { 1.0 / m.sqrt() } else { 0.0 }).collect();

    let mut kp = vec![0.0; nf * nf];
    for i in 0..nf {
        for j in 0..nf {
            kp[i * nf + j] = minv_sqrt[i] * kff[i * nf + j] * minv_sqrt[j];
        }
    }

    let (eigvals, eigvecs) = jacobi_eigen(&kp, nf);

    let mut frequencies_hz = Vec::with_capacity(nf);
    let mut mode_shapes = Vec::with_capacity(nf);
    for m in 0..nf {
        let lambda = eigvals[m].max(0.0);
        // Units: mm/N/MPa/g throughout, so K is N/mm and M is g. Converting to
        // SI (N/m, kg): K_SI = K*1e3, M_SI = M*1e-3, so
        // ω² = K_SI/M_SI = (K/M)·1e6, i.e. ω = 1000·sqrt(λ).
        let omega = 1000.0 * lambda.sqrt();
        frequencies_hz.push(omega / (2.0 * std::f64::consts::PI));
        let shape: Vec<f64> = (0..nf).map(|i| minv_sqrt[i] * eigvecs[i * nf + m]).collect();
        mode_shapes.push(shape);
    }

    ModalResult { frequencies_hz, mode_shapes, free_dofs }
}

/// Classic cyclic Jacobi eigenvalue algorithm for a dense symmetric `n×n`
/// matrix. Returns `(eigenvalues, eigenvectors)` sorted ascending, with
/// eigenvectors stored column-major (`eigenvectors[row*n + col]`).
pub fn jacobi_eigen(a_in: &[f64], n: usize) -> (Vec<f64>, Vec<f64>) {
    let mut a = a_in.to_vec();
    let mut v = vec![0.0; n * n];
    for i in 0..n {
        v[i * n + i] = 1.0;
    }

    for _sweep in 0..100 {
        let mut off = 0.0;
        for i in 0..n {
            for j in 0..n {
                if i != j {
                    off += a[i * n + j] * a[i * n + j];
                }
            }
        }
        if off.sqrt() < 1e-10 {
            break;
        }
        for p in 0..n {
            for q in (p + 1)..n {
                let apq = a[p * n + q];
                if apq.abs() < 1e-14 {
                    continue;
                }
                let theta = (a[q * n + q] - a[p * n + p]) / (2.0 * apq);
                let t = theta.signum() / (theta.abs() + (1.0 + theta * theta).sqrt());
                let c = 1.0 / (1.0 + t * t).sqrt();
                let s = t * c;
                let app = a[p * n + p];
                let aqq = a[q * n + q];
                a[p * n + p] = app - t * apq;
                a[q * n + q] = aqq + t * apq;
                a[p * n + q] = 0.0;
                a[q * n + p] = 0.0;
                for i in 0..n {
                    if i != p && i != q {
                        let aip = a[i * n + p];
                        let aiq = a[i * n + q];
                        a[i * n + p] = c * aip - s * aiq;
                        a[p * n + i] = a[i * n + p];
                        a[i * n + q] = s * aip + c * aiq;
                        a[q * n + i] = a[i * n + q];
                    }
                }
                for i in 0..n {
                    let vip = v[i * n + p];
                    let viq = v[i * n + q];
                    v[i * n + p] = c * vip - s * viq;
                    v[i * n + q] = s * vip + c * viq;
                }
            }
        }
    }

    let eigvals: Vec<f64> = (0..n).map(|i| a[i * n + i]).collect();
    let mut idx: Vec<usize> = (0..n).collect();
    idx.sort_by(|&i, &j| eigvals[i].partial_cmp(&eigvals[j]).unwrap());
    let sorted_vals: Vec<f64> = idx.iter().map(|&i| eigvals[i]).collect();
    let mut sorted_vecs = vec![0.0; n * n];
    for (new_col, &old_col) in idx.iter().enumerate() {
        for row in 0..n {
            sorted_vecs[row * n + new_col] = v[row * n + old_col];
        }
    }
    (sorted_vals, sorted_vecs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_vertex_kernel::geometry::solid::{Face, Solid};
    use tpt_vertex_kernel::math::Vec3;

    #[test]
    fn jacobi_recovers_known_eigenvalues_of_diagonal_matrix() {
        let a = vec![
            5.0, 0.0, 0.0,
            0.0, 1.0, 0.0,
            0.0, 0.0, 3.0,
        ];
        let (vals, _vecs) = jacobi_eigen(&a, 3);
        assert!((vals[0] - 1.0).abs() < 1e-9);
        assert!((vals[1] - 3.0).abs() < 1e-9);
        assert!((vals[2] - 5.0).abs() < 1e-9);
    }

    #[test]
    fn jacobi_recovers_known_eigenvalues_of_dense_symmetric_matrix() {
        // 2x2 with known eigenvalues 1 and 3 (trace=4, det=3): [[2,1],[1,2]].
        let a = vec![2.0, 1.0, 1.0, 2.0];
        let (vals, vecs) = jacobi_eigen(&a, 2);
        assert!((vals[0] - 1.0).abs() < 1e-9, "{:?}", vals);
        assert!((vals[1] - 3.0).abs() < 1e-9, "{:?}", vals);
        // Eigenvectors should be orthonormal.
        let v0 = [vecs[0], vecs[2]];
        let v1 = [vecs[1], vecs[3]];
        let dot = v0[0] * v1[0] + v0[1] * v1[1];
        assert!(dot.abs() < 1e-9, "eigenvectors not orthogonal: {dot}");
    }

    fn beam(l: f64) -> Solid {
        let mut s = Solid::new();
        let mut v = |x, y, z| s.add_vertex(Vec3::new(x, y, z));
        let p = [
            v(0.0, 0.0, 0.0), v(l, 0.0, 0.0), v(l, 1.0, 0.0), v(0.0, 1.0, 0.0),
            v(0.0, 0.0, 1.0), v(l, 0.0, 1.0), v(l, 1.0, 1.0), v(0.0, 1.0, 1.0),
        ];
        let mut f = |a, b, c| s.faces.push(Face::new(a, b, c));
        f(p[0], p[1], p[2]); f(p[0], p[2], p[3]);
        f(p[4], p[6], p[5]); f(p[4], p[7], p[6]);
        f(p[0], p[5], p[1]); f(p[0], p[4], p[5]);
        f(p[1], p[6], p[2]); f(p[1], p[5], p[6]);
        f(p[2], p[7], p[3]); f(p[2], p[6], p[7]);
        f(p[3], p[4], p[0]); f(p[3], p[7], p[4]);
        s
    }

    #[test]
    fn cantilever_first_mode_is_positive_and_finite() {
        let solid = beam(4.0);
        let m = crate::mesh::tetrahedralize(&solid, 1.0).unwrap();
        let fixed: Vec<usize> = m.nodes.iter().enumerate().filter(|(_, n)| n[0] < 1e-6).map(|(i, _)| i).collect();
        let res = modal_analysis(&m, 200_000.0, 0.3, 7.85e-3, &fixed);
        assert!(!res.frequencies_hz.is_empty());
        let f1 = res.frequencies_hz[0];
        assert!(f1.is_finite() && f1 > 0.0, "first frequency {f1}");
    }

    #[test]
    fn stiffer_material_has_higher_natural_frequency() {
        let solid = beam(4.0);
        let m = crate::mesh::tetrahedralize(&solid, 1.0).unwrap();
        let fixed: Vec<usize> = m.nodes.iter().enumerate().filter(|(_, n)| n[0] < 1e-6).map(|(i, _)| i).collect();
        // Same density, stiffer modulus (steel vs. a soft plastic) should raise f1.
        let soft = modal_analysis(&m, 2_000.0, 0.35, 1.2e-3, &fixed);
        let stiff = modal_analysis(&m, 200_000.0, 0.3, 7.85e-3, &fixed);
        assert!(stiff.frequencies_hz[0] > soft.frequencies_hz[0]);
    }
}
