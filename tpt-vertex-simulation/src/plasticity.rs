// SPDX-License-Identifier: MIT OR Apache-2.0

//! Nonlinear J2 (von Mises) plasticity with isotropic hardening.
//!
//! Implements the return-mapping algorithm for small-strain J2 plasticity
//! with a user-defined isotropic hardening curve `σ_y(ε_p)`. The algorithm
//! uses the classic elastic-predictor / plastic-corrector (radial return)
//! scheme per Gauss point, which is unconditionally stable for the
//! constant-strain tetrahedron.
//!
//! The Newton-Raphson global iteration is driven from `crate::nonlinear`
//! (or directly via [`nonlinear_solve`]) — this module provides the
//! *material-level* tangent and stress update that each global iteration
//! calls at the element level.

use crate::element::strain_displacement;
use crate::material::elastic_matrix;

/// Hardening law: returns the current yield stress given the accumulated
/// equivalent plastic strain `eps_p` and the material's initial yield
/// strength `sigma_y0`.
#[derive(Debug, Clone)]
pub enum HardeningLaw {
    /// Perfectly plastic: no hardening.
    Perfect,
    /// Linear isotropic hardening: `σ_y = σ_y0 + H * ε_p`.
    Linear { hardening_modulus: f64 },
    /// Swift power-law hardening: `σ_y = K * (ε_p0 + ε_p)^n`.
    Swift { k: f64, eps_p0: f64, n: f64 },
    /// Hollomon power-law: `σ_y = K * ε_p^n` (for ε_p > 0; initial yield
    /// at ε_p0).
    Hollomon { k: f64, eps_p0: f64, n: f64 },
}

impl HardeningLaw {
    /// Current yield stress at accumulated plastic strain `eps_p`.
    pub fn yield_stress(&self, sigma_y0: f64, eps_p: f64) -> f64 {
        match self {
            HardeningLaw::Perfect => sigma_y0,
            HardeningLaw::Linear { hardening_modulus } => sigma_y0 + hardening_modulus * eps_p,
            HardeningLaw::Swift { k, eps_p0, n } => k * (eps_p0 + eps_p).powf(*n),
            HardeningLaw::Hollomon { k, eps_p0, n } => {
                if eps_p + eps_p0 > 0.0 {
                    k * (eps_p + eps_p0).powf(*n)
                } else {
                    sigma_y0
                }
            }
        }
    }

    /// Derivative `dσ_y/dε_p` (tangent hardening modulus).
    pub fn tangent_modulus(&self, _sigma_y0: f64, eps_p: f64) -> f64 {
        match self {
            HardeningLaw::Perfect => 0.0,
            HardeningLaw::Linear { hardening_modulus } => *hardening_modulus,
            HardeningLaw::Swift { k, eps_p0, n } => {
                if eps_p0 + eps_p > 0.0 {
                    k * n * (eps_p0 + eps_p).powf(n - 1.0)
                } else {
                    0.0
                }
            }
            HardeningLaw::Hollomon { k, eps_p0, n } => {
                let base = eps_p + eps_p0;
                if base > 0.0 {
                    k * n * base.powf(n - 1.0)
                } else {
                    0.0
                }
            }
        }
    }
}

/// Per-element integration point state for plasticity.
#[derive(Debug, Clone, Default)]
pub struct IntegrationPointState {
    /// Accumulated equivalent plastic strain.
    pub eps_p: f64,
    /// Current trial stress norm (for convergence checks).
    pub _trial_norm: f64,
}

/// The 6-component trial stress and plastic strain state at an integration
/// point, used for the radial return algorithm.
#[derive(Debug, Clone, Copy)]
pub struct StressState {
    /// Trial (elastic) stress in Voigt notation.
    pub trial: [f64; 6],
    /// Accumulated equivalent plastic strain.
    pub eps_p: f64,
}

