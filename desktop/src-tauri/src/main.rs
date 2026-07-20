//! TPT Vertex desktop client (Tauri).
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! Wraps the web frontend in a native window (per ADR-0007) and exposes the
//! Rust geometry kernel to the WebView through Tauri commands, enabling
//! offline-first local evaluation with no server dependency. Native file open/
//! save is provided via the dialog + fs plugins; auto-update via the updater
//! plugin.

#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use serde::{Deserialize, Serialize};

use tpt_vertex_kernel::feature_tree::{Feature, FeatureTree};
use tpt_vertex_kernel::geometry::sketch::Sketch;
use tpt_vertex_kernel::geometry::solid::Solid;
use tpt_vertex_kernel::math::Vec2;

/// A minimal serializable description of a model coming from the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSpec {
    /// Rectangular base sketch `[x0, y0, x1, y1]`.
    pub rect: [f64; 4],
    /// Extrusion height.
    pub height: f64,
}

impl ModelSpec {
    /// Build the kernel solid for this spec (shared by all commands).
    fn to_solid(&self) -> Solid {
        let [x0, y0, x1, y1] = self.rect;
        let mut s = Sketch::new();
        s.line(Vec2::new(x0, y0), Vec2::new(x1, y0));
        s.line(Vec2::new(x1, y0), Vec2::new(x1, y1));
        s.line(Vec2::new(x1, y1), Vec2::new(x0, y1));
        s.line(Vec2::new(x0, y1), Vec2::new(x0, y0));
        let mut tree = FeatureTree::new();
        tree.add(
            Feature::Extrude {
                sketch: s,
                height: self.height,
            },
            None,
        );
        tree.evaluate().map(|e| e.final_solid).unwrap_or_default()
    }
}

/// Evaluated mesh handed back to the WebView renderer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshData {
    pub positions: Vec<f32>,
    pub indices: Vec<u32>,
    pub volume: f64,
}

/// Build a feature tree from a spec and evaluate it into a render mesh.
///
/// This runs entirely in-process (offline-first): the desktop app never needs a
/// server to evaluate geometry.
fn evaluate_spec(spec: &ModelSpec) -> MeshData {
    let solid = spec.to_solid();

    let mut positions = Vec::with_capacity(solid.vertices.len() * 3);
    for v in &solid.vertices {
        positions.push(v.x as f32);
        positions.push(v.y as f32);
        positions.push(v.z as f32);
    }
    let mut indices = Vec::with_capacity(solid.faces.len() * 3);
    for f in &solid.faces {
        indices.push(f.a);
        indices.push(f.b);
        indices.push(f.c);
    }
    MeshData {
        positions,
        indices,
        volume: solid.volume().abs(),
    }
}

/// Tauri command: evaluate a model spec locally and return the render mesh.
#[tauri::command]
fn evaluate_model(spec: ModelSpec) -> MeshData {
    evaluate_spec(&spec)
}

/// Tauri command: export the given spec to STEP text (offline).
#[tauri::command]
fn export_step_text(spec: ModelSpec, name: String) -> Result<String, String> {
    let solid = spec.to_solid();
    let mut buf: Vec<u8> = Vec::new();
    tpt_vertex_manufacturing::export_step(&mut buf, &solid, &name).map_err(|e| e.to_string())?;
    String::from_utf8(buf).map_err(|e| e.to_string())
}

/// Slice settings coming from the frontend slicer panel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SliceSpec {
    pub layer_height: f64,
    pub first_layer_height: f64,
    pub wall_count: usize,
    pub infill_density: f64,
    /// Material label (used for calibration lookup); e.g. "PLA", "ABS".
    pub material: String,
    /// Nozzle diameter in mm.
    pub nozzle_diameter: f64,
}

impl Default for SliceSpec {
    fn default() -> Self {
        SliceSpec {
            layer_height: 0.2,
            first_layer_height: 0.24,
            wall_count: 2,
            infill_density: 0.2,
            material: "PLA".to_string(),
            nozzle_diameter: 0.4,
        }
    }
}

