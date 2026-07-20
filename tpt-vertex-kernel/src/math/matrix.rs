//! Matrix types (column-major, row-vector convention).
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0

use crate::math::{Vec2, Vec3};

/// 3x3 matrix stored column-major as three column vectors.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat3 {
    pub cols: [Vec3; 3],
}

impl Mat3 {
    pub fn identity() -> Self {
        Mat3 {
            cols: [Vec3::X, Vec3::Y, Vec3::Z],
        }
    }

    /// Build a 3x3 matrix from three column vectors.
    pub fn from_cols(c0: Vec3, c1: Vec3, c2: Vec3) -> Self {
        Mat3 { cols: [c0, c1, c2] }
    }

    /// Build a 3x3 matrix from row-major `f64` values.
    pub fn from_row_major(m: [f64; 9]) -> Self {
        Mat3 {
            cols: [
                Vec3::new(m[0], m[3], m[6]),
                Vec3::new(m[1], m[4], m[7]),
                Vec3::new(m[2], m[5], m[8]),
            ],
        }
    }

    pub fn mul_vec(self, v: Vec3) -> Vec3 {
        self.cols[0] * v.x + self.cols[1] * v.y + self.cols[2] * v.z
    }

    pub fn transpose(self) -> Self {
        Mat3::from_row_major([
            self.cols[0].x,
            self.cols[0].y,
            self.cols[0].z,
            self.cols[1].x,
            self.cols[1].y,
            self.cols[1].z,
            self.cols[2].x,
            self.cols[2].y,
            self.cols[2].z,
        ])
    }
}

/// 4x4 matrix stored column-major as four column vectors. Used for affine
/// transforms in homogeneous coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat4 {
    pub cols: [Vec4; 4],
}

/// 4-dimensional homogeneous vector.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vec4 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub w: f64,
}

impl Vec4 {
    pub fn new(x: f64, y: f64, z: f64, w: f64) -> Self {
        Vec4 { x, y, z, w }
    }

    pub fn from_vec3(v: Vec3, w: f64) -> Self {
        Vec4::new(v.x, v.y, v.z, w)
    }

    pub fn xyz(self) -> Vec3 {
        Vec3::new(self.x, self.y, self.z)
    }
}

impl Mul<f64> for Vec4 {
    type Output = Vec4;
    fn mul(self, rhs: f64) -> Vec4 {
        Vec4::new(self.x * rhs, self.y * rhs, self.z * rhs, self.w * rhs)
    }
}

impl Add for Vec4 {
    type Output = Vec4;
    fn add(self, rhs: Vec4) -> Vec4 {
        Vec4::new(
            self.x + rhs.x,
            self.y + rhs.y,
            self.z + rhs.z,
            self.w + rhs.w,
        )
    }
}

impl Mat4 {
    pub fn identity() -> Self {
        Mat4 {
            cols: [
                Vec4::new(1.0, 0.0, 0.0, 0.0),
                Vec4::new(0.0, 1.0, 0.0, 0.0),
                Vec4::new(0.0, 0.0, 1.0, 0.0),
                Vec4::new(0.0, 0.0, 0.0, 1.0),
            ],
        }
    }

    pub fn from_cols(c0: Vec4, c1: Vec4, c2: Vec4, c3: Vec4) -> Self {
        Mat4 {
            cols: [c0, c1, c2, c3],
        }
    }

    pub fn translation(t: Vec3) -> Self {
        let mut m = Mat4::identity();
        m.cols[3] = Vec4::new(t.x, t.y, t.z, 1.0);
        m
    }

    pub fn scaling(s: Vec3) -> Self {
        Mat4::from_cols(
            Vec4::new(s.x, 0.0, 0.0, 0.0),
            Vec4::new(0.0, s.y, 0.0, 0.0),
            Vec4::new(0.0, 0.0, s.z, 0.0),
            Vec4::new(0.0, 0.0, 0.0, 1.0),
        )
    }

    pub fn mul_vec(self, v: Vec4) -> Vec4 {
        self.cols[0] * v.x + self.cols[1] * v.y + self.cols[2] * v.z + self.cols[3] * v.w
    }

    /// Transform a point (w = 1). Returns the transformed point.
    pub fn transform_point(self, p: Vec3) -> Vec3 {
        self.mul_vec(Vec4::from_vec3(p, 1.0)).xyz()
    }

    /// Transform a direction (w = 0, ignores translation).
    pub fn transform_dir(self, d: Vec3) -> Vec3 {
        self.mul_vec(Vec4::from_vec3(d, 0.0)).xyz()
    }

    pub fn transpose(self) -> Self {
        Mat4::from_cols(
            Vec4::new(
                self.cols[0].x,
                self.cols[1].x,
                self.cols[2].x,
                self.cols[3].x,
            ),
            Vec4::new(
                self.cols[0].y,
                self.cols[1].y,
                self.cols[2].y,
                self.cols[3].y,
            ),
            Vec4::new(
                self.cols[0].z,
                self.cols[1].z,
                self.cols[2].z,
                self.cols[3].z,
            ),
            Vec4::new(
                self.cols[0].w,
                self.cols[1].w,
                self.cols[2].w,
                self.cols[3].w,
            ),
        )
    }

    /// Upper-left 3x3 rotation/scale block.
    pub fn mat3(self) -> Mat3 {
        Mat3::from_cols(self.cols[0].xyz(), self.cols[1].xyz(), self.cols[2].xyz())
    }
}

/// 2x2 matrix used by the 2D constraint solver's Jacobian blocks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat2 {
    pub m: [[f64; 2]; 2],
}

impl Mat2 {
    pub fn identity() -> Self {
        Mat2 {
            m: [[1.0, 0.0], [0.0, 1.0]],
        }
    }

    pub fn mul_vec(self, v: Vec2) -> Vec2 {
        Vec2::new(
            self.m[0][0] * v.x + self.m[0][1] * v.y,
            self.m[1][0] * v.x + self.m[1][1] * v.y,
        )
    }
}

#[allow(unused_imports)]
use std::ops::Mul;

#[allow(unused_imports)]
use std::ops::Add;

impl Mul<Mat4> for Mat4 {
    type Output = Mat4;
    fn mul(self, rhs: Mat4) -> Mat4 {
        let cols = [
            self.mul_vec(rhs.cols[0]),
            self.mul_vec(rhs.cols[1]),
            self.mul_vec(rhs.cols[2]),
            self.mul_vec(rhs.cols[3]),
        ];
        Mat4 { cols }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mat4_translation() {
        let t = Mat4::translation(Vec3::new(1.0, 2.0, 3.0));
        let p = t.transform_point(Vec3::new(0.0, 0.0, 0.0));
        assert_eq!(p, Vec3::new(1.0, 2.0, 3.0));
    }

    #[test]
    fn mat4_identity_point() {
        let p = Mat4::identity().transform_point(Vec3::new(5.0, 6.0, 7.0));
        assert_eq!(p, Vec3::new(5.0, 6.0, 7.0));
    }

    #[test]
    fn mat4_mul_compose() {
        let a = Mat4::translation(Vec3::new(1.0, 0.0, 0.0));
        let b = Mat4::translation(Vec3::new(0.0, 2.0, 0.0));
        let p = (a * b).transform_point(Vec3::ZERO);
        assert_eq!(p, Vec3::new(1.0, 2.0, 0.0));
    }
}
