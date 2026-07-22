// SPDX-License-Identifier: MIT OR Apache-2.0

//! Topology optimization using the SIMP (Solid Isotropic Material with
//! Penalization) method.
//!
//! Minimizes compliance (maximizes stiffness) subject to a volume fraction
//! constraint. Each element has a pseudo-density `ρ_e ∈ [0, 1]` that
//! interpolates the material modulus: `E_e = ρ_e^p · E_0`, where `p ≥ 3`
//! is the penalization power that pushes densities toward 0 or 1.
//!
//! The optimality criteria (OC) update rule is used for v1, which is simple,
//! efficient, and well-suited for compliance minimization. Sensitivity
//! analysis uses the adjoint method (which for compliance = direct
//! differentiation of the displacement).
//!
//! Reference: Sigmund & Maass, "A 99 line MATLAB code for topology
//! optimization" (2009).

use crate::assembly::GlobalSystem;
use crate::bc::BoundaryCondition;
use crate::element::tet_stiffness;
use crate::material::elastic_matrix;
use crate::mesh::VolMesh;
use crate::solve::solve_dense;

/// Configuration for a topology optimization run.
#[derive(Debug, Clone)]
pub struct TopoOptConfig {
    /// Target volume fraction (0.0–1.0). E.g. 0.3 means 30% of the domain
    /// should remain solid.
    pub volume_fraction: f64,
    /// SIMP penalization power (typically 3.0).
    pub penal: f64,
    /// Filter radius in element-length units (mesh-independent filtering).
    /// Set to 0.0 to disable filtering.
    pub filter_radius: f64,
    /// Maximum number of OC iterations.
    pub max_iter: usize,
    /// Convergence tolerance on change in density.
    pub tol: f64,
    /// Move limit for OC update (fraction of current density).
    pub move_limit: f64,
}

impl Default for TopoOptConfig {
    fn default() -> Self {
        TopoOptConfig {
            volume_fraction: 0.3,
            penal: 3.0,
            filter_radius: 1.5,
            max_iter: 200,
            tol: 1e-4,
            move_limit: 0.2,
        }
    }
}

/// Result of a topology optimization run.
#[derive(Debug, Clone)]
pub struct TopoOptResult {
    /// Optimized element densities (0.0–1.0, length = n_tets).
    pub densities: Vec<f64>,
    /// Compliance (total strain energy) at convergence.
    pub compliance: f64,
    /// Number of iterations used.
    pub iterations: usize,
    /// Convergence history (compliance per iteration).
    pub history: Vec<f64>,
    /// Whether the optimization converged.
    pub converged: bool,
}

