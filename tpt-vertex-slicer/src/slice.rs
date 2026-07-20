//! Top-level slicing orchestration: turn a kernel [`Solid`] into G-code.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0

use tpt_vertex_kernel::geometry::solid::Solid;

use crate::gcode::{emit_gcode, GCode};
use crate::infill::generate_infill;
use crate::layers::{slice_solid as slice_mesh, Contour, P2};
use crate::offset::walls_for;
use crate::path::{plan_layer, ExtrusionPath, LayerPlan};
use crate::profile::{BodyRole, MaterialCalibration, PrinterProfile, SliceSettings};

/// The full product of a slice: per-layer plans and the emitted G-code.
#[derive(Debug, Clone)]
pub struct SliceResult {
    pub layers: Vec<LayerPlan>,
    pub gcode: GCode,
}

/// Slice a kernel solid into printable G-code.
///
/// `roles`, if supplied, maps a region (contour) index per layer to a
/// [`BodyRole`] for per-region wall/infill overrides; when `None` or missing,
/// `settings.default_role` is used everywhere. `material` calibrates flow and
/// temperatures.
pub fn slice_solid_to_gcode(
    solid: &Solid,
    printer: &PrinterProfile,
    settings: &SliceSettings,
    material: &MaterialCalibration,
    roles: Option<&[Vec<BodyRole>]>,
) -> SliceResult {
    let Some((min, max)) = solid.bounds() else {
        return SliceResult {
            layers: Vec::new(),
            gcode: GCode::default(),
        };
    };

    let raw_layers = slice_mesh(
        solid,
        min.z,
        max.z,
        settings.layer_height,
        settings.first_layer_height,
    );

    let width = printer.extrusion_width();
    let line_spacing = width * settings.infill_line_spacing_factor;

    let mut layer_plans: Vec<LayerPlan> = Vec::with_capacity(raw_layers.len());

    for (li, layer) in raw_layers.iter().enumerate() {
        let mut perimeters: Vec<ExtrusionPath> = Vec::new();
        let mut infill_lines = Vec::new();

        for (ci, contour) in layer.contours.iter().enumerate() {
            let role = roles
                .and_then(|r| r.get(li))
                .and_then(|row| row.get(ci).copied())
                .unwrap_or(settings.default_role);

            let wall_count = role.wall_count(settings.wall_count);
            let infill_density = role.infill_scale(settings.infill_density);

            // Outer boundary first (oriented CCW), then nested walls (outer-to-
            // inner).
            let outer = crate::offset::oriented_ccw(contour);
            let mut walls = vec![outer.clone()];
            walls.extend(walls_for(&outer, wall_count, width));

            for w in &walls {
                let mut pts: Vec<P2> = w.points.clone();
                if pts.len() >= 3 {
                    pts.push(pts[0]); // close the loop
                }
                perimeters.push(ExtrusionPath {
                    points: pts,
                    closed: true,
                });
            }

            // Infill fills the innermost wall (or the contour itself if no walls).
            let innermost = walls.last().cloned().unwrap_or_else(|| contour.clone());
            let region = Contour {
                points: innermost.points.clone(),
            };
            if infill_density > 0.0 {
                let lines = generate_infill(
                    &region,
                    infill_density,
                    line_spacing,
                    settings.zigzag_infill,
                    45.0,
                );
                infill_lines.extend(lines);
            }
        }

        // Order: outer walls first, then infill.
        let plan = plan_layer(layer.z, perimeters, infill_lines);
        layer_plans.push(plan);
    }

    let gcode = emit_gcode(&layer_plans, printer, material);
    SliceResult {
        layers: layer_plans,
        gcode,
    }
}

/// Convenience: slice with default printer + settings and PLA calibration.
pub fn slice_solid(solid: &Solid) -> SliceResult {
    slice_solid_to_gcode(
        solid,
        &PrinterProfile::default(),
        &SliceSettings::default(),
        &MaterialCalibration::default(),
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_vertex_kernel::geometry::solid::{Face, Solid as KernSolid};
    use tpt_vertex_kernel::math::Vec3;

    fn cube(center: Vec3, half: f64) -> KernSolid {
        let mut s = KernSolid::new();
        let (x0, y0, z0) = (center.x - half, center.y - half, center.z - half);
        let (x1, y1, z1) = (center.x + half, center.y + half, center.z + half);
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

    #[test]
    fn slice_cube_produces_gcode() {
        let s = cube(Vec3::new(0.0, 0.0, 5.0), 5.0); // 10x10x10 cube on bed
        let res = slice_solid(&s);
        assert!(res.layers.len() >= 40, "layers {}", res.layers.len());
        assert!(res.gcode.text.contains("G1 X"));
        assert!(res.gcode.estimated_filament_mm > 0.0);
    }
}
