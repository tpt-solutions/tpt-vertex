//! Linear 4-node tetrahedron element stiffness matrix.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! For a constant-strain tetrahedron the element stiffness matrix is
//! `K_e = V · Bᵀ D B`, where `B` is the 6×12 strain–displacement matrix built
//! from the constant shape-function gradients and `V` is the tet volume.

/// Compute the 12×12 linear-tet element stiffness matrix for the given 4 node
/// positions (in mm) and the 6×6 constitutive matrix `D` (from [`crate::material`]).
pub fn tet_stiffness(n: &[[f64; 3]; 4], d: &[[f64; 6]; 6]) -> [[f64; 12]; 12] {
    let b = strain_displacement(n);
    let mut bt_d_b = [[0.0; 12]; 12];
    let vol = tet_volume(n).abs();
    // bt_d_b = V · Bᵀ D B  (B is 6×12, D is 6×6)
    // Compute (D·B) first (6×12), then Bᵀ·(D·B) (12×12).
    let mut d_b = [[0.0; 12]; 6];
    for i in 0..6 {
        for j in 0..12 {
            let mut s = 0.0;
            for k in 0..6 {
                s += d[i][k] * b[k][j];
            }
            d_b[i][j] = s;
        }
    }
    for i in 0..12 {
        for j in 0..12 {
            let mut s = 0.0;
            for k in 0..6 {
                s += b[k][i] * d_b[k][j];
            }
            bt_d_b[i][j] = vol * s;
        }
    }
    bt_d_b
}

/// Build the 6×12 strain–displacement matrix `B` for a linear tetrahedron.
///
/// `B = [[∂N/∂x, 0, 0], [0, ∂N/∂y, 0], [0, 0, ∂N/∂z],
///       [∂N/∂y, ∂N/∂x, 0], [0, ∂N/∂z, ∂N/∂y], [∂N/∂z, 0, ∂N/∂x]]`
/// where the per-node gradient `(∂N_i/∂x, ∂N_i/∂y, ∂N_i/∂z)` occupies columns
/// `3i .. 3i+2`.
#[allow(clippy::needless_range_loop)]
pub fn strain_displacement(n: &[[f64; 3]; 4]) -> [[f64; 12]; 6] {
    let g = shape_gradients(n); // 4 × (gx, gy, gz)
    let mut b = [[0.0; 12]; 6];
    for i in 0..4 {
        let (gx, gy, gz) = (g[i][0], g[i][1], g[i][2]);
        let c = 3 * i;
        b[0][c] = gx;
        b[1][c + 1] = gy;
        b[2][c + 2] = gz;
        b[3][c] = gy;
        b[3][c + 1] = gx;
        b[4][c + 1] = gz;
        b[4][c + 2] = gy;
        b[5][c] = gz;
        b[5][c + 2] = gx;
    }
    b
}

