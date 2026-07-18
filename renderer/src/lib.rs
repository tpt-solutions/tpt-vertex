//! TPT Vertex WebGPU (wgpu) rendering engine.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! This crate consumes [`tpt_vertex_kernel`] geometry and turns it into draw
//! calls on a [`wgpu`] device. The public surface is intentionally small:
//!
//! - [`lib::Renderer`] owns the GPU device, surface, swap-chain config, and
//!   render pipelines, and exposes `resize` / `render_frame` entry points.
//! - [`camera`], [`scene`], [`mesh`], and [`picking`] are pure (GPU-free)
//!   helpers that the frontend and the renderer core share.

pub mod camera;
pub mod culling;
pub mod mesh;
pub mod picking;
pub mod renderer;
pub mod scene;

#[cfg(all(feature = "web", target_arch = "wasm32"))]
pub mod host;

pub use renderer::Renderer;
