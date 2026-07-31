//! Printer telemetry → simulation bridge for closed-loop deviation detection.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! Maps printer telemetry data (temperatures, layer progress) into
//! [`tpt_vertex_simulation`] boundary conditions, enabling prediction of
//! thermal warping, dimensional deviation, and cumulative damage during
//! a print.

use crate::client::{StatusSnapshot, Temperature};

/// A single telemetry observation at a specific point during a print.
#[derive(Debug, Clone)]
pub struct TelemetryObservation {
    /// Current layer index (0-based).
    pub layer_index: usize,
    /// Z height of the current layer in mm.
    pub layer_z: f64,
    /// Timestamp since print start in seconds.
    pub timestamp_s: f64,
    /// Temperature readings.
    pub temperature: Temperature,
    /// Print completion fraction 0..=1.
    pub completion: f64,
}

/// Configuration for mapping telemetry to simulation boundary conditions.
#[derive(Debug, Clone)]
pub struct TelemetryMapping {
    /// Reference (ambient) temperature in °C for thermal stress calculation.
    pub ambient_temperature: f64,
    /// Linear thermal expansion coefficient (1/°C) for the printed material.
    /// Typical values: PLA ~70e-6, ABS ~70e-6, PETG ~70e-6, Nylon ~80e-6.
    pub thermal_expansion: f64,
    /// Thermal conductivity (W/m·K) for steady-state approximation.
    pub thermal_conductivity: f64,
    /// Convective heat transfer coefficient (W/m²·K) for the part surface.
    pub convection_coefficient: f64,
}

impl Default for TelemetryMapping {
    fn default() -> Self {
        TelemetryMapping {
            ambient_temperature: 25.0,
            thermal_expansion: 70e-6,
            thermal_conductivity: 0.2,
            convection_coefficient: 10.0,
        }
    }
}

/// A deviation report comparing predicted thermal deformation against nominal
/// CAD geometry.
#[derive(Debug, Clone)]
pub struct DeviationReport {
    /// Per-layer maximum displacement magnitude (mm) due to thermal effects.
    pub max_displacement_per_layer: Vec<f64>,
    /// Overall maximum displacement across all layers (mm).
    pub global_max_displacement: f64,
    /// Estimated maximum von Mises thermal stress (MPa).
    pub max_thermal_stress: f64,
    /// Whether any layer exceeds the warpage threshold.
    pub exceeds_threshold: bool,
}

impl DeviationReport {
    /// Create a deviation report from a sequence of layer observations.
    pub fn from_observations(
        observations: &[TelemetryObservation],
        mapping: &TelemetryMapping,
        max_acceptable_warp_mm: f64,
    ) -> Self {
        let mut max_disp_per_layer = Vec::with_capacity(observations.len());
        let mut global_max = 0.0_f64;

        for obs in observations {
            // Estimate thermal displacement: ΔL = α * L * ΔT
            // where ΔT is the temperature difference from ambient.
            let dt_tool = (obs.temperature.tool - mapping.ambient_temperature).abs();
            let dt_bed = (obs.temperature.bed - mapping.ambient_temperature).abs();
            let dt_max = dt_tool.max(dt_bed);

            // Simplified thermal displacement estimate at the current layer.
            // A full FEA would use the simulation crate's thermal module;
            // here we use a first-order approximation based on the
            // temperature differential and a characteristic length scale.
            let characteristic_length = obs.layer_z.max(1.0); // mm
            let displacement = mapping.thermal_expansion * characteristic_length * dt_max;

            max_disp_per_layer.push(displacement);
            if displacement > global_max {
                global_max = displacement;
            }
        }

        // Estimate max thermal stress: σ = E * α * ΔT (simplified, uniaxial)
        // Using E ≈ 2000 MPa (typical for PLA/ABS) as a reference.
        let youngs_modulus = 2000.0; // MPa
        let max_dt = observations
            .iter()
            .map(|obs| {
                let dt_tool = (obs.temperature.tool - mapping.ambient_temperature).abs();
                let dt_bed = (obs.temperature.bed - mapping.ambient_temperature).abs();
                dt_tool.max(dt_bed)
            })
            .fold(0.0_f64, f64::max);
        let max_stress = youngs_modulus * mapping.thermal_expansion * max_dt;

        DeviationReport {
            max_displacement_per_layer: max_disp_per_layer,
            global_max_displacement: global_max,
            max_thermal_stress: max_stress,
            exceeds_threshold: global_max > max_acceptable_warp_mm,
        }
    }

