//! Basic overhang-triggered support generation.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! This is a first-generation ("basic") support strategy, tracked against the
//! Phase 10 fast-follow list: overhangs are detected on a regular XY sampling
//! grid by comparing each layer's solid footprint against the layer below
//! (grown by the horizontal distance the configured overhang angle allows),
//! and support material is a sparse grid of independent square pillars run
//! from the build plate up to just short of the overhanging surface. This is
//! deliberately not tree/organic supports (a separate, further fast-follow):
//! pillars are simple, independent, and do not merge or branch.

use crate::infill::point_in_polygon;
use crate::layers::{Contour, Layer, P2};
use crate::offset::offset_contour;
use crate::path::ExtrusionPath;

/// Tunables for basic grid/pillar support generation.
#[derive(Debug, Clone, PartialEq)]
pub struct SupportSettings {
    /// Maximum overhang angle, in degrees measured from vertical, that prints
    /// cleanly without support. Steeper (more horizontal) overhangs than this
    /// trigger a support pillar.
    pub overhang_angle_deg: f64,
    /// XY spacing between support pillar sample points, in millimetres.
    pub pillar_spacing: f64,
    /// Half-width of each square support pillar, in millimetres.
    pub pillar_half_width: f64,
    /// Vertical air gap, in layers, left between the top of a support pillar
    /// and the overhanging surface it holds up, for easy break-away removal.
    pub z_gap_layers: usize,
}

impl Default for SupportSettings {
    fn default() -> Self {
        SupportSettings {
            overhang_angle_deg: 45.0,
            pillar_spacing: 3.0,
            pillar_half_width: 0.4,
            z_gap_layers: 1,
        }
    }
}

/// A single support pillar's footprint center at a given layer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SupportPillar {
    pub center: P2,
}

/// Per-layer support toolpath: the pillar footprints that must be printed at
/// this layer's Z.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SupportLayer {
    pub pillars: Vec<SupportPillar>,
}

impl SupportLayer {
    /// Render this layer's pillars as closed square extrusion paths.
    pub fn to_paths(&self, half_width: f64) -> Vec<ExtrusionPath> {
        self.pillars
            .iter()
            .map(|p| pillar_path(p.center, half_width))
            .collect()
    }
}

/// A closed square path centered on `center` with the given half-width.
pub fn pillar_path(center: P2, half_width: f64) -> ExtrusionPath {
    let h = half_width.max(1e-3);
    ExtrusionPath::new(
        vec![
            P2::new(center.x - h, center.y - h),
            P2::new(center.x + h, center.y - h),
            P2::new(center.x + h, center.y + h),
            P2::new(center.x - h, center.y + h),
            P2::new(center.x - h, center.y - h),
        ],
        true,
    )
}

/// Compute per-layer support pillar placements for a stack of slice `layers`
/// (bottom-to-top, as produced by [`crate::layers::slice_solid`]). Returns one
/// [`SupportLayer`] per input layer (same length/order), listing pillars that
/// should be printed as support material at that layer.
pub fn generate_supports(layers: &[Layer], settings: &SupportSettings) -> Vec<SupportLayer> {
    let n = layers.len();
    let mut result = vec![SupportLayer::default(); n];
    let Some(grid) = sample_grid(layers, settings.pillar_spacing) else {
        return result;
    };

    // Horizontal distance a layer may overhang beyond the (grown) layer below
    // without needing support, derived from the allowed overhang angle
    // (measured from vertical): allowance = layer_height * tan(angle).
    let tan_angle = settings.overhang_angle_deg.to_radians().tan();
    let z_gap = settings.z_gap_layers.max(1);

    for p in grid {
        let mut prev_z = layers[0].z;
        let mut prev_grown: Option<Vec<Contour>> = None;

        for (i, layer) in layers.iter().enumerate() {
            let solid_here = point_in_any(&layer.contours, p);
            if !solid_here {
                prev_grown = None;
                prev_z = layer.z;
                continue;
            }

            let backed = prev_grown
                .as_ref()
                .is_some_and(|grown| point_in_any(grown, p));
            if !backed {
                // Unsupported: run a pillar from the build plate up to just
                // short of this layer, skipping any layer where the part
                // itself is already solid at this XY (e.g. a lower,
                // already-printed overhang at the same column).
                let top = i.saturating_sub(z_gap);
                for (layer_idx, below) in layers.iter().enumerate().take(top) {
                    if !point_in_any(&below.contours, p) {
                        result[layer_idx].pillars.push(SupportPillar { center: p });
                    }
                }
            }

            let dz = (layer.z - prev_z).max(1e-6);
            let allowance = dz * tan_angle;
            prev_grown = Some(
                layer
                    .contours
                    .iter()
                    .map(|c| offset_contour(c, allowance))
                    .collect(),
            );
            prev_z = layer.z;
        }
    }

    result
}

