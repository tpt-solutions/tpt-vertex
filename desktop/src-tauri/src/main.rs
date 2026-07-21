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

mod printer;

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

// --- Static FEA & motion (tpt-vertex-simulation) ----------------------------

/// Static-analysis request coming from the frontend simulation panel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisRequest {
    /// Material label (looked up in the kernel's material table; falls back to
    /// generic plastic).
    pub material: String,
    /// Indices of mesh nodes to fully fix (in the returned `nodes` array).
    pub fixed_nodes: Vec<usize>,
    /// Point loads: `(node_index, fx, fy, fz)` in N.
    pub loads: Vec<(usize, f64, f64, f64)>,
    /// Target tetrahedral edge length in mm.
    pub max_tet_edge: f64,
}

/// Result of a local static analysis: the volume mesh plus per-element von
/// Mises stress, ready for color-mapped rendering in the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResponse {
    /// Mesh node positions `[x, y, z]` flattened.
    pub nodes: Vec<f64>,
    /// Tetrahedron connectivity: each tet is 4 node indices.
    pub tets: Vec<usize>,
    /// Per-element von Mises stress (MPa), one per tet.
    pub von_mises: Vec<f64>,
    /// Max nodal displacement magnitude (mm).
    pub max_displacement: f64,
    /// Max von Mises stress across the model (MPa).
    pub max_von_mises: f64,
}

/// Run a linear-elastic static analysis of the spec's solid, fully offline.
///
/// Extracted into a non-Tauri function for unit testing.
fn run_static_analysis_spec(spec: &ModelSpec, req: &AnalysisRequest) -> AnalysisResponse {
    use tpt_vertex_kernel::geometry::solid::{Face, Solid};
    use tpt_vertex_kernel::material::Material;
    use tpt_vertex_kernel::math::Vec3;
    use tpt_vertex_simulation::bc::{BoundaryCondition, PointLoad};
    use tpt_vertex_simulation::{run_static_analysis, AnalysisSettings};

    // Build a watertight box directly from the spec's rectangular footprint so
    // the mesher has a closed manifold to tetrahedralize. (The kernel's
    // rectangle-extrude currently yields a degenerate surface; tracked
    // separately from the simulation pipeline.)
    let [x0, y0, x1, y1] = spec.rect;
    let h = spec.height.max(1e-6);
    let mut solid = Solid::new();
    let v = |x: f64, y: f64, z: f64| solid.add_vertex(Vec3::new(x, y, z));
    let p = [
        v(x0, y0, 0.0), v(x1, y0, 0.0), v(x1, y1, 0.0), v(x0, y1, 0.0),
        v(x0, y0, h), v(x1, y0, h), v(x1, y1, h), v(x0, y1, h),
    ];
    let f = |a: u32, b: u32, c: u32| solid.faces.push(Face::new(a, b, c));
    f(p[0], p[1], p[2]); f(p[0], p[2], p[3]);
    f(p[4], p[6], p[5]); f(p[4], p[7], p[6]);
    f(p[0], p[5], p[1]); f(p[0], p[4], p[5]);
    f(p[1], p[6], p[2]); f(p[1], p[5], p[6]);
    f(p[2], p[7], p[3]); f(p[2], p[6], p[7]);
    f(p[3], p[4], p[0]); f(p[3], p[7], p[4]);

    let material = Material::from_name(&req.material);
    let mut bc = BoundaryCondition::new();
    for &n in &req.fixed_nodes {
        bc = bc.fix_node(n);
    }
    for &(n, fx, fy, fz) in &req.loads {
        bc = bc.with_load(PointLoad { node: n, fx, fy, fz });
    }
    let settings = AnalysisSettings::new(material, bc, req.max_tet_edge);
    let res = run_static_analysis(&solid, &settings).expect("static analysis");

    let nodes = res
        .mesh
        .nodes
        .iter()
        .flat_map(|n| [n[0], n[1], n[2]])
        .collect();
    let tets = res.mesh.tets.iter().flat_map(|t| t.to_vec()).collect();

    AnalysisResponse {
        nodes,
        tets,
        von_mises: res.von_mises,
        max_displacement: res.max_displacement,
        max_von_mises: res.max_von_mises,
    }
}

/// Tauri command: run a local static-stress analysis.
#[tauri::command]
fn run_static_analysis(spec: ModelSpec, req: AnalysisRequest) -> AnalysisResponse {
    run_static_analysis_spec(&spec, &req)
}

/// Motion-frame request: drive a revolute joint by `angle` (rad) about `axis`
/// through `anchor`, and return the resulting part poses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MotionRequest {
    /// Rotation axis (need not be normalized; normalized internally).
    pub axis: [f64; 3],
    /// Rotation angle in radians.
    pub angle: f64,
    /// Pivot/anchor point of the joint.
    pub anchor: [f64; 3],
}

/// A single part's pose in assembly space.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartPose {
    pub name: String,
    pub position: [f64; 3],
    /// Unit quaternion `(w, x, y, z)`.
    pub rotation: [f64; 4],
}

