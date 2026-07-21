//! FDM 3D-printing slicer for TPT Vertex.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! Turns kernel [`Solid`](tpt_vertex_kernel::geometry::solid::Solid) meshes into
//! printable G-code: planar layering, perimeter/wall generation via polygon
//! offsetting, rectilinear infill, and toolpath/G-code emission for a
//! configurable FDM printer profile.
//!
//! Module overview:
//! - [`profile`] — printer, slice, and material-calibration settings.
//! - [`layers`] — planar slicing of a triangle mesh into closed contours.
//! - [`offset`] — polygon inset/offset for perimeters and walls.
//! - [`infill`] — rectilinear/zigzag infill generation.
//! - [`path`] — toolpath ordering (perimeters, infill, travel/retraction).
//! - [`gcode`] — G-code emission for a generic configurable FDM profile.
//! - [`support`] — basic overhang-triggered grid/pillar support generation.
//! - [`adaptive`] — adaptive layer height driven by local surface slope.
//! - [`bridging`] — bridge detection for bridge-specific speed/cooling.
//! - [`seam`] — perimeter loop seam (start-point) placement.
//! - [`variable_width`] — basic (uniform-width) thin-wall fill.
//! - [`repair`] — mesh repair/manifold-checking pre-pass.
//! - [`presets`] — Klipper/Marlin-style printer-profile preset import/export.
//! - [`plugin`] — [`tpt_vertex_manufacturing`] `ExporterPlugin` adapter.
//! - [`slice`] — top-level orchestration tying the above together.

pub mod adaptive;
pub mod bridging;
pub mod gcode;
pub mod infill;
pub mod layers;
pub mod offset;
pub mod path;
pub mod plugin;
pub mod presets;
pub mod profile;
pub mod repair;
pub mod seam;
pub mod slice;
pub mod support;
pub mod variable_width;

pub use adaptive::{adaptive_layer_zs, AdaptiveLayerSettings};
pub use bridging::BridgeSettings;
pub use profile::{
    BodyRole, ExtruderProfile, MaterialCalibration, PrinterProfile, RegionTag, SliceSettings,
};
pub use repair::{repair_mesh, RepairReport};
pub use seam::SeamMode;
pub use slice::{slice_solid, SliceResult};
pub use support::{generate_supports, SupportSettings};
pub use variable_width::VariableWidthSettings;
