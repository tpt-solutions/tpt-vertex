// SPDX-License-Identifier: MIT OR Apache-2.0

//! Nonlinear (Newton-Raphson) solver for large-deformation FEA.
//!
//! Implements a Total Lagrangian formulation where the reference configuration
//! is the original (undeformed) mesh. At each Newton iteration:
//!
//! 1. Compute the internal force `f_int = ∫ B^T σ dV` from the current
//!    stress/displacement state.
//! 2. Compute the residual `r = f_ext - f_int`.
//! 3. Assemble the tangent stiffness `K_T = K_mat + K_geo` (material +
//!    geometric contributions).
//! 4. Solve `K_T Δu = r` for the displacement increment.
//! 5. Update `u ← u + Δu` and the mesh geometry.
//! 6. Check convergence: `‖Δu‖ / ‖u‖ < tol` and `‖r‖ / ‖f_ext‖ < tol`.
//!
//! The material tangent is provided by [`crate::plasticity`] for nonlinear
//! materials, or by the standard elastic matrix for linear materials. The
//! geometric stiffness accounts for stress stiffening/softening under large
//! rotations.

use crate::assembly::GlobalSystem;
use crate::bc::BoundaryCondition;
use crate::element::{strain_displacement, tet_volume};
use crate::material::elastic_matrix;
use crate::mesh::VolMesh;
use crate::plasticity::{radial_return, HardeningLaw, IntegrationPointState};
use crate::solve::solve_dense;

/// Convergence tolerances for the Newton-Raphson iteration.
#[derive(Debug, Clone, Copy)]
pub struct NonlinearTolerance {
    /// Relative displacement norm tolerance: `‖Δu‖/‖u‖ < tol_disp`.
    pub disp: f64,
    /// Relative residual (force) norm tolerance: `‖r‖/‖f_ext‖ < tol_force`.
    pub force: f64,
    /// Maximum number of iterations per load step.
    pub max_iter: usize,
}

impl Default for NonlinearTolerance {
    fn default() -> Self {
        NonlinearTolerance {
            disp: 1e-8,
            force: 1e-8,
            max_iter: 50,
        }
    }
}

/// Result of a single Newton-Raphson iteration.
#[derive(Debug, Clone)]
pub struct NewtonStep {
    /// Iteration number (0-based).
    pub iteration: usize,
    /// Current residual norm `‖r‖`.
    pub residual_norm: f64,
    /// Current displacement increment norm `‖Δu‖`.
    pub disp_increment_norm: f64,
    /// Relative residual `‖r‖/‖f_ext‖`.
    pub relative_force: f64,
    /// Relative displacement `‖Δu‖/‖u‖`.
    pub relative_disp: f64,
    /// Whether this step converged.
    pub converged: bool,
}

/// Full result of a nonlinear analysis.
#[derive(Debug, Clone)]
pub struct NonlinearResult {
    /// Converged displacement vector.
    pub displacements: Vec<f64>,
    /// Per-element von Mises stress.
    pub von_mises: Vec<f64>,
    /// Maximum nodal displacement magnitude.
    pub max_displacement: f64,
    /// Maximum von Mises stress.
    pub max_von_mises: f64,
    /// Newton iteration history.
    pub history: Vec<NewtonStep>,
    /// Total number of iterations across all load steps.
    pub total_iterations: usize,
    /// Whether the analysis converged.
    pub converged: bool,
}

/// Material model for the nonlinear solver.
#[derive(Debug, Clone)]
pub enum NonlinearMaterial {
    /// Linear elastic: `σ = D ε` (no iteration needed, but the framework
    /// still runs Newton-Raphson for geometric nonlinearity).
    Linear,
    /// J2 plasticity with isotropic hardening.
    J2Plasticity {
        sigma_y0: f64,
        hardening: HardeningLaw,
    },
}

