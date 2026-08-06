//! Closed-loop hardware feedback abstraction (best-effort, design-only).
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! Defines a data-only interface for reading in-printer sensor telemetry
//! (filament width, chamber/hotend temperature) and computing print corrections
//! (flow ratio, temperature offset). No firmware is involved yet — see ADR-0012;
//! the firmware-integration pass is deferred. The [`HardwareFeedback`] trait lets
//! the rest of Vertex stay decoupled from a specific sensor/board, and the
//! [`ClosedLoopController`] turns a reading + target into a [`Correction`].

use crate::PrinterError;

/// A snapshot of in-printer sensor telemetry.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SensorReading {
    /// Measured extruded filament width in mm (vs. the nominal set by the slicer).
    pub filament_width_mm: f64,
    /// Measured chamber/ambient temperature in °C.
    pub chamber_temp_c: f64,
    /// Measured hotend temperature in °C.
    pub hotend_temp_c: f64,
}

/// A correction derived from a [`SensorReading`] to keep the print on target.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Correction {
    /// Multiplier applied to extrusion flow (1.0 = no change).
    pub flow_ratio: f64,
    /// Temperature offset in °C added to the hotend setpoint (0 = no change).
    pub hotend_temp_offset_c: f64,
}

impl Default for Correction {
    fn default() -> Self {
        Correction {
            flow_ratio: 1.0,
            hotend_temp_offset_c: 0.0,
        }
    }
}

/// Source of in-printer sensor telemetry. Implemented by real firmware bridges
/// later; here we provide a [`MockFeedback`] for tests and a pure-computation
/// [`ClosedLoopController`].
pub trait HardwareFeedback {
    /// Read the current sensor telemetry.
    fn read(&self) -> Result<SensorReading, PrinterError>;
}

/// Targets the closed loop tries to hold.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FeedbackTarget {
    /// Nominal filament width the slicer assumed, in mm.
    pub nominal_filament_width_mm: f64,
    /// Desired hotend temperature in °C.
    pub hotend_temp_c: f64,
}

/// A pure, firmware-independent closed-loop controller: compares a reading to a
/// target and produces a [`Correction`]. Proportional, clamped, no integral term
/// (kept simple for the design-only stage).
pub struct ClosedLoopController {
    /// Proportional gain on the filament-width error (per mm of error).
    pub width_gain: f64,
    /// Proportional gain on the hotend-temperature error (per °C of error).
    pub temp_gain: f64,
}

impl Default for ClosedLoopController {
    fn default() -> Self {
        ClosedLoopController {
            width_gain: 0.5,
            temp_gain: 0.1,
        }
    }
}

impl ClosedLoopController {
    /// Compute the correction that nudges the print back toward `target`.
    pub fn correct(&self, reading: &SensorReading, target: &FeedbackTarget) -> Correction {
        let width_err = reading.filament_width_mm - target.nominal_filament_width_mm;
        let temp_err = reading.hotend_temp_c - target.hotend_temp_c;
        // Thin filament (negative error) needs *more* flow, so subtract the error.
        let flow_ratio = (1.0 - self.width_gain * width_err).clamp(0.5, 1.5);
        // Over-temperature (positive error) needs a *lower* setpoint, so negate.
        let hotend_temp_offset_c = (-self.temp_gain * temp_err).clamp(-10.0, 10.0);
        Correction {
            flow_ratio,
            hotend_temp_offset_c,
        }
    }
}

/// In-memory sensor for tests / offline simulation.
#[derive(Debug, Default)]
pub struct MockFeedback {
    /// Reading returned by [`HardwareFeedback::read`].
    pub reading: SensorReading,
}

impl HardwareFeedback for MockFeedback {
    fn read(&self) -> Result<SensorReading, PrinterError> {
        Ok(self.reading)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controller_compensates_thin_filament_with_more_flow() {
        let ctrl = ClosedLoopController::default();
        let target = FeedbackTarget {
            nominal_filament_width_mm: 0.4,
            hotend_temp_c: 210.0,
        };
        let reading = SensorReading {
            filament_width_mm: 0.38,
            chamber_temp_c: 25.0,
            hotend_temp_c: 210.0,
        };
        let c = ctrl.correct(&reading, &target);
        assert!(c.flow_ratio > 1.0, "thin filament should increase flow");
    }

    #[test]
    fn controller_compensates_hot_temp_with_lower_setpoint() {
        let ctrl = ClosedLoopController::default();
        let target = FeedbackTarget {
            nominal_filament_width_mm: 0.4,
            hotend_temp_c: 210.0,
        };
        let reading = SensorReading {
            filament_width_mm: 0.4,
            chamber_temp_c: 25.0,
            hotend_temp_c: 220.0,
        };
        let c = ctrl.correct(&reading, &target);
        assert!(
            c.hotend_temp_offset_c < 0.0,
            "over-temp should lower setpoint"
        );
    }

    #[test]
    fn mock_feedback_returns_configured_reading() {
        let fb = MockFeedback {
            reading: SensorReading {
                filament_width_mm: 0.4,
                chamber_temp_c: 25.0,
                hotend_temp_c: 210.0,
            },
        };
        assert_eq!(fb.read().unwrap().hotend_temp_c, 210.0);
    }
}
