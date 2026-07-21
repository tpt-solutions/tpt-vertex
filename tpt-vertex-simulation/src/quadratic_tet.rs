// SPDX-License-Identifier: MIT OR Apache-2.0

//! 10-node quadratic tetrahedron element.
//!
//! A higher-order element with 4 corner nodes + 6 mid-edge nodes, using
//! second-order (quadratic) shape functions in natural coordinates. Compared
//! to the constant-strain linear tet, the quadratic tet:
//!
//! - Captures curved geometry (if mid-edge nodes are placed on the true
//!   surface) and parabolic displacement fields.
//! - Does not suffer from volumetric locking or shear locking.
//! - Requires numerical integration (4-point or 5-point Gauss quadrature)
//!   because the B matrix varies within the element.
//!
//! Node ordering follows the standard convention:
//! ```text
//! Corner nodes:  0, 1, 2, 3  (same as linear tet)
//! Mid-edge nodes: 4 = edge(0,1), 5 = edge(1,2), 6 = edge(0,2),
//!                 7 = edge(0,3), 8 = edge(1,3), 9 = edge(2,3)
//! ```
//!
//! Natural coordinates use the barycentric system `(λ₁, λ₂, λ₃, λ₄)` with
//! `λ₁ + λ₂ + λ₃ + λ₄ = 1` and the mapping to `(ξ, η, ζ)` is implicit.

/// Number of nodes per element.
pub const N_NODES: usize = 10;
/// Number of DOFs per element (3 per node).
pub const N_DOFS: usize = 30;
/// Strain components in Voigt notation.
pub const N_STRAIN: usize = 6;

/// Quadratic shape functions in barycentric coordinates `(l1, l2, l3, l4)`.
/// Returns the 10 shape function values.
pub fn shape_functions(l: [f64; 4]) -> [f64; N_NODES] {
    [
        // Corner nodes: N_i = l_i (2l_i - 1)
        l[0] * (2.0 * l[0] - 1.0),
        l[1] * (2.0 * l[1] - 1.0),
        l[2] * (2.0 * l[2] - 1.0),
        l[3] * (2.0 * l[3] - 1.0),
        // Mid-edge nodes: N_ij = 4 l_i l_j
        4.0 * l[0] * l[1], // node 4: edge(0,1)
        4.0 * l[1] * l[2], // node 5: edge(1,2)
        4.0 * l[0] * l[2], // node 6: edge(0,2)
        4.0 * l[0] * l[3], // node 7: edge(0,3)
        4.0 * l[1] * l[3], // node 8: edge(1,3)
        4.0 * l[2] * l[3], // node 9: edge(2,3)
    ]
}

/// Derivatives of shape functions w.r.t. barycentric coordinates.
/// Returns a 10×4 matrix `dN_i/dl_j`.
pub fn dshape_dlambda() -> [[f64; 4]; N_NODES] {
    [
        // Corner nodes: dN_i/dl_i = 4l_i - 1, dN_i/dl_j = 0 (j≠i)
        // Evaluated symbolically: these are functions of l, but we return
        // the general form. Callers must evaluate at a specific point.
        // For constant-stress (linear) this isn't needed; for quadratic we
        // pass the actual lambda values.
        //
        // We use a different approach: return dN/dl as functions that take l.
        // But for simplicity, we compute them inline in the element routine.
        // This function is kept as documentation; actual derivatives are
        // computed in `b_matrix`.
        [0.0; 4], // placeholder — see b_matrix
        [0.0; 4],
        [0.0; 4],
        [0.0; 4],
        [0.0; 4],
        [0.0; 4],
        [0.0; 4],
        [0.0; 4],
        [0.0; 4],
        [0.0; 4],
    ]
}

