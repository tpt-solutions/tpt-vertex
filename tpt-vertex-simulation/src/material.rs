//! Isotropic linear-elastic constitutive relations.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! Provides the 6×6 stress–strain (constitutive) matrix `D` in Voigt notation
//! for a 3D isotropic material, plus a helper to derive Lamé parameters. Kernel
//! units are millimetres and MPa (N/mm²), so Young's modulus `E` and Poisson's
//! ratio `nu` feed the matrix directly and stresses come out in MPa.

use tpt_vertex_kernel::material::Material;

/// Build the 6×6 isotropic elastic constitutive matrix `D` (Voigt notation).
///
/// Stress/strain ordering is `[εxx, εyy, εzz, γxy, γyz, γzx]`.
/// ```text
/// D = E / ((1+ν)(1-2ν)) *
///     [ 1-ν   ν     ν     0     0     0
///       ν   1-ν    ν     0     0     0
///       ν    ν   1-ν     0     0     0
///       0    0     0  (1-2ν)/2 0     0
///       0    0     0     0  (1-2ν)/2 0
///       0    0     0     0     0  (1-2ν)/2 ]
/// ```
pub fn elastic_matrix(e: f64, nu: f64) -> [[f64; 6]; 6] {
    let d = e / ((1.0 + nu) * (1.0 - 2.0 * nu));
    let c = (1.0 - nu) * d;
    let o = nu * d;
    let s = (1.0 - 2.0 * nu) / 2.0 * d;
    let mut m = [[0.0; 6]; 6];
    m[0][0] = c; m[0][1] = o; m[0][2] = o;
    m[1][0] = o; m[1][1] = c; m[1][2] = o;
    m[2][0] = o; m[2][1] = o; m[2][2] = c;
    m[3][3] = s;
    m[4][4] = s;
    m[5][5] = s;
    m
}

/// Lamé parameters `(λ, μ)` from `E` and `ν`.
pub fn lame(e: f64, nu: f64) -> (f64, f64) {
    let lambda = e * nu / ((1.0 + nu) * (1.0 - 2.0 * nu));
    let mu = e / (2.0 * (1.0 + nu));
    (lambda, mu)
}

/// Extract `(E, ν)` from a kernel [`Material`].
pub fn from_material(m: &Material) -> (f64, f64) {
    (m.youngs_modulus, m.poisson_ratio)
}

/// Multiply the 6×6 constitutive matrix by a 6-vector strain, yielding stress
/// (used in post-processing tests / pipeline validation).
pub fn apply_d(d: &[[f64; 6]; 6], strain: [f64; 6]) -> [f64; 6] {
    let mut out = [0.0; 6];
    for i in 0..6 {
        let mut s = 0.0;
        for j in 0..6 {
            s += d[i][j] * strain[j];
        }
        out[i] = s;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uniaxial_strain_recovers_stress() {
        // Applied uniaxial STRESS σxx = 2000 MPa on steel (E=200000, ν=0.3) with
        // the lateral strains that keep σyy = σzz = 0:
        // Uniaxial STRESS σxx (σyy=σzz=0) => lateral strains εyy=εzz=-ν·εxx.
        let d = elastic_matrix(200_000.0, 0.3);
        let exx = 0.01;
        let eyz = -0.3 * exx;
        let strain = [exx, eyz, eyz, 0.0, 0.0, 0.0];
        let sigma = apply_d(&d, strain);
        // σxx = E·εxx = 2000 MPa; lateral stresses vanish under uniaxial load.
        assert!((sigma[0] - 2000.0).abs() < 1e-6, "σxx {}", sigma[0]);
        assert!(sigma[1].abs() < 1e-6, "σyy {}", sigma[1]);
        assert!(sigma[2].abs() < 1e-6, "σzz {}", sigma[2]);
        assert!(sigma[3].abs() < 1e-9);
    }

    #[test]
    fn pure_shear_strain_gives_shear_stress() {
        let d = elastic_matrix(200_000.0, 0.3);
        // γxy = 0.02 => τxy = G·γxy, G = E/(2(1+ν)) = 76923.08
        let strain = [0.0, 0.0, 0.0, 0.02, 0.0, 0.0];
        let sigma = apply_d(&d, strain);
        let g = 200_000.0 / (2.0 * 1.3);
        assert!((sigma[3] - g * 0.02).abs() < 1e-3);
    }
}