/// Motion-frame result: the pose of every part after solving the joint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MotionResponse {
    pub parts: Vec<PartPose>,
}

/// Compute a motion-study frame by driving a revolute joint, fully offline.
#[allow(unused_variables)] // `spec` reserved for future per-model assembly builds
fn run_motion_frame_spec(spec: &ModelSpec, req: &MotionRequest) -> MotionResponse {
    use tpt_vertex_kernel::assembly::{Assembly, Mate, Part, PartId};
    use tpt_vertex_kernel::feature_tree::Feature;
    use tpt_vertex_kernel::geometry::sketch::Sketch;
    use tpt_vertex_kernel::math::{Quaternion, Vec2, Vec3};
    use tpt_vertex_simulation::MotionPlayer;

    let block = || {
        let mut s = Sketch::new();
        s.line(Vec2::ZERO, Vec2::new(1.0, 0.0));
        s.line(Vec2::new(1.0, 0.0), Vec2::new(1.0, 1.0));
        s.line(Vec2::new(1.0, 1.0), Vec2::ZERO);
        let mut t = FeatureTree::new();
        t.add(Feature::Extrude { sketch: s, height: 1.0 }, None);
        t
    };

    let mut asm = Assembly::new();
    let base = asm.add_part(Part::new("base", block()));
    let mover = asm.add_part(Part::new("mover", block()));
    let _ = PartId(0);
    let axis = Vec3::new(req.axis[0], req.axis[1], req.axis[2]).normalize();
    let anchor = Vec3::new(req.anchor[0], req.anchor[1], req.anchor[2]);
    asm.add_mate(Mate::Revolute {
        base,
        mover,
        anchor,
        axis,
        angle: req.angle,
        limits: None,
    });

    let mut player = MotionPlayer::new(asm, mover, 0.0, req.angle);
    let solved = player.frame_at(1.0);

    let q_to_arr = |q: Quaternion| [q.w, q.x, q.y, q.z];
    let parts = solved
        .parts()
        .iter()
        .map(|(id, p)| {
            let _ = id;
            PartPose {
                name: p.name.clone(),
                position: [p.transform.translation.x, p.transform.translation.y, p.transform.translation.z],
                rotation: q_to_arr(p.transform.rotation),
            }
        })
        .collect();

    MotionResponse { parts }
}

/// Tauri command: compute a motion-study frame from a driven revolute joint.
#[tauri::command]
fn run_motion_frame(spec: ModelSpec, req: MotionRequest) -> MotionResponse {
    run_motion_frame_spec(&spec, &req)
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_store::Builder::default().build())
        .invoke_handler(tauri::generate_handler![
            evaluate_model,
            export_step_text,
            slice_model,
            run_static_analysis,
            run_motion_frame,
            printer::list_printers,
            printer::save_printer,
            printer::delete_printer,
            printer::test_printer,
            printer::send_to_printer,
            printer::printer_status
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

    #[test]
    fn static_analysis_returns_mesh_and_stress() {
        let model = ModelSpec {
            rect: [0.0, 0.0, 2.0, 2.0],
            height: 2.0,
        };
        // Fix nothing complex: just make sure the pipeline runs and returns a
        // volume mesh with matching stress/connectivity lengths.
        let req = AnalysisRequest {
            material: "Steel".to_string(),
            fixed_nodes: vec![],
            loads: vec![], // unloaded => zero stress, but pipeline must succeed
            max_tet_edge: 0.5,
        };
        let res = run_static_analysis_spec(&model, &req);
        assert_eq!(res.nodes.len() % 3, 0);
        let node_count = res.nodes.len() / 3;
        assert_eq!(res.tets.len() % 4, 0);
        assert_eq!(res.tets.len() / 4, res.von_mises.len());
        assert!(node_count > 0);
        // Unloaded static analysis => no stress, finite results.
        assert!(res.max_von_mises.abs() < 1e-9);
    }

    #[test]
    fn motion_frame_rotates_mover() {
        let model = ModelSpec {
            rect: [0.0, 0.0, 1.0, 1.0],
            height: 1.0,
        };
        let req = MotionRequest {
            axis: [0.0, 0.0, 1.0],
            angle: std::f64::consts::FRAC_PI_2,
            anchor: [0.0, 0.0, 0.0],
        };
        let res = run_motion_frame_spec(&model, &req);
        assert_eq!(res.parts.len(), 2);
        // The mover's rotation quaternion should be a +90° rotation about Z.
        let mover = res.parts.iter().find(|p| p.name == "mover").unwrap();
        let expected = (std::f64::consts::FRAC_PI_2 / 2.0).cos(); // w = cos(θ/2)
        assert!((mover.rotation[0] - expected).abs() < 1e-9, "w {}", mover.rotation[0]);
        assert!((mover.rotation[3] - (std::f64::consts::FRAC_PI_2 / 2.0).sin()).abs() < 1e-9);
    }
}