/// Compute the Jacobian matrix (3×3) at a point with barycentric coordinates
/// `l` for the given 10 node positions. `J_ij = Σ_k (dN_k/dl_i) * x_kj`.
///
/// The barycentric derivatives are:
/// - Corner i: dN_corner_i / dl_i = 4l_i - 1, dN_corner_i / dl_j = 0 (j≠i)
/// - Mid-edge (i,j): dN_ij / dl_i = 4l_j, dN_ij / dl_j = 4l_i, dN_ij / dl_k = 0 (k≠i,j)
#[allow(clippy::needless_range_loop)]
pub fn jacobian(nodes: &[[f64; 3]; N_NODES], l: [f64; 4]) -> [[f64; 3]; 3] {
    // dN/dξ for each node at this integration point (independent coords).
    let dndxi = dndxi_at(l);

    let mut j = [[0.0; 3]; 3];
    for k in 0..N_NODES {
        for i in 0..3 {
            for j_col in 0..3 {
                j[i][j_col] += dndxi[k][i] * nodes[k][j_col];
            }
        }
    }
    j
}

/// Derivatives of shape functions w.r.t. barycentric coordinates at point `l`.
fn dndl_at(l: [f64; 4]) -> [[f64; 4]; N_NODES] {
    let mut dndl = [[0.0; 4]; N_NODES];
    // Corner nodes: dN_corner_i / dl_j = (2*δ_ij - 1) * l_i + l_i * δ_ij
    // = 4*l_i*δ_ij - 1  (but only if we consider the constraint l1+l2+l3+l4=1)
    // Actually: N_i = l_i(2l_i - 1), so dN_i/dl_i = 4l_i - 1, dN_i/dl_j = 0 for j≠i.
    for i in 0..4 {
        dndl[i][i] = 4.0 * l[i] - 1.0;
    }
    // Mid-edge nodes:
    // N_4 = 4 l0 l1: d/dl0=4l1, d/dl1=4l0
    dndl[4][0] = 4.0 * l[1]; dndl[4][1] = 4.0 * l[0];
    // N_5 = 4 l1 l2: d/dl1=4l2, d/dl2=4l1
    dndl[5][1] = 4.0 * l[2]; dndl[5][2] = 4.0 * l[1];
    // N_6 = 4 l0 l2: d/dl0=4l2, d/dl2=4l0
    dndl[6][0] = 4.0 * l[2]; dndl[6][2] = 4.0 * l[0];
    // N_7 = 4 l0 l3: d/dl0=4l3, d/dl3=4l0
    dndl[7][0] = 4.0 * l[3]; dndl[7][3] = 4.0 * l[0];
    // N_8 = 4 l1 l3: d/dl1=4l3, d/dl3=4l1
    dndl[8][1] = 4.0 * l[3]; dndl[8][3] = 4.0 * l[1];
    // N_9 = 4 l2 l3: d/dl2=4l3, d/dl3=4l2
    dndl[9][2] = 4.0 * l[3]; dndl[9][3] = 4.0 * l[2];
    dndl
}

/// Derivatives w.r.t. the 3 independent natural coordinates `(ξ, η, ζ) =
/// (l1, l2, l3)`, with `l0 = 1 - l1 - l2 - l3` treated as dependent (node 0
/// sits at the parametric origin, matching the physical placement of corner
/// node 0 at the reference tet's origin and nodes 1/2/3 along the axes).
///
/// By the chain rule, `dN/dξ_a = dN/dl_{a+1} - dN/dl_0` for `a` in `0..3`
/// (since `∂l0/∂ξ_a = -1`). The raw barycentric derivatives from
/// [`dndl_at`] are *not* directly usable as physical gradients on their
/// own — dropping a column outright (as opposed to subtracting it) silently
/// zeroes out the contribution of any node whose only nonzero raw
/// derivative is w.r.t. the dropped coordinate, corrupting the Jacobian and
/// the strain-displacement matrix for every node. Dropping `l0` (rather
/// than `l3`) is also what keeps `det(J)` positive for this node ordering:
/// `x(l) = l1*p1 + l2*p2 + l3*p3 + l0*p0` reduces to the identity mapping
/// `(x, y, z) = (l1, l2, l3)` on the reference element.
fn dndxi_at(l: [f64; 4]) -> [[f64; 3]; N_NODES] {
    let dndl = dndl_at(l);
    let mut out = [[0.0; 3]; N_NODES];
    for k in 0..N_NODES {
        for a in 0..3 {
            out[k][a] = dndl[k][a + 1] - dndl[k][0];
        }
    }
    out
}

