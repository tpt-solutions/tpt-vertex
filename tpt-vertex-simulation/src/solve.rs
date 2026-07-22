//! Linear solver for the global FEA system.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! Solves `K u = F` by Gaussian elimination with partial pivoting on the dense
//! system assembled in [`crate::assembly`]. For v1 the mesh sizes are modest
//! enough that a dense direct solve is acceptable; a sparse solver
//! (`faer`) is the recommended scale-out path (see the Phase-11 ADR).

/// Solve `K u = F` for the dense `n×n` matrix `k` (row-major) and vector `f`.
/// Returns the displacement vector `u` of length `n`.
pub fn solve_dense(k: &[f64], f: &[f64], n: usize) -> Vec<f64> {
    // Copy into augmented form [A | b] to avoid mutating the caller's matrix.
    let mut a = vec![0.0; n * n];
    a.copy_from_slice(k);
    let mut b = f.to_vec();

    // Forward elimination with partial pivoting.
    for col in 0..n {
        // Pivot: largest magnitude in column `col` at or below `col`.
        let mut piv = col;
        let mut best = a[col * n + col].abs();
        for r in (col + 1)..n {
            let v = a[r * n + col].abs();
            if v > best {
                best = v;
                piv = r;
            }
        }
        if best < 1e-18 {
            // Singular/near-singular — leave zeros (degenerate mesh).
            continue;
        }
        if piv != col {
            for c in 0..n {
                a.swap(piv * n + c, col * n + c);
            }
            b.swap(piv, col);
        }

        let diag = a[col * n + col];
        for r in (col + 1)..n {
            let factor = a[r * n + col] / diag;
            if factor == 0.0 {
                continue;
            }
            for c in col..n {
                a[r * n + c] -= factor * a[col * n + c];
            }
            b[r] -= factor * b[col];
        }
    }

    // Back substitution.
    let mut u = vec![0.0; n];
    for row in (0..n).rev() {
        let mut sum = b[row];
        for c in (row + 1)..n {
            sum -= a[row * n + c] * u[c];
        }
        let diag = a[row * n + row];
        u[row] = if diag.abs() > 1e-18 { sum / diag } else { 0.0 };
    }
    u
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solves_identity() {
        let k = vec![1.0, 0.0, 0.0, 1.0];
        let u = solve_dense(&k, &[3.0, 4.0], 2);
        assert!((u[0] - 3.0).abs() < 1e-9);
        assert!((u[1] - 4.0).abs() < 1e-9);
    }

    #[test]
    fn solves_2x2() {
        // [2 1][u0] = [5]  => u0=1, u1=3
        // [1 3][u1]   [10]
        let k = vec![2.0, 1.0, 1.0, 3.0];
        let u = solve_dense(&k, &[5.0, 10.0], 2);
        assert!((u[0] - 1.0).abs() < 1e-9);
        assert!((u[1] - 3.0).abs() < 1e-9);
    }

    #[test]
    fn solves_3x3() {
        let k = vec![4.0, -1.0, 0.0, -1.0, 4.0, -1.0, 0.0, -1.0, 3.0];
        let u = solve_dense(&k, &[3.0, 2.0, 1.0], 3);
        // verify K u ≈ f
        let f = [3.0, 2.0, 1.0];
        for i in 0..3 {
            let mut s = 0.0;
            for j in 0..3 {
                s += k[i * 3 + j] * u[j];
            }
            assert!((s - f[i]).abs() < 1e-6, "row {i}: {s} vs {}", f[i]);
        }
    }
}