/// A lightweight layer preview handed back to the frontend for rendering.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerPreview {
    pub z: f64,
    /// Flattened perimeter polylines: each polyline is a `Vec<[x, y]>`.
    pub perimeters: Vec<Vec<[f64; 2]>>,
    /// Flattened infill segments as `[x0, y0, x1, y1]`.
    pub infill: Vec<[f64; 4]>,
}

/// Result of a local slice: G-code plus a per-layer preview and metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SliceOutput {
    pub gcode: String,
    pub layer_count: usize,
    pub estimated_filament_mm: f64,
    pub estimated_time_s: f64,
    pub layers: Vec<LayerPreview>,
}

/// Build a slice output (extracted for testability, no Tauri types).
fn slice_spec(model: &ModelSpec, slice: &SliceSpec) -> SliceOutput {
    use tpt_vertex_slicer::path::Move;
    use tpt_vertex_slicer::profile::{MaterialCalibration, PrinterProfile, SliceSettings};
    use tpt_vertex_slicer::slice::slice_solid_to_gcode;

    let solid = model.to_solid();

    let printer = PrinterProfile {
        nozzle_diameter: slice.nozzle_diameter,
        ..PrinterProfile::default()
    };
    let settings = SliceSettings {
        layer_height: slice.layer_height,
        first_layer_height: slice.first_layer_height,
        wall_count: slice.wall_count,
        infill_density: slice.infill_density,
        ..SliceSettings::default()
    };
    let material = MaterialCalibration {
        name: slice.material.clone(),
        ..MaterialCalibration::default()
    };

    let res = slice_solid_to_gcode(&solid, &printer, &settings, &material, None);

    let mut layers = Vec::with_capacity(res.layers.len());
    for plan in &res.layers {
        let mut perimeters = Vec::new();
        let mut infill = Vec::new();
        for m in &plan.moves {
            if let Move::Extrude { path, .. } = m {
                if path.closed {
                    perimeters.push(path.points.iter().map(|p| [p.x, p.y]).collect());
                } else if path.points.len() >= 2 {
                    let a = path.points[0];
                    let b = path.points[path.points.len() - 1];
                    infill.push([a.x, a.y, b.x, b.y]);
                }
            }
        }
        layers.push(LayerPreview {
            z: plan.z,
            perimeters,
            infill,
        });
    }

    SliceOutput {
        gcode: res.gcode.text,
        layer_count: res.gcode.layer_count,
        estimated_filament_mm: res.gcode.estimated_filament_mm,
        estimated_time_s: res.gcode.estimated_time_s,
        layers,
    }
}

/// Tauri command: slice a model locally (offline) into G-code + layer preview.
#[tauri::command]
fn slice_model(spec: ModelSpec, slice: SliceSpec) -> SliceOutput {
    slice_spec(&spec, &slice)
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            evaluate_model,
            export_step_text,
            slice_model
        ])
        .run(tauri::generate_context!())
        .expect("error while running TPT Vertex desktop");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluate_spec_produces_mesh_and_volume() {
        let spec = ModelSpec {
            rect: [0.0, 0.0, 2.0, 3.0],
            height: 4.0,
        };
        let mesh = evaluate_spec(&spec);
        assert!(!mesh.positions.is_empty());
        assert_eq!(mesh.indices.len() % 3, 0);
        // 2 x 3 rectangle extruded 4 => volume 24.
        assert!((mesh.volume - 24.0).abs() < 1e-6);
    }

    #[test]
    fn slice_spec_produces_gcode_and_layers() {
        let model = ModelSpec {
            rect: [0.0, 0.0, 10.0, 10.0],
            height: 5.0,
        };
        let out = slice_spec(&model, &SliceSpec::default());
        assert!(out.layer_count > 10, "layers {}", out.layer_count);
        assert!(out.gcode.contains("G1 X"));
        assert!(out.estimated_filament_mm > 0.0);
        assert_eq!(out.layers.len(), out.layer_count);
        // Each layer should have at least one perimeter polyline.
        assert!(out.layers.iter().all(|l| !l.perimeters.is_empty()));
    }
}