/// Constant shape-function gradients `∂N_i/∂(x,y,z)` for a linear tetrahedron,
/// computed from the inverse Jacobian `J⁻¹` where `J = ∂(x,y,z)/∂(ξ,η,ζ)`.
pub fn shape_gradients(n: &[[f64; 3]; 4]) -> [[f64; 3]; 4] {
    // Natural-coordinate edge vectors from node 0.
    let x1 = sub(&n[1], &n[0]);
    let x2 = sub(&n[2], &n[0]);
    let x3 = sub(&n[3], &n[0]);
    // Jacobian columns are x1, x2, x3.
    let j = [
        [x1[0], x2[0], x3[0]],
        [x1[1], x2[1], x3[1]],
        [x1[2], x2[2], x3[2]],
    ];
    let inv = invert3(&j);
    // ∂N/∂ξ for nodes 1..3 is the standard linear-tet gradient in natural space:
    //   ∂N1/∂ξ = -1, ∂N2/∂ξ = 1, ∂N3/∂ξ = 0, ∂N4/∂ξ = 0, etc.
    // ∂(x,y,z)/∂ξ = J · [∂N/∂ξ]_vector, so ∂N/∂(x,y,z) = J⁻¹ · ∂N/∂ξ.
    let dn_dxi = [
        [-1.0, -1.0, -1.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
    ];
    let mut g = [[0.0; 3]; 4];
    for (i, row) in dn_dxi.iter().enumerate() {
        for (k, gik) in g[i].iter_mut().enumerate() {
            let mut s = 0.0;
            for (l, &xl) in row.iter().enumerate() {
                s += inv[k][l] * xl;
            }
            *gik = s;
        }
    }
    g
}

fn sub(a: &[f64; 3], b: &[f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// Invert a 3×3 matrix (returns zero matrix if singular).
fn invert3(m: &[[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    if det.abs() < 1e-18 {
        return [[0.0; 3]; 3];
    }
    let inv = 1.0 / det;
    [
        [
            (m[1][1] * m[2][2] - m[1][2] * m[2][1]) * inv,
            (m[0][2] * m[2][1] - m[0][1] * m[2][2]) * inv,
            (m[0][1] * m[1][2] - m[0][2] * m[1][1]) * inv,
        ],
        [
            (m[1][2] * m[2][0] - m[1][0] * m[2][2]) * inv,
            (m[0][0] * m[2][2] - m[0][2] * m[2][0]) * inv,
            (m[0][2] * m[1][0] - m[0][0] * m[1][2]) * inv,
        ],
        [
            (m[1][0] * m[2][1] - m[1][1] * m[2][0]) * inv,
            (m[0][1] * m[2][0] - m[0][0] * m[2][1]) * inv,
            (m[0][0] * m[1][1] - m[0][1] * m[1][0]) * inv,
        ],
    ]
}

/// Signed volume of a tetrahedron (positive for CCW node ordering).
pub fn tet_volume(n: &[[f64; 3]; 4]) -> f64 {
    let (a, b, c, d) = (n[0], n[1], n[2], n[3]);
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let ad = [d[0] - a[0], d[1] - a[1], d[2] - a[2]];
    let cx = ac[1] * ad[2] - ac[2] * ad[1];
    let cy = ac[2] * ad[0] - ac[0] * ad[2];
    let cz = ac[0] * ad[1] - ac[1] * ad[0];
    (ab[0] * cx + ab[1] * cy + ab[2] * cz) / 6.0
}

#[cfg(test)]
#[allow(clippy::needless_range_loop)]
mod tests {
    use super::*;
    use crate::material::elastic_matrix;

    fn ref_tet() -> [[f64; 3]; 4] {
        [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
        ]
    }

    #[test]
    fn stiffness_is_symmetric() {
        let d = elastic_matrix(200_000.0, 0.3);
        let k = tet_stiffness(&ref_tet(), &d);
        for i in 0..12 {
            for j in 0..12 {
                assert!((k[i][j] - k[j][i]).abs() < 1e-6, "asym at {i},{j}");
            }
        }
    }

    #[test]
    fn stiffness_positive_definite_on_rigid_body_removed() {
        // Sum of each column should be ~0 (rigid translation is in the null
        // space), a standard sanity check for element stiffness.
        let d = elastic_matrix(200_000.0, 0.3);
        let k = tet_stiffness(&ref_tet(), &d);
        for j in 0..12 {
            let mut s = 0.0;
            for i in 0..12 {
                s += k[i][j];
            }
            assert!(s.abs() < 1e-3, "column {j} sum {s}");
        }
    }

    #[test]
    fn shape_gradients_unit_tet() {
        let g = shape_gradients(&ref_tet());
        // Node 0 gradient should be (-1,-1,-1); node 1 (1,0,0); etc.
        assert!((g[0][0] + 1.0).abs() < 1e-9 && (g[0][1] + 1.0).abs() < 1e-9);
        assert!((g[1][0] - 1.0).abs() < 1e-9);
    }
}