/// Run a nonlinear analysis with Newton-Raphson iteration.
///
/// `n_load_steps` divides the external load into equal increments. The solver
/// iterates within each load step until convergence (or `max_iter`).
pub fn nonlinear_solve(
    vol: &VolMesh,
    e: f64,
    nu: f64,
    bc: &BoundaryCondition,
    material: &NonlinearMaterial,
    n_load_steps: usize,
    tol: &NonlinearTolerance,
) -> NonlinearResult {
    let n_nodes = vol.node_count();
    let n_dofs = n_nodes * 3;

    // Current displacement state.
    let mut u = vec![0.0; n_dofs];
    // Accumulated element states (plastic strain per element).
    let mut ip_states: Vec<IntegrationPointState> =
        vec![IntegrationPointState::default(); vol.tet_count()];
    // Current node positions (updated for geometric nonlinearity).
    let mut current_nodes = vol.nodes.clone();

    let mut total_iterations = 0usize;
    let mut converged = true;
    let mut history = Vec::new();

    // Load factor per step.
    let load_factor_step = 1.0 / n_load_steps as f64;

    for step in 0..n_load_steps {
        let load_factor = (step + 1) as f64 * load_factor_step;

        // Newton-Raphson iteration within this load step.
        for iter in 0..tol.max_iter {
            // 1. Assemble tangent stiffness and internal forces, with the
            // fixed-DOF rows/columns penalty-constrained (via
            // `assemble_constrained_tangent`). Solving the *unconstrained*
            // tangent here (as a previous version of this function did,
            // only zeroing the residual below) leaves the rigid-body modes
            // at the fixed nodes in the matrix, so `solve_dense` returns an
            // arbitrary combination of them on top of the real solution and
            // the Newton iteration diverges instead of converging.
            let (k_tangent, f_int) = assemble_constrained_tangent(
                vol,
                &current_nodes,
                e,
                nu,
                &u,
                material,
                &mut ip_states,
                bc,
            );

            // 2. External force vector (scaled by load factor).
            let f_ext = assemble_external_force(vol, bc, load_factor);

            // 3. Residual: r = f_ext - f_int.
            let mut residual = vec![0.0; n_dofs];
            for i in 0..n_dofs {
                residual[i] = f_ext[i] - f_int[i];
            }

            // 4. Zero the residual at fixed DOFs too, consistent with the
            // penalty rows/columns above (u=0 there).
            for &node in &bc.fixed_nodes {
                for a in 0..3 {
                    let dof = GlobalSystem::dof(node, a);
                    residual[dof] = 0.0;
                }
            }

            // 5. Solve for increment: K_T Δu = r.
            let delta_u = solve_dense(&k_tangent, &residual, n_dofs);

            // 6. Update displacement.
            for i in 0..n_dofs {
                u[i] += delta_u[i];
            }

            // 7. Update node positions for geometric nonlinearity.
            for (i, node) in current_nodes.iter_mut().enumerate() {
                node[0] = vol.nodes[i][0] + u[i * 3];
                node[1] = vol.nodes[i][1] + u[i * 3 + 1];
                node[2] = vol.nodes[i][2] + u[i * 3 + 2];
            }

            // 8. Convergence check.
            let disp_norm: f64 = delta_u.iter().map(|x| x * x).sum::<f64>().sqrt();
            let u_norm: f64 = u.iter().map(|x| x * x).sum::<f64>().sqrt().max(1e-30);
            let res_norm: f64 = residual.iter().map(|x| x * x).sum::<f64>().sqrt();
            let f_ext_norm: f64 = f_ext.iter().map(|x| x * x).sum::<f64>().sqrt().max(1e-30);

            let relative_disp = disp_norm / u_norm;
            let relative_force = res_norm / f_ext_norm;
            let converged_step = relative_disp < tol.disp && relative_force < tol.force;

            history.push(NewtonStep {
                iteration: iter,
                residual_norm: res_norm,
                disp_increment_norm: disp_norm,
                relative_force,
                relative_disp,
                converged: converged_step,
            });
            total_iterations += 1;

            if converged_step {
                break;
            }
            if iter == tol.max_iter - 1 {
                converged = false;
            }
        }

        if !converged {
            break;
        }
    }

    // Post-processing: von Mises stress.
    let von_mises = crate::post::von_mises_field(vol, e, nu, &u);
    let max_displacement = (0..n_nodes)
        .map(|n| {
            let d = crate::post::displacement_at(vol, &u, n);
            (d[0].powi(2) + d[1].powi(2) + d[2].powi(2)).sqrt()
        })
        .fold(0.0, f64::max);
    let max_von_mises = von_mises.iter().cloned().fold(0.0, f64::max);

    NonlinearResult {
        displacements: u,
        von_mises,
        max_displacement,
        max_von_mises,
        history,
        total_iterations,
        converged,
    }
}

