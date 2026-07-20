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
//! - [`slice`] — top-level orchestration tying the above together.

pub mod gcode;
pub mod infill;
pub mod layers;
pub mod offset;
pub mod path;
pub mod profile;
pub mod slice;

pub use profile::{MaterialCalibration, PrinterProfile, SliceSettings};
pub use slice::{slice_solid, SliceResult};