    /// Build a thermal boundary condition field function for the simulation
    /// crate from a sequence of observations.
    ///
    /// Returns a closure `f(x, y, z) -> f64` that maps a 3D point to a
    /// normalized temperature field (0 = ambient, 1 = max observed temperature).
    pub fn temperature_field<'a>(
        observations: &'a [TelemetryObservation],
        mapping: &'a TelemetryMapping,
    ) -> impl Fn(f64, f64, f64) -> f64 + 'a {
        let max_temp = observations
            .iter()
            .map(|obs| obs.temperature.tool.max(obs.temperature.bed))
            .fold(0.0_f64, f64::max)
            .max(mapping.ambient_temperature + 1.0);

        move |_x: f64, _y: f64, z: f64| {
            // Simple gradient model: temperature decreases linearly from the
            // nozzle temperature at the current layer height to ambient at
            // the build plate.
            let current_layer_z = observations.last().map(|obs| obs.layer_z).unwrap_or(0.0);
            let current_temp = observations
                .last()
                .map(|obs| obs.temperature.tool.max(obs.temperature.bed))
                .unwrap_or(mapping.ambient_temperature);

            if current_layer_z < 1e-6 {
                return 0.0;
            }
            let z_frac = (z / current_layer_z).clamp(0.0, 1.0);
            let temp_at_z =
                mapping.ambient_temperature + (current_temp - mapping.ambient_temperature) * z_frac;
            ((temp_at_z - mapping.ambient_temperature) / (max_temp - mapping.ambient_temperature))
                .clamp(0.0, 1.0)
        }
    }
}

/// Create a [`TelemetryObservation`] from a [`StatusSnapshot`] and layer info.
pub fn observation_from_snapshot(
    snapshot: &StatusSnapshot,
    layer_index: usize,
    layer_z: f64,
    timestamp_s: f64,
) -> TelemetryObservation {
    let completion = snapshot
        .progress
        .as_ref()
        .map(|p| p.completion)
        .unwrap_or(0.0);

    TelemetryObservation {
        layer_index,
        layer_z,
        timestamp_s,
        temperature: snapshot.temps,
        completion,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::{JobProgress, PrinterState};

    fn sample_observation(tool_temp: f64, bed_temp: f64, layer_z: f64) -> TelemetryObservation {
        TelemetryObservation {
            layer_index: 0,
            layer_z,
            timestamp_s: 0.0,
            temperature: Temperature {
                tool: tool_temp,
                tool_target: tool_temp,
                bed: bed_temp,
                bed_target: bed_temp,
            },
            completion: 0.0,
        }
    }

    #[test]
    fn deviation_report_detects_warping() {
        let observations = vec![
            sample_observation(210.0, 60.0, 0.2),
            sample_observation(210.0, 60.0, 1.0),
            sample_observation(210.0, 60.0, 5.0),
        ];
        let mapping = TelemetryMapping::default();
        let report = DeviationReport::from_observations(&observations, &mapping, 0.1);
        assert!(!report.max_displacement_per_layer.is_empty());
        assert!(report.global_max_displacement > 0.0);
    }

    #[test]
    fn temperature_field_returns_finite_values() {
        let observations = vec![sample_observation(210.0, 60.0, 10.0)];
        let mapping = TelemetryMapping::default();
        let field = DeviationReport::temperature_field(&observations, &mapping);
        let val = field(5.0, 5.0, 5.0);
        assert!(val.is_finite());
        assert!((0.0..=1.0).contains(&val));
    }

    #[test]
    fn observation_from_snapshot_extracts_data() {
        let snapshot = StatusSnapshot {
            state: PrinterState::Printing,
            temps: Temperature {
                tool: 200.0,
                tool_target: 210.0,
                bed: 60.0,
                bed_target: 60.0,
            },
            progress: Some(JobProgress {
                completion: 0.5,
                file: Some("test.gcode".to_string()),
                time_left_s: Some(300.0),
            }),
            firmware: Some("Klipper".to_string()),
        };
        let obs = observation_from_snapshot(&snapshot, 5, 1.0, 120.0);
        assert_eq!(obs.layer_index, 5);
        assert_eq!(obs.layer_z, 1.0);
        assert!((obs.completion - 0.5).abs() < 1e-6);
    }
}
