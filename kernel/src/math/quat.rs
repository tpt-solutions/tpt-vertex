//! Quaternion type for rotations.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0

use crate::math::{Mat3, Vec3};
use std::f64::consts::PI;
use std::ops::Mul;

/// Unit-norm quaternion `(w, x, y, z)` representing a rotation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quaternion {
    pub w: f64,
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Quaternion {
    pub fn identity() -> Self {
        Quaternion {
            w: 1.0,
            x: 0.0,
            y: 0.0,
            z: 0.0,
        }
    }

    /// Build a quaternion from an axis (need not be normalized) and angle (rad).
    pub fn from_axis_angle(axis: Vec3, angle: f64) -> Self {
        let a = axis.normalize();
        let half = angle * 0.5;
        let s = half.sin();
        Quaternion {
            w: half.cos(),
            x: a.x * s,
            y: a.y * s,
            z: a.z * s,
        }
    }

    pub fn normalize(self) -> Self {
        let len = (self.w * self.w + self.x * self.x + self.y * self.y + self.z * self.z).sqrt();
        if len == 0.0 {
            Quaternion::identity()
        } else {
            Quaternion {
                w: self.w / len,
                x: self.x / len,
                y: self.y / len,
                z: self.z / len,
            }
        }
    }
}

impl Mul<Quaternion> for Quaternion {
    type Output = Quaternion;
    fn mul(self, other: Quaternion) -> Quaternion {
        Quaternion {
            w: self.w * other.w - self.x * other.x - self.y * other.y - self.z * other.z,
            x: self.w * other.x + self.x * other.w + self.y * other.z - self.z * other.y,
            y: self.w * other.y - self.x * other.z + self.y * other.w + self.z * other.x,
            z: self.w * other.z + self.x * other.y - self.y * other.x + self.z * other.w,
        }
    }
}

impl Quaternion {
    /// Compose two rotations: `self` then `other`.
    pub fn compose(self, other: Quaternion) -> Quaternion {
        self * other
    }

    /// Rotate a vector by this quaternion.
    pub fn rotate_vec(self, v: Vec3) -> Vec3 {
        let q = self.normalize();
        let t = Vec3::new(q.x, q.y, q.z).cross(v) * 2.0;
        v + t * q.w + Vec3::new(q.x, q.y, q.z).cross(t)
    }

    /// Convert to a 3x3 rotation matrix (column-major).
    pub fn to_mat3(self) -> Mat3 {
        let q = self.normalize();
        let (w, x, y, z) = (q.w, q.x, q.y, q.z);
        let xx = x * x;
        let yy = y * y;
        let zz = z * z;
        let xy = x * y;
        let xz = x * z;
        let yz = y * z;
        let wx = w * x;
        let wy = w * y;
        let wz = w * z;
        Mat3::from_cols(
            Vec3::new(1.0 - 2.0 * (yy + zz), 2.0 * (xy + wz), 2.0 * (xz - wy)),
            Vec3::new(2.0 * (xy - wz), 1.0 - 2.0 * (xx + zz), 2.0 * (yz + wx)),
            Vec3::new(2.0 * (xz + wy), 2.0 * (yz - wx), 1.0 - 2.0 * (xx + yy)),
        )
    }
}

/// Shorthand to build a rotation quaternion from Euler angles (radians, XYZ order).
#[allow(dead_code)]
pub fn from_euler(x: f64, y: f64, z: f64) -> Quaternion {
    let qx = Quaternion::from_axis_angle(Vec3::X, x);
    let qy = Quaternion::from_axis_angle(Vec3::Y, y);
    let qz = Quaternion::from_axis_angle(Vec3::Z, z);
    (qz * qy * qx).normalize()
}

/// Smallest rotation (in radians) between two unit-ish vectors.
#[allow(dead_code)]
pub fn angle_between(a: Vec3, b: Vec3) -> f64 {
    let d = a.normalize().dot(b.normalize()).clamp(-1.0, 1.0);
    d.acos()
}

/// Construct a quaternion that rotates `from` onto `to`.
#[allow(dead_code)]
pub fn rotation_between(from: Vec3, to: Vec3) -> Quaternion {
    let a = from.normalize();
    let b = to.normalize();
    let d = a.dot(b);
    if d > 1.0 - 1e-9 {
        Quaternion::identity()
    } else if d < -1.0 + 1e-9 {
        // Opposite direction: rotate 180° about any perpendicular axis.
        let axis = if a.x.abs() < 0.9 {
            Vec3::X.cross(a)
        } else {
            Vec3::Y.cross(a)
        };
        Quaternion::from_axis_angle(axis, PI)
    } else {
        let c = a.cross(b);
        Quaternion {
            w: 1.0 + d,
            x: c.x,
            y: c.y,
            z: c.z,
        }
        .normalize()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotate_around_z() {
        let q = Quaternion::from_axis_angle(Vec3::Z, PI / 2.0);
        let r = q.rotate_vec(Vec3::X);
        assert!((r.x).abs() < 1e-12);
        assert!((r.y - 1.0).abs() < 1e-12);
        assert!((r.z).abs() < 1e-12);
    }

    #[test]
    fn rotation_between_aligned() {
        let q = rotation_between(Vec3::X, Vec3::X);
        assert_eq!(q, Quaternion::identity());
    }

    #[test]
    fn to_mat3_matches_rotate() {
        let q = Quaternion::from_axis_angle(Vec3::new(1.0, 1.0, 1.0).normalize(), 0.7);
        let v = Vec3::new(2.0, -3.0, 5.0);
        let via_q = q.rotate_vec(v);
        let via_m = q.to_mat3().mul_vec(v);
        assert!((via_q.x - via_m.x).abs() < 1e-12);
        assert!((via_q.y - via_m.y).abs() < 1e-12);
        assert!((via_q.z - via_m.z).abs() < 1e-12);
    }
}