fn sample_grid(layers: &[Layer], spacing: f64) -> Option<Vec<P2>> {
    let spacing = spacing.max(1e-3);
    let mut minx = f64::INFINITY;
    let mut miny = f64::INFINITY;
    let mut maxx = f64::NEG_INFINITY;
    let mut maxy = f64::NEG_INFINITY;
    for layer in layers {
        for c in &layer.contours {
            if let Some(((bx0, by0), (bx1, by1))) = c.bbox() {
                minx = minx.min(bx0);
                miny = miny.min(by0);
                maxx = maxx.max(bx1);
                maxy = maxy.max(by1);
            }
        }
    }
    if !minx.is_finite() || !maxx.is_finite() {
        return None;
    }

    let mut pts = Vec::new();
    let mut x = minx + spacing / 2.0;
    while x <= maxx {
        let mut y = miny + spacing / 2.0;
        while y <= maxy {
            pts.push(P2::new(x, y));
            y += spacing;
        }
        x += spacing;
    }
    Some(pts)
}

fn point_in_any(contours: &[Contour], p: P2) -> bool {
    contours.iter().any(|c| point_in_polygon(c, p))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_vertex_kernel::geometry::solid::{Face, Solid as KernSolid};
    use tpt_vertex_kernel::math::Vec3;

    fn box_solid(cx: f64, cy: f64, z0: f64, z1: f64, half: f64) -> KernSolid {
        let mut s = KernSolid::new();
        let (x0, y0) = (cx - half, cy - half);
        let (x1, y1) = (cx + half, cy + half);
        let mut v = |x: f64, y: f64, z: f64| s.add_vertex(Vec3::new(x, y, z));
        let p = [
            v(x0, y0, z0), v(x1, y0, z0), v(x1, y1, z0), v(x0, y1, z0),
            v(x0, y0, z1), v(x1, y0, z1), v(x1, y1, z1), v(x0, y1, z1),
        ];
        let mut f = |a: u32, b: u32, c: u32| s.faces.push(Face::new(a, b, c));
        f(p[0], p[1], p[2]); f(p[0], p[2], p[3]);
        f(p[4], p[6], p[5]); f(p[4], p[7], p[6]);
        f(p[0], p[5], p[1]); f(p[0], p[4], p[5]);
        f(p[1], p[6], p[2]); f(p[1], p[5], p[6]);
        f(p[2], p[7], p[3]); f(p[2], p[6], p[7]);
        f(p[3], p[4], p[0]); f(p[3], p[7], p[4]);
        s
    }

    /// A narrow post (4x4) topped by a much wider slab (10x10): the slab's
    /// first layer overhangs the post on every side and must be supported.
    fn mushroom() -> Vec<Layer> {
        let mut post = box_solid(0.0, 0.0, 0.0, 2.0, 2.0);
        let cap = box_solid(0.0, 0.0, 2.0, 4.0, 5.0);
        // Merge into one mesh (slicing does not require boolean union).
        let base = post.vertices.len() as u32;
        post.vertices.extend(cap.vertices);
        post.faces.extend(cap.faces.iter().map(|f| {
            Face::new(f.a + base, f.b + base, f.c + base)
        }));
        crate::layers::slice_solid(&post, 0.0, 4.0, 0.2, 0.2)
    }

    #[test]
    fn overhanging_cap_gets_support_pillars_beneath() {
        let layers = mushroom();
        let settings = SupportSettings::default();
        let supports = generate_supports(&layers, &settings);
        assert_eq!(supports.len(), layers.len());

        // Somewhere under the post-to-cap transition, pillars must appear.
        assert!(
            supports.iter().any(|l| !l.pillars.is_empty()),
            "expected at least one layer with support pillars"
        );

        // A sample point clearly under the overhang (outside the post,
        // inside the cap) should get a pillar at a low layer.
        let overhang_pt = P2::new(4.0, 0.0);
        let has_pillar_near = supports[0].pillars.iter().any(|pl| pl.center.dist(overhang_pt) < 2.0);
        assert!(has_pillar_near, "expected a pillar near the overhang column at the first layer");
    }

    #[test]
    fn no_support_needed_for_a_plain_vertical_wall() {
        let s = box_solid(0.0, 0.0, 0.0, 4.0, 3.0);
        let layers = crate::layers::slice_solid(&s, 0.0, 4.0, 0.2, 0.2);
        let supports = generate_supports(&layers, &SupportSettings::default());
        assert!(supports.iter().all(|l| l.pillars.is_empty()));
    }
}
