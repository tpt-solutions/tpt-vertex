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
use tpt_vertex_kernel::math::Vec2;

/// A minimal serializable description of a model coming from the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSpec {
    /// Rectangular base sketch `[x0, y0, x1, y1]`.
    pub rect: [f64; 4],
    /// Extrusion height.
    pub height: f64,
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
    let [x0, y0, x1, y1] = spec.rect;
    let mut s = Sketch::new();
    s.line(Vec2::new(x0, y0), Vec2::new(x1, y0));
    s.line(Vec2::new(x1, y0), Vec2::new(x1, y1));
    s.line(Vec2::new(x1, y1), Vec2::new(x0, y1));
    s.line(Vec2::new(x0, y1), Vec2::new(x0, y0));

    let mut tree = FeatureTree::new();
    tree.add(
        Feature::Extrude {
            sketch: s,
            height: spec.height,
        },
        None,
    );
    let solid = tree.evaluate().map(|e| e.final_solid).unwrap_or_default();

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
    let [x0, y0, x1, y1] = spec.rect;
    let mut s = Sketch::new();
    s.line(Vec2::new(x0, y0), Vec2::new(x1, y0));
    s.line(Vec2::new(x1, y0), Vec2::new(x1, y1));
    s.line(Vec2::new(x1, y1), Vec2::new(x0, y1));
    s.line(Vec2::new(x0, y1), Vec2::new(x0, y0));
    let mut tree = FeatureTree::new();
    tree.add(Feature::Extrude { sketch: s, height: spec.height }, None);
    let solid = tree.evaluate().map(|e| e.final_solid).unwrap_or_default();

    let mut buf: Vec<u8> = Vec::new();
    tpt_vertex_manufacturing::export_step(&mut buf, &solid, &name)
        .map_err(|e| e.to_string())?;
    String::from_utf8(buf).map_err(|e| e.to_string())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![evaluate_model, export_step_text])
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
}
