//! Floating-point tolerancing and approximate comparison for geometry.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! All kernel geometry is `f64`. Exact equality is unsafe for computed
//! geometry, so comparisons use a small absolute epsilon plus a relative
//! component scaled by magnitude.

use crate::math::{Vec2, Vec3};

/// Default absolute tolerance for coincident-point tests, in model units.
pub const EPSILON: f64 = 1e-9;

/// Larger tolerance for "good enough" UI-level comparisons (e.g. snapping).
pub const UI_EPSILON: f64 = 1e-6;

/// Relative tolerance used in [`relative_eq`].
pub const REL_TOL: f64 = 1e-9;

/// Approximate equality with an absolute + relative tolerance.
pub fn relative_eq(a: f64, b: f64, eps: f64, rel: f64) -> bool {
    let diff = (a - b).abs();
    diff <= eps || diff <= rel * a.abs().max(b.abs())
}

/// Default approximate equality.
pub fn eq(a: f64, b: f64) -> bool {
    relative_eq(a, b, EPSILON, REL_TOL)
}

/// Approximate equality of two 2D points within `eps`.
pub fn vec2_eq(a: Vec2, b: Vec2, eps: f64) -> bool {
    a.distance(b) <= eps
}

/// Approximate equality of two 3D points within `eps`.
pub fn vec3_eq(a: Vec3, b: Vec3, eps: f64) -> bool {
    a.distance(b) <= eps
}

/// Clamp `x` to the inclusive range `[lo, hi]`.
pub fn clamp(x: f64, lo: f64, hi: f64) -> f64 {
    if x < lo {
        lo
    } else if x > hi {
        hi
    } else {
        x
    }
}

/// Round `x` to `digits` decimal places (stable, used for display only).
pub fn round_to(x: f64, digits: u32) -> f64 {
    let f = 10f64.powi(digits as i32);
    (x * f).round() / f
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::{Vec2, Vec3};

    #[test]
    fn relative_eq_handles_scale() {
        assert!(eq(1.0, 1.0 + 1e-12));
        assert!(!eq(1.0, 1.001));
        assert!(relative_eq(1e6, 1e6 + 1e-4, 1e-9, 1e-9));
        assert!(!relative_eq(1e6, 1e6 + 1.0, 1e-9, 1e-9));
    }

    #[test]
    fn vec_eq() {
        assert!(vec3_eq(
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1e-10, 0.0, 0.0),
            EPSILON
        ));
        assert!(!vec3_eq(
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1e-3, 0.0, 0.0),
            EPSILON
        ));
        assert!(vec2_eq(Vec2::X, Vec2::X, EPSILON));
    }

    #[test]
    fn clamp_and_round() {
        assert_eq!(clamp(5.0, 0.0, 3.0), 3.0);
        assert_eq!(round_to(1.23456, 2), 1.23);
    }
}