/// Radial return stress update for J2 plasticity.
///
/// Given the trial stress `σ_trial` (from `D : ε_total`) and the current
/// accumulated plastic strain `eps_p`, compute:
/// 1. The deviatoric trial stress and its norm.
/// 2. The trial equivalent (von Mises) stress.
/// 3. If σ_eq > σ_y(eps_p): plastic flow occurs.
/// 4. Return the corrected stress and updated plastic strain.
///
/// Returns `(corrected_stress, new_eps_p, consistent_tangent_factor)`.
/// The consistent tangent factor `α` is used to modify the material
/// tangent: `D_ct = D - α * n⊗n` where `n` is the deviatoric stress
/// direction.
#[allow(clippy::too_many_arguments)] // mirrors the FEA material parameter set used throughout this crate
pub fn radial_return(
    sigma_trial: [f64; 6],
    eps_p: f64,
    e: f64,
    nu: f64,
    sigma_y0: f64,
    hardening: &HardeningLaw,
    tol: f64,
    max_iter: usize,
) -> ([f64; 6], f64, f64) {
    // Deviatoric stress.
    let sm = (sigma_trial[0] + sigma_trial[1] + sigma_trial[2]) / 3.0;
    let s_dev = [
        sigma_trial[0] - sm,
        sigma_trial[1] - sm,
        sigma_trial[2] - sm,
        sigma_trial[3],
        sigma_trial[4],
        sigma_trial[5],
    ];

    // Von Mises equivalent stress: sqrt(3/2 * s_dev : s_dev).
    let j2 = 0.5
        * (s_dev[0] * s_dev[0]
            + s_dev[1] * s_dev[1]
            + s_dev[2] * s_dev[2]
            + 2.0 * (s_dev[3] * s_dev[3] + s_dev[4] * s_dev[4] + s_dev[5] * s_dev[5]));
    let sigma_eq = (3.0 * j2).sqrt();

    let sy = hardening.yield_stress(sigma_y0, eps_p);

    // Elastic check: if sigma_eq <= sigma_y, no plastic flow.
    if sigma_eq <= sy + tol {
        return (sigma_trial, eps_p, 0.0);
    }

    // Plastic corrector: radial return with Newton iteration on the
    // equivalent plastic strain increment Δε_p.
    //
    // The residual is: R(Δε_p) = σ_eq_trial - 3G·Δε_p - σ_y(eps_p + Δε_p)
    // where G = E / (2(1+ν)).
    let g = e / (2.0 * (1.0 + nu));

    let mut dep = 0.0; // Δε_p
    let mut sy_cur = sy;
    for _ in 0..max_iter {
        let sy_next = hardening.yield_stress(sigma_y0, eps_p + dep);
        let h = hardening.tangent_modulus(sigma_y0, eps_p + dep);
        let residual = sigma_eq - 3.0 * g * dep - sy_next;
        // Newton update is dep_new = dep - R/R' where R'(dep) = -(3G + H).
        // Using `tangent = 3G + H` (the negation) lets us add residual/tangent
        // directly; the previous `-3G - H` with `dep += residual/tangent`
        // applied the update with the wrong sign, driving Δε_p negative and
        // pushing the corrected stress further from (rather than onto) the
        // yield surface.
        let tangent = 3.0 * g + h;
        if tangent.abs() < 1e-18 {
            break;
        }
        let delta = residual / tangent;
        dep += delta;
        sy_cur = sy_next;
        if delta.abs() < tol {
            break;
        }
    }

    // Corrected stress: σ = σ_trial - 3G·Δε_p · (s_dev / σ_eq_trial).
    let factor = 3.0 * g * dep / sigma_eq;
    let mut sigma_corr = [0.0; 6];
    for i in 0..6 {
        sigma_corr[i] = sigma_trial[i] - factor * s_dev[i];
    }
    // The shear components (i=3,4,5) had no mean-subtraction applied in s_dev,
    // so they're already correct: s_dev[3] = sigma_trial[3], etc.
    // The factor is applied uniformly, which is correct for radial return.

    // Consistent tangent factor: α = 3G / (3G + H) * (1 - sy/σ_eq)
    // This gives the elastoplastic tangent: D_ep = D - α * n⊗n
    let h = hardening.tangent_modulus(sigma_y0, eps_p + dep);
    let alpha = if (3.0 * g + h).abs() > 1e-18 {
        3.0 * g * sy_cur / (3.0 * g + h) / sigma_eq
    } else {
        0.0
    };

    (sigma_corr, eps_p + dep, alpha)
}

