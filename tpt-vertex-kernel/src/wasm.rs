//! WebAssembly bindings (browser use via `wasm-bindgen`).
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! Compiled with `cargo build --target wasm32-unknown-unknown` (or `wasm-pack`).
//! Exposes a small, stable API surface for the frontend to build and evaluate
//! geometry without pulling the whole Rust API into JS. Geometry is exchanged
//! as flat `f32` vertex / `u32` index buffers suitable for WebGPU.
//!
//! A [`Model`] accumulates an in-progress sketch (lines / arcs / circles) that
//! is committed with [`Model::extrude`] or [`Model::revolve`], and supports the
//! real boolean (`union` / `subtract` / `intersect`) and `fillet` / `chamfer`
//! operations from the kernel's CSG engine (ADR-0013).

use crate::feature_tree::{Feature, FeatureTree};
use crate::geometry::edges::{chamfer_edges, fillet_edges};
use crate::geometry::features::{intersect, revolve, subtract, union};
use crate::geometry::sketch::Sketch;
use crate::geometry::solid::Solid;
use crate::math::Vec2;
use wasm_bindgen::prelude::*;

/// A minimal, serializable handle to a feature tree built from JS.
#[wasm_bindgen]
pub struct Model {
    /// The in-progress feature tree (authoritative until a boolean/fillet
    /// bakes the result into `solid`).
    tree: FeatureTree,
    /// A sketch being assembled by `add_line`/`add_circle`/`add_arc`.
    pending: Option<Sketch>,
    /// After a boolean/fillet/chamfer, the result lives here and `tree` is
    /// cleared, so subsequent geometry queries use the baked solid.
    baked: Option<Solid>,
}

#[wasm_bindgen]
impl Model {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Model {
        Model {
            tree: FeatureTree::new(),
            pending: None,
            baked: None,
        }
    }

    /// Add an extruded rectangle (x0,y0)-(x1,y1) of the given height.
    ///
    /// Resets any in-progress sketch and bakes the previous solid, so each
    /// `add_box` begins a fresh base body.
    pub fn add_box(&mut self, x0: f64, y0: f64, x1: f64, y1: f64, height: f64) {
        self.baked = None;
        self.tree = FeatureTree::new();
        let mut sk = Sketch::new();
        sk.line(Vec2::new(x0, y0), Vec2::new(x1, y0));
        sk.line(Vec2::new(x1, y0), Vec2::new(x1, y1));
        sk.line(Vec2::new(x1, y1), Vec2::new(x0, y1));
        sk.line(Vec2::new(x0, y1), Vec2::new(x0, y0));
        self.tree.add(Feature::Extrude { sketch: sk, height }, None);
    }

    /// Append a line segment to the in-progress sketch.
    pub fn add_line(&mut self, x0: f64, y0: f64, x1: f64, y1: f64) {
        self.pending.get_or_insert_with(Sketch::new).line(
            Vec2::new(x0, y0),
            Vec2::new(x1, y1),
        );
    }

    /// Append a full circle to the in-progress sketch.
    pub fn add_circle(&mut self, cx: f64, cy: f64, r: f64) {
        self.pending
            .get_or_insert_with(Sketch::new)
            .circle(Vec2::new(cx, cy), r);
    }

    /// Append an arc (start -> end about center, counter-clockwise when `ccw`)
    /// to the in-progress sketch.
    pub fn add_arc(
        &mut self,
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
        cx: f64,
        cy: f64,
        ccw: bool,
    ) {
        self.pending.get_or_insert_with(Sketch::new).arc(
            Vec2::new(x0, y0),
            Vec2::new(x1, y1),
            Vec2::new(cx, cy),
            ccw,
        );
    }

    /// Commit the in-progress sketch as an extruded feature of `height`.
    pub fn extrude(&mut self, height: f64) {
        if let Some(sk) = self.pending.take() {
            self.baked = None;
            self.tree.add(Feature::Extrude { sketch: sk, height }, None);
        }
    }

    /// Commit the in-progress sketch as a solid of revolution by `angle_deg`.
    pub fn revolve(&mut self, angle_deg: f64) {
        if let Some(sk) = self.pending.take() {
            self.baked = None;
            let angle = angle_deg.to_radians();
            let solid = revolve(&sk, angle, 64);
            self.tree = FeatureTree::new();
            self.baked = Some(solid);
        }
    }

    /// Boolean union of this model with `other`.
    pub fn union(&mut self, other: &Model) {
        let a = self.current();
        let b = other.current();
        self.set_baked(union(&a, &b));
    }

    /// Boolean subtract `other` from this model.
    pub fn subtract(&mut self, other: &Model) {
        let a = self.current();
        let b = other.current();
        self.set_baked(subtract(&a, &b));
    }

    /// Boolean intersection of this model with `other`.
    pub fn intersect(&mut self, other: &Model) {
        let a = self.current();
        let b = other.current();
        self.set_baked(intersect(&a, &b));
    }

    /// Fillet (round) every roundable edge of this model with `radius`.
    pub fn fillet(&mut self, radius: f64) {
        let a = self.current();
        self.set_baked(fillet_edges(&a, radius, &[]));
    }

    /// Chamfer (bevel) every roundable edge of this model by `distance`.
    pub fn chamfer(&mut self, distance: f64) {
        let a = self.current();
        self.set_baked(chamfer_edges(&a, distance, &[]));
    }

    /// Evaluate the model and return a packed `[x,y,z, x,y,z, ...]` vertex
    /// buffer (f32) of the final solid.
    pub fn vertices(&self) -> Vec<f32> {
        let solid = self.current();
        let mut out = Vec::with_capacity(solid.vertex_count() * 3);
        for v in &solid.vertices {
            out.push(v.x as f32);
            out.push(v.y as f32);
            out.push(v.z as f32);
        }
        out
    }

    /// Triangle indices (`Vec<u32>`) of the final solid.
    pub fn indices(&self) -> Vec<u32> {
        let solid = self.current();
        let mut out = Vec::with_capacity(solid.triangle_count() * 3);
        for f in &solid.faces {
            out.push(f.a);
            out.push(f.b);
            out.push(f.c);
        }
        out
    }

    /// Approximate volume of the final solid.
    pub fn volume(&self) -> f64 {
        self.current().volume().abs()
    }
}

impl Model {
    /// The currently-authoritative solid: the baked result, or the feature
    /// tree's evaluated output.
    fn current(&self) -> Solid {
        if let Some(s) = &self.baked {
            return s.clone();
        }
        self.tree
            .evaluate()
            .map(|e| e.final_solid)
            .unwrap_or_default()
    }

    /// Store a baked solid and discard the feature tree (which is no longer
    /// authoritative after a boolean/fillet).
    fn set_baked(&mut self, solid: Solid) {
        self.baked = Some(solid);
        self.tree = FeatureTree::new();
    }
}

/// Library entry point: ensure the WASM panic hook is installed.
#[wasm_bindgen(start)]
pub fn start() {
    #[cfg(feature = "wasm")]
    console_error_panic_hook::set_once();
}
