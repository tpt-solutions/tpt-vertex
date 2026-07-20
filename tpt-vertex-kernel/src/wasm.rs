//! WebAssembly bindings (browser use via `wasm-bindgen`).
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! Compiled with `cargo build --target wasm32-unknown-unknown` (or `wasm-pack`).
//! Exposes a small, stable API surface for the frontend to build and evaluate
//! geometry without pulling the whole Rust API into JS. Geometry is exchanged
//! as flat `f32` vertex/index buffers suitable for WebGPU.

use crate::feature_tree::{Feature, FeatureTree};
use crate::geometry::sketch::Sketch;
use crate::math::Vec2;
use wasm_bindgen::prelude::*;

/// A minimal, serializable handle to a feature tree built from JS.
#[wasm_bindgen]
pub struct Model {
    tree: FeatureTree,
}

#[wasm_bindgen]
impl Model {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Model {
        Model {
            tree: FeatureTree::new(),
        }
    }

    /// Add an extruded rectangle (x0,y0)-(x1,y1) of the given height.
    pub fn add_box(&mut self, x0: f64, y0: f64, x1: f64, y1: f64, height: f64) {
        let mut sk = Sketch::new();
        sk.line(Vec2::new(x0, y0), Vec2::new(x1, y0));
        sk.line(Vec2::new(x1, y0), Vec2::new(x1, y1));
        sk.line(Vec2::new(x1, y1), Vec2::new(x0, y1));
        sk.line(Vec2::new(x0, y1), Vec2::new(x0, y0));
        self.tree.add(Feature::Extrude { sketch: sk, height }, None);
    }

    /// Evaluate the model and return a packed `[x,y,z, x,y,z, ...]` vertex
    /// buffer (f32) of the final solid.
    pub fn vertices(&self) -> Vec<f32> {
        let eval = self.tree.evaluate().expect("model must evaluate");
        let mut out = Vec::with_capacity(eval.final_solid.vertex_count() * 3);
        for v in &eval.final_solid.vertices {
            out.push(v.x as f32);
            out.push(v.y as f32);
            out.push(v.z as f32);
        }
        out
    }

    /// Triangle indices (u32 reinterpreted as f32-free `Vec<u32>` via JsValue).
    pub fn indices(&self) -> Vec<u32> {
        let eval = self.tree.evaluate().expect("model must evaluate");
        let mut out = Vec::with_capacity(eval.final_solid.triangle_count() * 3);
        for f in &eval.final_solid.faces {
            out.push(f.a);
            out.push(f.b);
            out.push(f.c);
        }
        out
    }

    /// Approximate volume of the final solid.
    pub fn volume(&self) -> f64 {
        let eval = self.tree.evaluate().expect("model must evaluate");
        eval.final_solid.volume().abs()
    }
}

/// Library entry point: ensure the WASM panic hook is installed.
#[wasm_bindgen(start)]
pub fn start() {
    #[cfg(feature = "wasm")]
    console_error_panic_hook::set_once();
}