/// Compute the element internal force vector and tangent stiffness for a
/// nonlinear material. This is the element-level routine called by the
/// Newton-Raphson global solver.
///
/// Given the current displacement `u_e` (30 for quad tet, 12 for lin tet),
/// the integration point states, and the hardening law, computes:
/// - `f_int`: internal force vector (equivalent to `∫ B^T σ dV`)
/// - `k_ep`: elastoplastic tangent stiffness (for the global tangent matrix)
#[allow(clippy::too_many_arguments)] // mirrors the FEA material parameter set used throughout this crate
#[allow(clippy::needless_range_loop)] // fixed-size 6x6/12x12 matrix indexing is clearest with range loops
pub fn element_nonlinear_force_and_tangent(
    nodes_lin: &[[f64; 3]; 4],
    u_e: &[f64],
    e: f64,
    nu: f64,
    sigma_y0: f64,
    hardening: &HardeningLaw,
    ip_states: &mut [IntegrationPointState; 1],
    _is_quadratic: bool,
) -> ([f64; 12], [[f64; 12]; 12]) {
    // For the linear tet (v1), use a single integration point at the centroid.
    // For quadratic tets this would use multiple Gauss points, but we keep
    // the linear-tet interface for v1 simplicity.
    let d = elastic_matrix(e, nu);
    let b = strain_displacement(nodes_lin);
    let vol = crate::element::tet_volume(nodes_lin).abs();

    // Gather element displacement (12 DOFs).
    let mut ue = [0.0; 12];
    ue.copy_from_slice(u_e);

    // Compute total strain: ε = B u_e (6-vector).
    let mut strain = [0.0; 6];
    for i in 0..6 {
        let mut s = 0.0;
        for j in 0..12 {
            s += b[i][j] * ue[j];
        }
        strain[i] = s;
    }

    // Trial stress: σ_trial = D ε.
    let mut sigma_trial = [0.0; 6];
    for i in 0..6 {
        let mut s = 0.0;
        for j in 0..6 {
            s += d[i][j] * strain[j];
        }
        sigma_trial[i] = s;
    }

    // Radial return.
    let ip = &mut ip_states[0];
    let (sigma_corr, new_eps_p, alpha) =
        radial_return(sigma_trial, ip.eps_p, e, nu, sigma_y0, hardening, 1e-8, 50);
    ip.eps_p = new_eps_p;

    // Internal force: f_int = V * B^T σ_corr.
    let mut f_int = [0.0; 12];
    for i in 0..12 {
        let mut s = 0.0;
        for j in 0..6 {
            s += b[j][i] * sigma_corr[j];
        }
        f_int[i] = vol * s;
    }

    // Elastoplastic tangent: K_ep = V * B^T D_ep B.
    // D_ep = D - α * n⊗n where n = s_dev / ||s_dev|| (deviatoric direction).
    // For simplicity, use the scalar reduction: D_ep ≈ D * (1 - α * σ_eq / (3G))
    // which is the isotropic approximation.
    let g = e / (2.0 * (1.0 + nu));
    let reduction = if alpha > 0.0 && sigma_eq_from_trial(sigma_trial) > 1e-12 {
        alpha
    } else {
        0.0
    };

    // Build the effective D_ep matrix.
    let mut d_ep = d;
    // Subtract the isotropic plastic correction from the deviatoric part.
    // The correction reduces the shear moduli by `reduction * 3G / (2 * sigma_eq)`.
    let sigma_eq = sigma_eq_from_trial(sigma_trial);
    if reduction > 0.0 && sigma_eq > 1e-12 {
        let corr = reduction * 3.0 * g / sigma_eq;
        // Approximate: reduce the shear entries of D by corr.
        for i in 3..6 {
            for j in 3..6 {
                d_ep[i][j] *= 1.0 - corr;
            }
        }
    }

    let mut k_ep = [[0.0; 12]; 12];
    for i in 0..12 {
        for j in 0..12 {
            let mut s = 0.0;
            for k in 0..6 {
                s += b[k][i] * d_ep[k][j];
            }
            k_ep[i][j] = vol * s;
        }
    }

    (f_int, k_ep)
}