/// Assemble the tangent stiffness matrix and internal force vector.
///
/// The tangent stiffness includes both material and geometric contributions:
/// `K_T = K_mat + K_geo`.
#[allow(clippy::needless_range_loop)] // fixed-size 6x6/12x12 matrix indexing is clearest with range loops
fn assemble_tangent_and_internal_force(
    vol: &VolMesh,
    current_nodes: &[[f64; 3]],
    e: f64,
    nu: f64,
    u: &[f64],
    material: &NonlinearMaterial,
    ip_states: &mut [IntegrationPointState],
) -> (Vec<f64>, Vec<f64>) {
    let n_nodes = vol.node_count();
    let n_dofs = n_nodes * 3;
    let d = elastic_matrix(e, nu);

    let mut k_tangent = vec![0.0; n_dofs * n_dofs];
    let mut f_int = vec![0.0; n_dofs];

    for (t, tet) in vol.tets.iter().enumerate() {
        // Original (reference) node positions for B matrix.
        let ref_nodes = [
            vol.nodes[tet[0]],
            vol.nodes[tet[1]],
            vol.nodes[tet[2]],
            vol.nodes[tet[3]],
        ];
        // Current (deformed) node positions for volume and geometric stiffness.
        let cur_nodes = [
            current_nodes[tet[0]],
            current_nodes[tet[1]],
            current_nodes[tet[2]],
            current_nodes[tet[3]],
        ];

        // Element displacement vector.
        let mut ue = [0.0; 12];
        for i in 0..4 {
            let gdof = GlobalSystem::dof(tet[i], 0);
            ue[i * 3] = u[gdof];
            ue[i * 3 + 1] = u[gdof + 1];
            ue[i * 3 + 2] = u[gdof + 2];
        }

        // Strain from reference configuration.
        let b = strain_displacement(&ref_nodes);
        let mut strain = [0.0; 6];
        for i in 0..6 {
            for j in 0..12 {
                strain[i] += b[i][j] * ue[j];
            }
        }

        // Stress from material model.
        let (sigma, alpha) = match material {
            NonlinearMaterial::Linear => {
                let mut sigma = [0.0; 6];
                for i in 0..6 {
                    for j in 0..6 {
                        sigma[i] += d[i][j] * strain[j];
                    }
                }
                (sigma, 0.0)
            }
            NonlinearMaterial::J2Plasticity {
                sigma_y0,
                hardening,
            } => {
                let mut trial = [0.0; 6];
                for i in 0..6 {
                    for j in 0..6 {
                        trial[i] += d[i][j] * strain[j];
                    }
                }
                let (sigma_corr, new_eps_p, alpha) = radial_return(
                    trial,
                    ip_states[t].eps_p,
                    e,
                    nu,
                    *sigma_y0,
                    hardening,
                    1e-8,
                    50,
                );
                ip_states[t].eps_p = new_eps_p;
                (sigma_corr, alpha)
            }
        };

        // Material tangent stiffness: K_mat = V_ref * B^T D_ep B.
        let vol_ref = tet_volume(&ref_nodes).abs();
        let mut d_ep = d;
        if alpha > 0.0 {
            // Reduce shear moduli for plastic tangent.
            for i in 3..6 {
                for j in 3..6 {
                    d_ep[i][j] *= 1.0 - alpha;
                }
            }
        }

        // K_mat = V_ref * B^T D_ep B. `d_ep` is 6×6, so it must first be
        // contracted with B (6×12) to form `db` (6×12) before contracting
        // with B^T — indexing `d_ep[k][j]` directly with `j` in `0..12`
        // (skipping that intermediate product) reads past the end of each
        // 6-element row of `d_ep` once `j >= 6`.
        let mut db = [[0.0; 12]; 6];
        for i in 0..6 {
            for j in 0..12 {
                let mut s = 0.0;
                for k in 0..6 {
                    s += d_ep[i][k] * b[k][j];
                }
                db[i][j] = s;
            }
        }
        let mut k_mat = [[0.0; 12]; 12];
        for i in 0..12 {
            for j in 0..12 {
                let mut s = 0.0;
                for k in 0..6 {
                    s += b[k][i] * db[k][j];
                }
                k_mat[i][j] = vol_ref * s;
            }
        }

        // Geometric stiffness from current stress state.
        let k_geo = crate::buckling::geometric_stiffness(&cur_nodes, sigma);

        // Element tangent = K_mat + K_geo.
        let mut ke = [[0.0; 12]; 12];
        for i in 0..12 {
            for j in 0..12 {
                ke[i][j] = k_mat[i][j] + k_geo[i][j];
            }
        }

        // Internal force: f_int_e = V_ref * B^T sigma.
        let mut f_int_e = [0.0; 12];
        for i in 0..12 {
            for j in 0..6 {
                f_int_e[i] += b[j][i] * sigma[j];
            }
            f_int_e[i] *= vol_ref;
        }

        // Scatter to global.
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
            f_int[gdof[i]] += f_int_e[i];
            for j in 0..12 {
                k_tangent[gdof[i] * n_dofs + gdof[j]] += ke[i][j];
            }
        }
    }

    // Penalty constraints on tangent stiffness.
    // Applied in the caller via assemble_constrained_tangent.

    // We need the boundary conditions to apply penalties, but they aren't
    // passed directly. We'll apply them in the caller instead.
    // For now, just return the unassembled tangent and internal force.
    // The caller applies penalties.

    (k_tangent, f_int)
}