/// Invert a 3×3 matrix. Returns zero matrix if singular.
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

/// Build the 6×30 strain-displacement matrix `B` at integration point with
/// barycentric coordinates `l` for the given 10 node positions.
///
/// `B` maps the 30-element nodal displacement vector to the 6-component
/// Voigt strain at the integration point: `ε = B u_e`.
#[allow(clippy::needless_range_loop)]
pub fn b_matrix(nodes: &[[f64; 3]; N_NODES], l: [f64; 4]) -> [[f64; N_DOFS]; N_STRAIN] {
    let dndxi = dndxi_at(l);
    let j = jacobian(nodes, l);
    let jinv = invert3(&j);

    // dN/dx = dN/dξ * J^{-1}
    let mut dndx = [[0.0; 3]; N_NODES];
    for k in 0..N_NODES {
        for j_col in 0..3 {
            let mut s = 0.0;
            for l_idx in 0..3 {
                s += dndxi[k][l_idx] * jinv[l_idx][j_col];
            }
            dndx[k][j_col] = s;
        }
    }

    let mut b = [[0.0; N_DOFS]; N_STRAIN];
    for k in 0..N_NODES {
        let c = 3 * k;
        b[0][c] = dndx[k][0];      // εxx
        b[1][c + 1] = dndx[k][1];  // εyy
        b[2][c + 2] = dndx[k][2];  // εzz
        b[3][c] = dndx[k][1];      // γxy
        b[3][c + 1] = dndx[k][0];
        b[4][c + 1] = dndx[k][2];  // γyz
        b[4][c + 2] = dndx[k][1];
        b[5][c] = dndx[k][2];      // γzx
        b[5][c + 2] = dndx[k][0];
    }
    b
}

/// Compute the Jacobian determinant at an integration point.
pub fn det_jacobian(nodes: &[[f64; 3]; N_NODES], l: [f64; 4]) -> f64 {
    let j = jacobian(nodes, l);
    j[0][0] * (j[1][1] * j[2][2] - j[1][2] * j[2][1])
        - j[0][1] * (j[1][0] * j[2][2] - j[1][2] * j[2][0])
        + j[0][2] * (j[1][0] * j[2][1] - j[1][1] * j[2][0])
}

/// 4-point Gauss quadrature rule for a tetrahedron (degree 3 polynomial
/// exactness). Points in barycentric coordinates, weights sum to 1/6
/// (the volume of the reference tet in barycentric space).
///
/// From Dunavant (1985).
pub fn gauss_points_4() -> Vec<([f64; 4], f64)> {
    let a = 0.1381966011250105;
    let b = 0.5854101966249685;
    // Standard 4-point rule: weight = 1/4 of total tet volume (1/6 in barycentric)
    // Each weight is 1/24.
    vec![
        ([a, a, a, b], 1.0 / 24.0),
        ([b, a, a, a], 1.0 / 24.0),
        ([a, b, a, a], 1.0 / 24.0),
        ([a, a, b, a], 1.0 / 24.0),
    ]
}

