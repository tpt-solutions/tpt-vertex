//! Bridging detection: flag layer regions that span an unsupported gap so
//! they can print at bridge-specific speed and cooling.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! A region is treated as a bridge when most of its sampled interior has no
//! backing material directly below it on the previous layer — as opposed to
//! a plain overhang, where only the edge extends past the layer below (see
//! [`crate::support`], which handles that case with support pillars instead).

use crate::infill::point_in_polygon;
use crate::layers::{Contour, P2};

/// Tunables for bridge detection.
#[derive(Debug, Clone, PartialEq)]
pub struct BridgeSettings {
    /// Fraction (0..=1) of a region's sampled interior that must be
    /// unsupported by the layer directly below for the whole region to be
    /// treated as a bridge.
    pub min_unsupported_fraction: f64,
    /// Spacing between interior sample points, in millimetres.
    pub sample_spacing: f64,
}

impl Default for BridgeSettings {
    fn default() -> Self {
        BridgeSettings {
            min_unsupported_fraction: 0.9,
            sample_spacing: 1.0,
        }
    }
}

/// True if `contour`'s interior is mostly unsupported by `below` (the
/// previous layer's contours), per `settings`. A layer with no `below`
/// contours (e.g. the first layer, resting on the bed) is never a bridge.
pub fn is_bridge(contour: &Contour, below: &[Contour], settings: &BridgeSettings) -> bool {
    if below.is_empty() {
        return false;
    }
    let Some(((minx, miny), (maxx, maxy))) = contour.bbox() else {
        return false;
    };
    let spacing = settings.sample_spacing.max(1e-3);
    let mut total = 0usize;
    let mut unsupported = 0usize;

    let mut x = minx + spacing / 2.0;
    while x <= maxx {
        let mut y = miny + spacing / 2.0;
        while y <= maxy {
            let p = P2::new(x, y);
            if point_in_polygon(contour, p) {
                total += 1;
                if !below.iter().any(|c| point_in_polygon(c, p)) {
                    unsupported += 1;
                }
            }
            y += spacing;
        }
        x += spacing;
    }

    if total == 0 {
        return false;
    }
    (unsupported as f64 / total as f64) >= settings.min_unsupported_fraction
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(cx: f64, cy: f64, half: f64) -> Contour {
        Contour {
            points: vec![
                P2::new(cx - half, cy - half),
                P2::new(cx + half, cy - half),
                P2::new(cx + half, cy + half),
                P2::new(cx - half, cy + half),
            ],
        }
    }

    #[test]
    fn fully_unsupported_span_is_a_bridge() {
        let below = vec![square(-10.0, 0.0, 1.0), square(10.0, 0.0, 1.0)];
        let span = square(0.0, 0.0, 5.0); // spans the gap between the two below pillars
        assert!(is_bridge(&span, &below, &BridgeSettings::default()));
    }

    #[test]
    fn fully_backed_region_is_not_a_bridge() {
        let below = vec![square(0.0, 0.0, 5.0)];
        let region = square(0.0, 0.0, 4.0);
        assert!(!is_bridge(&region, &below, &BridgeSettings::default()));
    }

    #[test]
    fn first_layer_is_never_a_bridge() {
        let region = square(0.0, 0.0, 5.0);
        assert!(!is_bridge(&region, &[], &BridgeSettings::default()));
    }
}