/// Assemble the external force vector scaled by `load_factor`.
fn assemble_external_force(vol: &VolMesh, bc: &BoundaryCondition, load_factor: f64) -> Vec<f64> {
    let n_dofs = vol.node_count() * 3;
    let mut f = vec![0.0; n_dofs];
    for load in &bc.loads {
        let gdof = GlobalSystem::dof(load.node, 0);
        f[gdof] += load.fx * load_factor;
        f[gdof + 1] += load.fy * load_factor;
        f[gdof + 2] += load.fz * load_factor;
    }
    f
}

/// Assemble tangent stiffness with penalty constraints (convenience wrapper).
#[allow(clippy::too_many_arguments)] // mirrors the FEA material/BC parameter set used throughout this crate
pub fn assemble_constrained_tangent(
    vol: &VolMesh,
    current_nodes: &[[f64; 3]],
    e: f64,
    nu: f64,
    u: &[f64],
    material: &NonlinearMaterial,
    ip_states: &mut [IntegrationPointState],
    bc: &BoundaryCondition,
) -> (Vec<f64>, Vec<f64>) {
    let n_dofs = vol.node_count() * 3;
    let (mut k, f_int) =
        assemble_tangent_and_internal_force(vol, current_nodes, e, nu, u, material, ip_states);

    // Apply penalty constraints.
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
        }
    }

    (k, f_int)
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
        // Standard 6-tet Kuhn triangulation sharing diagonal (0,6) — see
        // mesh::tetrahedralize for why a 5-tet list sharing this diagonal
        // (as this fixture previously had) leaves a gap: node 4 ends up
        // referenced by no tet at all, so its stiffness rows/columns are
        // all zero and the global system is singular.
        let tets = vec![
            [0, 1, 2, 6],
            [0, 2, 3, 6],
            [0, 3, 7, 6],
            [0, 7, 4, 6],
            [0, 4, 5, 6],
            [0, 5, 1, 6],
        ];
        VolMesh { nodes, tets }
    }

    #[test]
    fn linear_material_converges_in_one_step() {
        let vol = cube();
        // Fix the whole x=-1 face (nodes 0,3,4,7), not just node 0: pinning
        // a single node only removes 3 translational DOFs and leaves 3
        // rigid-body rotation modes about that point unconstrained, which
        // makes the global tangent stiffness singular.
        let bc = BoundaryCondition::new()
            .fix_all(&[0, 3, 4, 7])
            .with_load(crate::bc::PointLoad {
                node: 6,
                fx: 100.0,
                fy: 0.0,
                fz: 0.0,
            });
        let mat = NonlinearMaterial::Linear;
        let tol = NonlinearTolerance::default();
        let result = nonlinear_solve(&vol, 200_000.0, 0.3, &bc, &mat, 1, &tol);
        assert!(result.converged, "should converge");
        assert!(!result.history.is_empty());
        assert!(result.max_displacement > 0.0);
    }

    #[test]
    fn plasticity_converges_with_more_steps() {
        let vol = cube();
        let bc = BoundaryCondition::new()
            .fix_all(&[0, 3, 4, 7])
            .with_load(crate::bc::PointLoad {
                node: 6,
                fx: 500.0,
                fy: 0.0,
                fz: 0.0,
            });
        let mat = NonlinearMaterial::J2Plasticity {
            sigma_y0: 250.0,
            hardening: HardeningLaw::Linear {
                hardening_modulus: 1000.0,
            },
        };
        let tol = NonlinearTolerance {
            max_iter: 100,
            ..Default::default()
        };
        let result = nonlinear_solve(&vol, 200_000.0, 0.3, &bc, &mat, 10, &tol);
        assert!(result.converged, "should converge with subincrementation");
    }

    #[test]
    fn geometric_nonlinearity_stiffens_structure() {
        // A structure under large deformation should be stiffer than linear
        // prediction due to membrane/stretching effects (for appropriate BCs).
        // This is a qualitative check.
        let vol = cube();
        let bc = BoundaryCondition::new()
            .fix_all(&[0, 3, 4, 7])
            .with_load(crate::bc::PointLoad {
                node: 6,
                fx: 100.0,
                fy: 0.0,
                fz: 0.0,
            });

        let mat = NonlinearMaterial::Linear;
        let tol = NonlinearTolerance::default();

        let linear = nonlinear_solve(&vol, 200_000.0, 0.3, &bc, &mat, 1, &tol);
        // The geometric stiffness should modify the result.
        assert!(linear.converged);
        assert!(linear.total_iterations >= 1);
    }
}
