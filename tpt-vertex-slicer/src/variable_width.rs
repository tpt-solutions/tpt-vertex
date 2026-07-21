//! Basic Arachne-style variable-width perimeter fill: when the innermost
//! fixed-width wall can't fit another full-width loop but a residual sliver
//! of material remains, fill that sliver with a single centreline pass sized
//! to the actual local gap instead of leaving it unprinted.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! This is a uniform-width approximation for the whole residual region, not
//! full Arachne (which solves a continuously variable width, point by point,
//! from the shape's medial axis / straight skeleton). A true per-point
//! variable-width solver is a further fast-follow.

use crate::layers::Contour;
use crate::offset::offset_contour;

/// Tunables for thin-wall fill.
#[derive(Debug, Clone, PartialEq)]
pub struct VariableWidthSettings {
    /// Minimum residual gap, as a fraction of the nominal extrusion width,
    /// worth printing at all (smaller slivers are dropped rather than
    /// printed as a near-zero-width, likely-clogging line).
    pub min_residual_fraction: f64,
}

impl Default for VariableWidthSettings {
    fn default() -> Self {
        VariableWidthSettings {
            min_residual_fraction: 0.15,
        }
    }
}

/// A single perimeter path with a caller-chosen (non-default) extrusion width.
#[derive(Debug, Clone, PartialEq)]
pub struct VariableWidthWall {
    pub path: Contour,
    pub width: f64,
}

/// Attempt to fill the residual gap left after the innermost successfully
/// placed fixed-width wall with a single centreline pass.
///
/// `cur` is the CCW-oriented source contour (the same one walls were offset
/// from); `last_wall_distance` is the inset distance, from `cur`, of the
/// innermost wall that was actually placed (`0.0` if none were). Returns
/// `None` if there's no meaningful residual to fill.
pub fn thin_wall_fill(
    cur: &Contour,
    last_wall_distance: f64,
    width: f64,
    settings: &VariableWidthSettings,
) -> Option<VariableWidthWall> {
    let start = last_wall_distance.max(0.0);
    let area_at = |d: f64| offset_contour(cur, -d).signed_area().abs();

    // This hand-rolled offset (see `offset.rs`) does not clip self-
    // intersections: past the shape's true medial axis, the offset polygon
    // does not reliably shrink to nothing and stay degenerate — so the only
    // robust signal for "we've reached the middle" is the offset area
    // ceasing to shrink as the inset distance grows. March forward in small
    // steps and stop at the first such point.
    let step = (width * 0.05).max(1e-4);
    let mut d = start;
    let mut prev_area = area_at(d);
    loop {
        let next_d = d + step;
        let next_area = area_at(next_d);
        if next_area >= prev_area || next_area < 1e-9 {
            break;
        }
        d = next_d;
        prev_area = next_area;
    }

    let residual = 2.0 * (d - start);
    if residual < width * settings.min_residual_fraction {
        return None;
    }

    let center_d = (start + d) / 2.0;
    let path = offset_contour(cur, -center_d);
    if path.points.len() < 3 {
        return None;
    }

    Some(VariableWidthWall {
        path,
        width: residual.clamp(width * 0.3, width * 2.0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layers::P2;
    use crate::offset::walls_for;

    fn square(s: f64) -> Contour {
        Contour {
            points: vec![
                P2::new(-s, -s),
                P2::new(s, -s),
                P2::new(s, s),
                P2::new(-s, s),
            ],
        }
    }

    #[test]
    fn thin_square_gets_a_centerline_fill() {
        // 3mm half-width square (6mm across) with a 0.48mm nominal width and
        // 3 requested walls: only ~2 fit before collapsing, leaving a
        // fillable sliver in the middle.
        let width = 0.48;
        let c = square(1.0);
        let walls = walls_for(&c, 3, width);
        assert!(walls.len() < 3, "expected the walls to collapse before 3 fit");
        let last_d = width * (walls.len() as f64 - 0.5).max(0.0);
        let fill = thin_wall_fill(&c, last_d, width, &VariableWidthSettings::default());
        assert!(fill.is_some(), "expected a thin-wall fill for the residual sliver");
        let fill = fill.unwrap();
        assert!(fill.width > 0.0);
        assert!(fill.path.points.len() >= 3);
    }

    #[test]
    fn already_at_the_medial_axis_needs_no_further_fill() {
        // Starting the search right at the shape's true center leaves no
        // meaningful residual to fill.
        let width = 0.4;
        let c = square(1.0);
        let fill = thin_wall_fill(&c, 1.0, width, &VariableWidthSettings::default());
        assert!(fill.is_none());
    }

    #[test]
    fn large_region_reports_its_full_remaining_half_width() {
        // thin_wall_fill only measures the residual from a given starting
        // distance out to the shape's medial axis — callers are expected to
        // only invoke it once normal wall generation has actually collapsed
        // early (see `slice.rs`); on its own terms, a large remaining region
        // is still a legitimate (if large) residual.
        let width = 0.4;
        let c = square(20.0);
        let fill = thin_wall_fill(&c, width * 0.5, width, &VariableWidthSettings::default());
        assert!(fill.is_some());
    }
}