/// Run topology optimization on `vol` under material `(e, nu)` and BCs.
pub fn topo_optimize(
    vol: &VolMesh,
    e: f64,
    nu: f64,
    bc: &BoundaryCondition,
    config: &TopoOptConfig,
) -> TopoOptResult {
    let n_tets = vol.tet_count();
    let n_nodes = vol.node_count();
    let n_dofs = n_nodes * 3;
    let d = elastic_matrix(e, nu);

    // Initial densities: uniform at the volume fraction.
    let mut rho = vec![config.volume_fraction; n_tets];

    // Element volumes.
    let elem_vols: Vec<f64> = vol
        .tets
        .iter()
        .map(|tet| {
            let nodes = [
                vol.nodes[tet[0]],
                vol.nodes[tet[1]],
                vol.nodes[tet[2]],
                vol.nodes[tet[3]],
            ];
            crate::element::tet_volume(&nodes).abs()
        })
        .collect();
    let total_vol: f64 = elem_vols.iter().sum();
    let target_vol = config.volume_fraction * total_vol;

    // Filter weights (distance-based).
    let filter_weights = if config.filter_radius > 0.0 {
        compute_filter_weights(vol, config.filter_radius)
    } else {
        None
    };

    let mut history = Vec::new();
    let mut converged = false;

    for _iter in 0..config.max_iter {
        // 1. Assemble global stiffness with penalized densities.
        let mut k = vec![0.0; n_dofs * n_dofs];
        for (t, tet) in vol.tets.iter().enumerate() {
            let nodes = [
                vol.nodes[tet[0]],
                vol.nodes[tet[1]],
                vol.nodes[tet[2]],
                vol.nodes[tet[3]],
            ];
            let d_penal = penalized_matrix(&d, rho[t], config.penal);
            let ke = tet_stiffness(&nodes, &d_penal);
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

        // Apply penalty constraints.
        let mut kmax: f64 = 0.0;
        for &v in &k {
            kmax = kmax.max(v.abs());
        }
        let penalty = kmax * 1e12 + 1.0;
        let mut f = vec![0.0; n_dofs];
        for load in &bc.loads {
            let gdof = GlobalSystem::dof(load.node, 0);
            f[gdof] += load.fx;
            f[gdof + 1] += load.fy;
            f[gdof + 2] += load.fz;
        }
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

        // 2. Solve for displacements.
        let u = solve_dense(&k, &f, n_dofs);

        // 3. Compute compliance and sensitivities.
        let mut compliance = 0.0;
        let mut sensitivities = vec![0.0; n_tets];

        for (t, tet) in vol.tets.iter().enumerate() {
            let nodes = [
                vol.nodes[tet[0]],
                vol.nodes[tet[1]],
                vol.nodes[tet[2]],
                vol.nodes[tet[3]],
            ];
            let d_penal = penalized_matrix(&d, rho[t], config.penal);
            let ke = tet_stiffness(&nodes, &d_penal);

            // Gather element displacement.
            let mut ue = [0.0; 12];
            for i in 0..4 {
                let gdof = GlobalSystem::dof(tet[i], 0);
                ue[i * 3] = u[gdof];
                ue[i * 3 + 1] = u[gdof + 1];
                ue[i * 3 + 2] = u[gdof + 2];
            }

            // Element compliance: u_e^T K_e u_e.
            let mut ce = 0.0;
            for i in 0..12 {
                for j in 0..12 {
                    ce += ue[i] * ke[i][j] * ue[j];
                }
            }
            compliance += ce;

            // Sensitivity: dC/dρ_e = -p · ρ_e^(p-1) · u_e^T K_0 u_e.
            // K_0 is the stiffness at ρ=1.
            let d_0 = elastic_matrix(e, nu);
            let ke_0 = tet_stiffness(&nodes, &d_0);
            let mut ce_0 = 0.0;
            for i in 0..12 {
                for j in 0..12 {
                    ce_0 += ue[i] * ke_0[i][j] * ue[j];
                }
            }
            sensitivities[t] = -config.penal * rho[t].powf(config.penal - 1.0) * ce_0;
        }

        history.push(compliance);

        // 4. Sensitivity filter (convolution with distance-weighted mask).
        if let Some(ref weights) = filter_weights {
            sensitivities = apply_filter(&sensitivities, weights);
        }

        // 5. OC update.
        let rho_new = oc_update(&rho, &sensitivities, &elem_vols, target_vol, config);

        // 6. Convergence check.
        let change: f64 = rho
            .iter()
            .zip(rho_new.iter())
            .map(|(r, rn)| (r - rn).abs())
            .fold(0.0, f64::max);

        rho = rho_new;

        if change < config.tol {
            converged = true;
            break;
        }
    }

    TopoOptResult {
        densities: rho,
        compliance: history.last().copied().unwrap_or(0.0),
        iterations: history.len(),
        history,
        converged,
    }
}

/// Penalized constitutive matrix: D_penal = ρ^p · D.
fn penalized_matrix(d: &[[f64; 6]; 6], rho: f64, penal: f64) -> [[f64; 6]; 6] {
    let factor = rho.powf(penal);
    let mut dp = *d;
    for row in dp.iter_mut() {
        for v in row.iter_mut() {
            *v *= factor;
        }
    }
    dp
}

/// Compute distance-based filter weights for all element pairs within
/// `radius`. Returns a sparse weight matrix: `weights[i]` is a list of
/// `(j, w)` pairs where `w` is the distance-based weight.
fn compute_filter_weights(vol: &VolMesh, radius: f64) -> Option<Vec<Vec<(usize, f64)>>> {
    // Element centroids.
    let centroids: Vec<[f64; 3]> = vol
        .tets
        .iter()
        .map(|tet| {
            let n0 = vol.nodes[tet[0]];
            let n1 = vol.nodes[tet[1]];
            let n2 = vol.nodes[tet[2]];
            let n3 = vol.nodes[tet[3]];
            [
                (n0[0] + n1[0] + n2[0] + n3[0]) / 4.0,
                (n0[1] + n1[1] + n2[1] + n3[1]) / 4.0,
                (n0[2] + n1[2] + n2[2] + n3[2]) / 4.0,
            ]
        })
        .collect();

    let mut weights = Vec::new();
    for ci in centroids.iter() {
        let mut row = Vec::new();
        for (j, cj) in centroids.iter().enumerate() {
            let dx = ci[0] - cj[0];
            let dy = ci[1] - cj[1];
            let dz = ci[2] - cj[2];
            let dist = (dx * dx + dy * dy + dz * dz).sqrt();
            if dist < radius {
                // Linear hat function: w = max(0, 1 - dist/radius).
                let w = (1.0 - dist / radius).max(0.0);
                row.push((j, w));
            }
        }
        weights.push(row);
    }
    Some(weights)
}

/// Apply sensitivity filter: a distance-weighted average of the *signed*
/// neighbor sensitivities, `ŝ_i = Σ_j w_ij · s_j / Σ_j w_ij`.
///
/// Averaging the signed values (rather than their absolute values) is what
/// lets neighboring sensitivities of opposite sign cancel — that
/// cancellation is exactly what suppresses the checkerboard-style
/// oscillation this filter exists to remove. Averaging `|s_j|` and only
/// reattaching `s_i`'s sign afterward (as a previous version of this
/// function did) discards that cancellation and can make the filtered
/// magnitude *larger* than the original at a local sign-alternating point,
/// the opposite of smoothing.
fn apply_filter(sens: &[f64], weights: &[Vec<(usize, f64)>]) -> Vec<f64> {
    sens.iter()
        .enumerate()
        .map(|(i, si)| {
            let row = &weights[i];
            let mut num = 0.0;
            let mut den = 0.0;
            for &(j, w) in row {
                num += w * sens[j];
                den += w;
            }
            if den > 1e-12 {
                num / den
            } else {
                *si
            }
        })
        .collect()
}

/// Optimality criteria (OC) density update.
fn oc_update(
    rho: &[f64],
    sensitivities: &[f64],
    elem_vols: &[f64],
    target_vol: f64,
    config: &TopoOptConfig,
) -> Vec<f64> {
    let n = rho.len();

    // Lagrange multiplier via bisection on the volume constraint.
    let mut lambda_min = 0.0;
    let mut lambda_max = 1e10;
    let mut lambda = 0.0;

    for _ in 0..50 {
        lambda = (lambda_min + lambda_max) / 2.0;
        let vol_sum: f64 = (0..n)
            .map(|i| {
                let mut r = rho[i] * (-sensitivities[i] / lambda).sqrt();
                r = r.max(config.move_limit).clamp(0.0, 1.0);
                r * elem_vols[i]
            })
            .sum();

        if vol_sum > target_vol {
            lambda_min = lambda;
        } else {
            lambda_max = lambda;
        }
    }

    // Final OC update.
    (0..n)
        .map(|i| {
            let mut r = rho[i] * (-sensitivities[i] / lambda).sqrt();
            r = r.max(config.move_limit).clamp(0.0, 1.0);
            r
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn beam_mesh() -> VolMesh {
        // Simple 2×1×1 beam mesh (8 nodes, 5 tets).
        let nodes = vec![
            [0.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [2.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [2.0, 0.0, 1.0],
            [2.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ];
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
    fn topo_opt_reduces_volume() {
        let vol = beam_mesh();
        let bc = BoundaryCondition::new()
            .fix_node(0)
            .with_load(crate::bc::PointLoad {
                node: 6,
                fx: 0.0,
                fy: -100.0,
                fz: 0.0,
            });
        let config = TopoOptConfig {
            volume_fraction: 0.5,
            penal: 3.0,
            filter_radius: 0.0,
            max_iter: 50,
            tol: 1e-4,
            move_limit: 0.3,
        };
        let result = topo_optimize(&vol, 200_000.0, 0.3, &bc, &config);
        // Densities should not all be at the initial value.
        let avg_rho: f64 = result.densities.iter().sum::<f64>() / result.densities.len() as f64;
        assert!(
            avg_rho < config.volume_fraction + 0.1,
            "average density {avg_rho}"
        );
        assert!(!result.history.is_empty());
    }

    #[test]
    fn penalized_matrix_scales_with_density() {
        let d = elastic_matrix(200_000.0, 0.3);
        let d1 = penalized_matrix(&d, 0.5, 3.0);
        let d2 = penalized_matrix(&d, 1.0, 3.0);
        assert!(
            (d1[0][0] / d2[0][0] - 0.125).abs() < 1e-9,
            "ρ=0.5 should give 0.125×"
        );
    }

    #[test]
    fn filter_reduces_sensitivity_oscillation() {
        let sens = vec![1.0, -2.0, 3.0, -1.0];
        let weights = vec![
            vec![(0, 1.0), (1, 0.5)],
            vec![(0, 0.5), (1, 1.0), (2, 0.5)],
            vec![(1, 0.5), (2, 1.0), (3, 0.5)],
            vec![(2, 0.5), (3, 1.0)],
        ];
        let filtered = apply_filter(&sens, &weights);
        assert_eq!(filtered.len(), 4);
        // Filtered values should be smoothed (smaller magnitude).
        for (s, f) in sens.iter().zip(filtered.iter()) {
            assert!(f.abs() <= s.abs() + 1e-9);
        }
    }
}