/// Von Mises equivalent stress from a stress tensor (Voigt notation).
fn sigma_eq_from_trial(sigma: [f64; 6]) -> f64 {
    let sm = (sigma[0] + sigma[1] + sigma[2]) / 3.0;
    let s0 = sigma[0] - sm;
    let s1 = sigma[1] - sm;
    let s2 = sigma[2] - sm;
    let j2 = 0.5
        * (s0 * s0
            + s1 * s1
            + s2 * s2
            + 2.0 * (sigma[3] * sigma[3] + sigma[4] * sigma[4] + sigma[5] * sigma[5]));
    (3.0 * j2).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elastic_region_no_plastic_flow() {
        let hardening = HardeningLaw::Perfect;
        let sigma_trial = [100.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        let (sigma, eps_p, alpha) = radial_return(
            sigma_trial,
            0.0,
            200_000.0,
            0.3,
            250.0,
            &hardening,
            1e-8,
            50,
        );
        // sigma_eq = 100 < 250, so no plastic flow.
        assert!((sigma[0] - 100.0).abs() < 1e-9);
        assert!(eps_p < 1e-12);
        assert!(alpha.abs() < 1e-12);
    }

    #[test]
    fn plastic_flow_reduces_stress() {
        let hardening = HardeningLaw::Perfect;
        // Uniaxial tension above yield: σ_trial = [500, 0, 0, 0, 0, 0]
        // von Mises = 500 > 250 => plastic flow.
        let sigma_trial = [500.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        let (sigma, eps_p, _alpha) = radial_return(
            sigma_trial,
            0.0,
            200_000.0,
            0.3,
            250.0,
            &hardening,
            1e-8,
            50,
        );
        // After return, von Mises should be ≈ 250 (yield surface).
        let vm = sigma_eq_from_trial(sigma);
        assert!((vm - 250.0).abs() < 1.0, "von Mises after return: {vm}");
        assert!(eps_p > 0.0, "plastic strain should be positive");
    }

    #[test]
    fn linear_hardening_increases_yield() {
        let hardening = HardeningLaw::Linear {
            hardening_modulus: 1000.0,
        };
        let sigma_trial = [500.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        let (_, eps_p1, _) = radial_return(
            sigma_trial,
            0.0,
            200_000.0,
            0.3,
            250.0,
            &hardening,
            1e-8,
            50,
        );
        // With hardening, the yield stress increases, so less plastic strain
        // than the perfect-plasticity case.
        let (_, eps_p2, _) = radial_return(
            sigma_trial,
            0.0,
            200_000.0,
            0.3,
            250.0,
            &HardeningLaw::Perfect,
            1e-8,
            50,
        );
        assert!(eps_p1 < eps_p2, "hardening should reduce plastic strain");
    }

    #[test]
    fn yield_stress_matches_hardening_law() {
        let law = HardeningLaw::Linear {
            hardening_modulus: 500.0,
        };
        assert!((law.yield_stress(200.0, 0.0) - 200.0).abs() < 1e-9);
        assert!((law.yield_stress(200.0, 0.1) - 250.0).abs() < 1e-9);
        assert!((law.tangent_modulus(200.0, 0.5) - 500.0).abs() < 1e-9);
    }
}