/// Compute the 30×30 element stiffness matrix for a 10-node quadratic
/// tetrahedron using 4-point Gauss quadrature.
///
/// `d` is the 6×6 constitutive matrix from `crate::material::elastic_matrix`.
#[allow(clippy::needless_range_loop)]
pub fn quadratic_tet_stiffness(nodes: &[[f64; 3]; N_NODES], d: &[[f64; 6]; 6]) -> [[f64; N_DOFS]; N_DOFS] {
    let gps = gauss_points_4();
    let mut ke = [[0.0; N_DOFS]; N_DOFS];

    for (l, w) in &gps {
        let b = b_matrix(nodes, *l);
        let det_j = det_jacobian(nodes, *l).abs();
        let wt = w * det_j;

        // K_e += w * det(J) * B^T D B
        // Compute D*B first (6×30).
        let mut db = [[0.0; N_DOFS]; 6];
        for i in 0..6 {
            for j in 0..N_DOFS {
                let mut s = 0.0;
                for k in 0..6 {
                    s += d[i][k] * b[k][j];
                }
                db[i][j] = s;
            }
        }
        // Then B^T * (D*B) → ke (30×30).
        for i in 0..N_DOFS {
            for j in 0..N_DOFS {
                let mut s = 0.0;
                for k in 0..6 {
                    s += b[k][i] * db[k][j];
                }
                ke[i][j] += wt * s;
            }
        }
    }
    ke
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Standard 10-node unit tet with mid-edge nodes at midpoints.
    fn ref_quad_tet() -> [[f64; 3]; N_NODES] {
        let c = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
        ];
        // Mid-edge nodes at midpoints of linear-tet edges.
        let m = [
            [(c[0][0]+c[1][0])/2.0, (c[0][1]+c[1][1])/2.0, (c[0][2]+c[1][2])/2.0], // 4: edge(0,1)
            [(c[1][0]+c[2][0])/2.0, (c[1][1]+c[2][1])/2.0, (c[1][2]+c[2][2])/2.0], // 5: edge(1,2)
            [(c[0][0]+c[2][0])/2.0, (c[0][1]+c[2][1])/2.0, (c[0][2]+c[2][2])/2.0], // 6: edge(0,2)
            [(c[0][0]+c[3][0])/2.0, (c[0][1]+c[3][1])/2.0, (c[0][2]+c[3][2])/2.0], // 7: edge(0,3)
            [(c[1][0]+c[3][0])/2.0, (c[1][1]+c[3][1])/2.0, (c[1][2]+c[3][2])/2.0], // 8: edge(1,3)
            [(c[2][0]+c[3][0])/2.0, (c[2][1]+c[3][1])/2.0, (c[2][2]+c[3][2])/2.0], // 9: edge(2,3)
        ];
        [c[0], c[1], c[2], c[3], m[0], m[1], m[2], m[3], m[4], m[5]]
    }

    #[test]
    fn stiffness_is_symmetric() {
        let d = crate::material::elastic_matrix(200_000.0, 0.3);
        let k = quadratic_tet_stiffness(&ref_quad_tet(), &d);
        for i in 0..N_DOFS {
            for j in 0..N_DOFS {
                assert!((k[i][j] - k[j][i]).abs() < 1e-6, "asym at {i},{j}");
            }
        }
    }

    #[test]
    fn shape_functions_sum_to_one() {
        // At any point in the element, the shape functions must sum to 1.
        let points = [
            [0.25, 0.25, 0.25, 0.25],
            [1.0, 0.0, 0.0, 0.0],
            [0.5, 0.5, 0.0, 0.0],
            [0.1381966011250105, 0.1381966011250105, 0.1381966011250105, 0.5854101966249685],
        ];
        for l in points {
            let n = shape_functions(l);
            let sum: f64 = n.iter().sum();
            assert!((sum - 1.0).abs() < 1e-12, "shape function sum at {:?} = {}", l, sum);
        }
    }

    #[test]
    fn stiffness_column_sums_near_zero() {
        // Rigid-body translation should produce zero strain energy.
        let d = crate::material::elastic_matrix(200_000.0, 0.3);
        let k = quadratic_tet_stiffness(&ref_quad_tet(), &d);
        // Sum each column of K — should be ~0 (rigid translation null space).
        for j in 0..N_DOFS {
            let mut s = 0.0;
            for i in 0..N_DOFS {
                s += k[i][j];
            }
            assert!(s.abs() < 1e-3, "column {j} sum {s}");
        }
    }

    #[test]
    fn det_jacobian_is_positive() {
        let nodes = ref_quad_tet();
        let l = [0.25, 0.25, 0.25, 0.25];
        let det = det_jacobian(&nodes, l);
        assert!(det > 0.0, "det(J) = {det}");
    }
}
