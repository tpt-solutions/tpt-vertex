//! TPT Vertex geometry kernel.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0

pub mod assembly;
pub mod feature_tree;
pub mod geometry;
pub mod material;
pub mod math;
pub mod tolerance;

#[cfg(target_arch = "wasm32")]
pub mod wasm;

#[cfg(feature = "ffi")]
pub mod ffi;

pub use math::*;
