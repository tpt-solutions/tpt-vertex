//! Native FFI boundary for desktop (C ABI) use.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! Enabled with the `ffi` feature. These functions form a stable C-ABI surface
//! that the Tauri desktop client (or any native host) can call via `dlopen` /
//! FFI. Geometry is exchanged as raw pointers to `f32`/`u32` buffers that the
//! caller owns and frees via `vertex_free_buffer`.
//!
//! This is a thin, explicit boundary: the rich Rust API stays internal, and
//! only flat arrays cross the FFI line.

use crate::feature_tree::{Feature, FeatureTree};
use crate::geometry::sketch::Sketch;
use crate::math::Vec2;
use std::ffi::c_void;
use std::os::raw::c_int;

/// Opaque model handle.
pub struct VertexModel {
    tree: FeatureTree,
}

/// Create a new empty model. Returns null on allocation failure.
///
/// # Safety
/// The returned pointer must be released with `vertex_model_free`.
#[no_mangle]
pub unsafe extern "C" fn vertex_model_new() -> *mut VertexModel {
    Box::into_raw(Box::new(VertexModel {
        tree: FeatureTree::new(),
    }))
}

/// Free a model created by `vertex_model_new`.
///
/// # Safety
/// `model` must be a pointer returned by `vertex_model_new` and not used
/// afterwards.
#[no_mangle]
pub unsafe extern "C" fn vertex_model_free(model: *mut VertexModel) {
    if !model.is_null() {
        let _ = Box::from_raw(model);
    }
}

/// Add an extruded box (rectangle in XY, extruded along Z) to the model.
///
/// # Safety
/// `model` must be a valid model pointer.
#[no_mangle]
pub unsafe extern "C" fn vertex_model_add_box(
    model: *mut VertexModel,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    height: f64,
) -> c_int {
    if model.is_null() {
        return -1;
    }
    let model = &mut *model;
    let mut sk = Sketch::new();
    sk.line(Vec2::new(x0, y0), Vec2::new(x1, y0));
    sk.line(Vec2::new(x1, y0), Vec2::new(x1, y1));
    sk.line(Vec2::new(x1, y1), Vec2::new(x0, y1));
    sk.line(Vec2::new(x0, y1), Vec2::new(x0, y0));
    model
        .tree
        .add(Feature::Extrude { sketch: sk, height }, None);
    0
}

/// Evaluate the model and write the final solid's vertex buffer (f32, xyz
/// interleaved) into a freshly allocated buffer. Sets `*out_vertices` and
/// `*out_vertex_count`, and returns the triangle count, or a negative error
/// code on failure.
///
/// # Safety
/// `model`, `out_vertices`, `out_vertex_count` must be valid pointers. The
/// caller must free `*out_vertices` with `vertex_free_buffer`.
#[no_mangle]
pub unsafe extern "C" fn vertex_model_eval_vertices(
    model: *const VertexModel,
    out_vertices: *mut *mut f32,
    out_vertex_count: *mut usize,
) -> c_int {
    if model.is_null() || out_vertices.is_null() || out_vertex_count.is_null() {
        return -1;
    }
    let model = &*model;
    let eval = match model.tree.evaluate() {
        Ok(e) => e,
        Err(_) => return -2,
    };
    let mut buf: Vec<f32> = Vec::with_capacity(eval.final_solid.vertex_count() * 3);
    for v in &eval.final_solid.vertices {
        buf.push(v.x as f32);
        buf.push(v.y as f32);
        buf.push(v.z as f32);
    }
    *out_vertex_count = buf.len();
    *out_vertices = buf.leak().as_mut_ptr();
    eval.final_solid.triangle_count() as c_int
}

/// Free a buffer returned by the FFI (vertex or index buffers).
///
/// # Safety
/// `ptr` must have been returned by a `vertex_*` function and not freed yet.
#[no_mangle]
pub unsafe extern "C" fn vertex_free_buffer(ptr: *mut c_void) {
    if !ptr.is_null() {
        let _ = Vec::from_raw_parts(ptr as *mut f32, 0, 0);
    }
}
