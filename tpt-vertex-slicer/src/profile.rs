//! Printer, slice, and material-calibration profiles for the slicer.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0

/// A configurable FDM printer profile (dimensions, kinematics, temperatures).
#[derive(Debug, Clone, PartialEq)]
pub struct PrinterProfile {
    /// Human-readable printer name.
    pub name: String,
    /// Build volume extents in millimetres (X, Y, Z).
    pub bed_size: [f64; 3],
    /// Nozzle diameter in millimetres.
    pub nozzle_diameter: f64,
    /// Default extrusion width as a multiple of nozzle diameter.
    pub extrusion_width_factor: f64,
    /// Hotend temperature in °C for the default material.
    pub nozzle_temperature: f64,
    /// Heated-bed temperature in °C (0 if no heated bed).
    pub bed_temperature: f64,
    /// Maximum travel speed in mm/s.
    pub travel_speed: f64,
    /// Maximum print (extrusion) speed in mm/s.
    pub print_speed: f64,
    /// Maximum volumetric flow in mm³/s (caps effective extrusion).
    pub max_volumetric_flow: f64,
    /// Filament diameter in millimetres (typically 1.75).
    pub filament_diameter: f64,
    /// Retraction distance in millimetres on travel moves.
    pub retraction_length: f64,
    /// Retraction speed in mm/s.
    pub retraction_speed: f64,
    /// Z-hop height in millimetres applied during travel.
    pub z_hop: f64,
}

impl Default for PrinterProfile {
    fn default() -> Self {
        PrinterProfile {
            name: "Generic FDM".to_string(),
            bed_size: [220.0, 220.0, 250.0],
            nozzle_diameter: 0.4,
            extrusion_width_factor: 1.2,
            nozzle_temperature: 210.0,
            bed_temperature: 60.0,
            travel_speed: 150.0,
            print_speed: 60.0,
            max_volumetric_flow: 15.0,
            filament_diameter: 1.75,
            retraction_length: 1.0,
            retraction_speed: 45.0,
            z_hop: 0.2,
        }
    }
}

impl PrinterProfile {
    /// Effective extrusion width in millimetres.
    pub fn extrusion_width(&self) -> f64 {
        self.nozzle_diameter * self.extrusion_width_factor
    }

    /// Cross-sectional area of the filament in mm².
    pub fn filament_area(&self) -> f64 {
        let r = self.filament_diameter / 2.0;
        std::f64::consts::PI * r * r
    }
}

/// Per-material calibration overrides (per-spool tuning).
#[derive(Debug, Clone, PartialEq)]
pub struct MaterialCalibration {
    /// Material label (e.g. "PLA", "ABS").
    pub name: String,
    /// Linear flow ratio applied to all extrusion lengths (1.0 = as-designed).
    pub flow_ratio: f64,
    /// Temperature offset in °C added to the printer's base nozzle temperature.
    pub temperature_offset: f64,
    /// Bed temperature offset in °C.
    pub bed_temperature_offset: f64,
    /// Fan speed as a fraction 0..=1 at the cooling plateau.
    pub cooling_fraction: f64,
    /// Cooling ramp (seconds) at the start of each layer.
    pub cooling_ramp_s: f64,
    /// Linear thermal-expansion coefficient (1/°C) used for shrink compensation
    /// in XY (1.0 means no compensation).
    pub shrink_compensation: f64,
}

impl Default for MaterialCalibration {
    fn default() -> Self {
        MaterialCalibration {
            name: "PLA".to_string(),
            flow_ratio: 1.0,
            temperature_offset: 0.0,
            bed_temperature_offset: 0.0,
            cooling_fraction: 1.0,
            cooling_ramp_s: 2.0,
            shrink_compensation: 1.0,
        }
    }
}

impl MaterialCalibration {
    /// Effective nozzle temperature after calibration offset.
    pub fn nozzle_temperature(&self, base: f64) -> f64 {
        base + self.temperature_offset
    }

    /// Effective bed temperature after calibration offset.
    pub fn bed_temperature(&self, base: f64) -> f64 {
        base + self.bed_temperature_offset
    }

    /// Scale a computed extrusion length by the flow ratio.
    pub fn scale_extrusion(&self, e: f64) -> f64 {
        e * self.flow_ratio
    }
}

/// Structural classification applied to solid regions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyRole {
    /// Load-bearing / functional geometry — denser walls/infill.
    Structural,
    /// Cosmetic or non-critical geometry — can be lighter.
    NonStructural,
}

impl BodyRole {
    /// Per-region wall/infill overrides for this role.
    pub fn wall_count(&self, default: usize) -> usize {
        match self {
            BodyRole::Structural => default.max(3),
            BodyRole::NonStructural => default.min(2).max(1),
        }
    }

    /// Per-region infill density multiplier.
    pub fn infill_scale(&self, default: f64) -> f64 {
        match self {
            BodyRole::Structural => (default * 1.5).min(1.0),
            BodyRole::NonStructural => (default * 0.6).clamp(0.0, 1.0),
        }
    }
}

/// Slice settings (geometry of the toolpath, independent of hardware).
#[derive(Debug, Clone, PartialEq)]
pub struct SliceSettings {
    /// Layer height in millimetres.
    pub layer_height: f64,
    /// Initial (first-layer) height in millimetres.
    pub first_layer_height: f64,
    /// Number of perimeter/wall loops per region.
    pub wall_count: usize,
    /// Infill density as a fraction 0..=1.
    pub infill_density: f64,
    /// Infill line spacing as a fraction of the extrusion width (1.0 = solid).
    pub infill_line_spacing_factor: f64,
    /// Whether to generate a rectilinear (`false`) or zigzag (`true`) infill.
    pub zigzag_infill: bool,
    /// Z-gap (in layers) left between infill and top/bottom surfaces.
    pub top_bottom_layers: usize,
    /// Default body role applied to all regions when no tagging is supplied.
    pub default_role: BodyRole,
}

impl Default for SliceSettings {
    fn default() -> Self {
        SliceSettings {
            layer_height: 0.2,
            first_layer_height: 0.24,
            wall_count: 2,
            infill_density: 0.2,
            infill_line_spacing_factor: 1.0,
            zigzag_infill: false,
            top_bottom_layers: 3,
            default_role: BodyRole::Structural,
        }
    }
}
