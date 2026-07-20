//! Rigid transform (rotation + translation) used throughout the kernel.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0

use crate::math::matrix::Vec4;
use crate::math::{Mat4, Quaternion, Vec3};

/// A rigid (orientation-preserving) transform: `point' = R * point + t`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    pub rotation: Quaternion,
    pub translation: Vec3,
}

impl Transform {
    pub fn identity() -> Self {
        Transform {
            rotation: Quaternion::identity(),
            translation: Vec3::ZERO,
        }
    }

    pub fn from_translation(t: Vec3) -> Self {
        Transform {
            rotation: Quaternion::identity(),
            translation: t,
        }
    }

    pub fn from_rotation(q: Quaternion) -> Self {
        Transform {
            rotation: q,
            translation: Vec3::ZERO,
        }
    }

    pub fn compose(self, other: Transform) -> Transform {
        Transform {
            rotation: self.rotation * other.rotation,
            translation: self.rotation.rotate_vec(other.translation) + self.translation,
        }
    }

    pub fn inverse(self) -> Transform {
        let inv = self.rotation.normalize();
        let r_inv = Quaternion {
            w: inv.w,
            x: -inv.x,
            y: -inv.y,
            z: -inv.z,
        };
        Transform {
            rotation: r_inv,
            translation: r_inv.rotate_vec(-self.translation),
        }
    }

    pub fn transform_point(self, p: Vec3) -> Vec3 {
        self.rotation.rotate_vec(p) + self.translation
    }

    pub fn transform_dir(self, d: Vec3) -> Vec3 {
        self.rotation.rotate_vec(d)
    }

    pub fn to_mat4(self) -> Mat4 {
        let r = self.rotation.to_mat3();
        let t = self.translation;
        Mat4::from_cols(
            Vec4::new(r.cols[0].x, r.cols[0].y, r.cols[0].z, 0.0),
            crate::math::Vec4::new(r.cols[1].x, r.cols[1].y, r.cols[1].z, 0.0),
            crate::math::Vec4::new(r.cols[2].x, r.cols[2].y, r.cols[2].z, 0.0),
            crate::math::Vec4::new(t.x, t.y, t.z, 1.0),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    #[test]
    fn inverse_round_trip() {
        let t = Transform {
            rotation: Quaternion::from_axis_angle(Vec3::Y, 0.5),
            translation: Vec3::new(1.0, 2.0, 3.0),
        };
        let p = Vec3::new(4.0, -1.0, 2.0);
        let back = t.inverse().transform_point(t.transform_point(p));
        assert!((back.x - p.x).abs() < 1e-12);
        assert!((back.y - p.y).abs() < 1e-12);
        assert!((back.z - p.z).abs() < 1e-12);
    }

    #[test]
    fn compose_matches_apply() {
        let a = Transform::from_translation(Vec3::new(1.0, 0.0, 0.0));
        let b = Transform::from_rotation(Quaternion::from_axis_angle(Vec3::Z, PI));
        let c = a.compose(b);
        // c = a ∘ b: apply b (rotate 180° about Z) then a (translate +X).
        let p = c.transform_point(Vec3::X);
        assert!((p.x - 0.0).abs() < 1e-12);
        assert!((p.y - 0.0).abs() < 1e-12);
        assert!((p.z - 0.0).abs() < 1e-12);
    }
}
