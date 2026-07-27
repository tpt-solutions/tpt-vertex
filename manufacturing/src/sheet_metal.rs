//! Sheet-metal module: flat-pattern unfolding, bend allowances, bend order.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! A [`SheetPart`] describes a flat pattern derived from a 3D sheet-metal
//! model: the 2D outline, bend lines (hinge lines), bend angles, radii, and
//! a recommended bend-order sequence.  The unbend/unfold algorithm works from
//! the kernel's [`Solid`] by identifying planar faces connected at bend edges.

use tpt_vertex_kernel::geometry::solid::Solid;
use tpt_vertex_kernel::math::Vec3;

/// A 2D point in the flat-pattern coordinate system.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlatPt {
    pub x: f64,
    pub y: f64,
}

impl FlatPt {
    pub fn new(x: f64, y: f64) -> Self {
        FlatPt { x, y }
    }
}

/// A bend line (hinge) in the flat pattern.
#[derive(Debug, Clone, PartialEq)]
pub struct BendLine {
    /// Start of the hinge line in flat-pattern coordinates.
    pub start: FlatPt,
    /// End of the hinge line.
    pub end: FlatPt,
    /// Bend angle in radians (positive = bend up).
    pub angle_rad: f64,
    /// Bend radius in millimetres (inside radius).
    pub radius_mm: f64,
    /// Bend direction: `true` = bend toward the viewer, `false` = away.
    pub bend_up: bool,
}

/// Bend allowance strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BendAllowance {
    /// K-factor method: `ba = angle * (radius + k * thickness)`.
    KFactor,
    /// Bend deduction: subtract a fixed length per bend.
    BendDeduction,
    /// Y-factor (similar to K-factor but with a different constant).
    YFactor,
}

/// Configuration for flat-pattern generation.
#[derive(Debug, Clone, PartialEq)]
pub struct SheetMetalConfig {
    /// Material thickness in millimetres.
    pub thickness: f64,
    /// Bend allowance strategy.
    pub allowance: BendAllowance,
    /// K-factor (0.0–1.0, typically 0.33–0.50 for steel).
    pub k_factor: f64,
    /// Minimum inside bend radius (mm); bends sharper than this are clamped.
    pub min_bend_radius: f64,
}

impl Default for SheetMetalConfig {
    fn default() -> Self {
        SheetMetalConfig {
            thickness: 1.0,
            allowance: BendAllowance::KFactor,
            k_factor: 0.44,
            min_bend_radius: 0.5,
        }
    }
}

/// The output of flat-pattern generation.
#[derive(Debug, Clone, PartialEq)]
pub struct FlatPattern {
    /// 2D outline of the unfolded part (closed polygon).
    pub outline: Vec<FlatPt>,
    /// Bend lines in order.
    pub bend_lines: Vec<BendLine>,
    /// Suggested bend sequence (indices into `bend_lines`).
    pub bend_order: Vec<usize>,
}

/// Compute the bend allowance arc length for a single bend.
pub fn bend_allowance_length(
    angle_rad: f64,
    radius: f64,
    thickness: f64,
    k_factor: f64,
) -> f64 {
    // Neutral-axis arc length: ba = angle * (R + k * T)
    angle_rad.abs() * (radius + k_factor * thickness)
}

/// Generate a flat pattern from a solid (simplified: projects all faces onto
/// the dominant plane and identifies bend edges as edges shared by two faces
/// at a non-180° dihedral angle).
///
/// This is a structural implementation; real sheet-metal unfolding requires
/// face pairing and bend-region identification that depends on the B-rep
/// topology.  The function returns a placeholder for a rectangular blank.
pub fn unfold_solid(solid: &Solid, config: &SheetMetalConfig) -> Option<FlatPattern> {
    let (min, max) = solid.bounds()?;
    let dx = max.x - min.x;
    let dy = max.y - min.y;

    // Placeholder: rectangular outline with no bends for a flat plate.
    let outline = vec![
        FlatPt::new(min.x, min.y),
        FlatPt::new(max.x, min.y),
        FlatPt::new(max.x, max.y),
        FlatPt::new(min.x, max.y),
    ];

    let _ = (dx, dy, config);

    Some(FlatPattern {
        outline,
        bend_lines: Vec::new(),
        bend_order: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_vertex_kernel::geometry::solid::{Face, Solid as KernSolid};
    use tpt_vertex_kernel::math::Vec3;

    fn plate() -> KernSolid {
        let mut s = KernSolid::new();
        let v = |x: f64, y: f64, z: f64| s.add_vertex(Vec3::new(x, y, z));
        let p = [v(0.0, 0.0, 0.0), v(10.0, 0.0, 0.0), v(10.0, 5.0, 0.0), v(0.0, 5.0, 0.0)];
        let mut f = |a: u32, b: u32, c: u32| s.faces.push(Face::new(a, b, c));
        f(p[0], p[1], p[2]); f(p[0], p[2], p[3]);
        s
    }

    #[test]
    fn unfold_plate_produces_outline() {
        let config = SheetMetalConfig::default();
        let fp = unfold_solid(&plate(), &config).unwrap();
        assert_eq!(fp.outline.len(), 4);
        assert!(fp.bend_lines.is_empty());
    }

    #[test]
    fn bend_allowance_kfactor() {
        let ba = bend_allowance_length(std::f64::consts::FRAC_PI_2, 1.0, 2.0, 0.44);
        // angle * (R + k*T) = pi/2 * (1.0 + 0.44*2.0) = pi/2 * 1.88
        let expected = std::f64::consts::FRAC_PI_2 * 1.88;
        assert!((ba - expected).abs() < 1e-10);
    }
}
