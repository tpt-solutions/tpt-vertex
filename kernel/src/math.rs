//! Core math primitives for the TPT Vertex geometry kernel.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0

mod matrix;
pub mod quat;
mod transform;
mod vec;

pub use matrix::{Mat2, Mat3, Mat4, Vec4};
pub use quat::Quaternion;
pub use transform::Transform;
pub use vec::{Vec2, Vec3};
