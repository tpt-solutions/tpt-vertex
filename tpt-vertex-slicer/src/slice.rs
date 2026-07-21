//! Top-level slicing orchestration: turn a kernel [`Solid`] into G-code.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0

use tpt_vertex_kernel::geometry::solid::Solid;

use crate::adaptive::adaptive_layer_zs;
use crate::bridging::is_bridge;
use crate::gcode::{emit_gcode, GCode};
use crate::infill::generate_infill;
use crate::layers::{slice_solid as slice_mesh, slice_solid_at_zs, Contour, P2};
use crate::offset::{oriented_ccw, walls_for};
use crate::path::{plan_layer, ExtrusionPath, LayerPlan};
use crate::profile::{MaterialCalibration, PrinterProfile, RegionTag, SliceSettings};
use crate::repair::repair_mesh;
use crate::seam::place_seam;
use crate::support::generate_supports;
use crate::variable_width::thin_wall_fill;

/// The full product of a slice: per-layer plans and the emitted G-code.
#[derive(Debug, Clone)]
pub struct SliceResult {
    pub layers: Vec<LayerPlan>,
    pub gcode: GCode,
}

/// Slice a kernel solid into printable G-code.
///
/// `regions`, if supplied, maps a region (contour) index per layer to a
/// [`RegionTag`] (structural role + extruder/tool) for per-region overrides;
/// when `None` or missing, `settings.default_role` and tool `0` apply.
/// `stress`, if supplied, is sampled at `(x, y, z)` in each region and used to
/// raise local infill density toward `1.0` in proportion to the returned
/// value (expected roughly normalized to `0..=1`) — the caller is expected to
/// derive this from a `tpt-vertex-simulation` FEA run (von Mises stress field)
/// under whatever load case makes sense for the part; slicing itself has no
/// opinion on loads/boundary conditions.
pub fn slice_solid_to_gcode(
    solid: &Solid,
    printer: &PrinterProfile,
    settings: &SliceSettings,
    material: &MaterialCalibration,
    regions: Option<&[Vec<RegionTag>]>,
    stress: Option<&dyn Fn(f64, f64, f64) -> f64>,
) -> SliceResult {
    let repaired;
    let solid: &Solid = if settings.repair_mesh {
        let (cleaned, _report) = repair_mesh(solid, 1e-4);
        repaired = cleaned;
        &repaired
    } else {
        solid
    };

    let Some((min, max)) = solid.bounds() else {
        return SliceResult {
            layers: Vec::new(),
            gcode: GCode::default(),
        };
    };

    let raw_layers = match &settings.adaptive_layers {
        Some(adaptive) => {
            let zs = adaptive_layer_zs(solid, min.z, max.z, settings.first_layer_height, adaptive);
            slice_solid_at_zs(solid, &zs)
        }
        None => slice_mesh(
            solid,
            min.z,
            max.z,
            settings.layer_height,
            settings.first_layer_height,
        ),
    };

    let width = printer.extrusion_width();
    let line_spacing = width * settings.infill_line_spacing_factor;

    let support_layers = settings
        .supports
        .as_ref()
        .map(|s| generate_supports(&raw_layers, s));

    let no_contours: Vec<Contour> = Vec::new();

    let mut layer_plans: Vec<LayerPlan> = Vec::with_capacity(raw_layers.len());

    for (li, layer) in raw_layers.iter().enumerate() {
        let below: &[Contour] = if li == 0 {
            &no_contours
        } else {
            &raw_layers[li - 1].contours
        };

        let mut perimeters: Vec<ExtrusionPath> = Vec::new();
        let mut infill_lines = Vec::new();

        for (ci, contour) in layer.contours.iter().enumerate() {
            let tag = regions.and_then(|r| r.get(li)).and_then(|row| row.get(ci).copied());
            let role = tag.and_then(|t| t.role).unwrap_or(settings.default_role);
            let tool = tag.map(|t| t.tool).unwrap_or(0);

            let wall_count = role.wall_count(settings.wall_count);
            let mut infill_density = role.infill_scale(settings.infill_density);

            let bridge_here = settings
                .bridging
                .as_ref()
                .map(|bs| is_bridge(contour, below, bs))
                .unwrap_or(false);

            // Outer boundary first (oriented CCW), seam-placed if configured,
            // then nested walls (outer-to-inner) offset from it.
            let mut outer = oriented_ccw(contour);
            if let Some(mode) = &settings.seam {
                outer = place_seam(&outer, *mode);
            }

            let insets = walls_for(&outer, wall_count, width);
            let insets_len = insets.len();
            let mut walls = vec![outer.clone()];
            walls.extend(insets);

            for w in &walls {
                let mut pts: Vec<P2> = w.points.clone();
                if pts.len() >= 3 {
                    pts.push(pts[0]); // close the loop
                }
                let mut path = ExtrusionPath::new(pts, true);
                path.tool = tool;
                path.is_bridge = bridge_here;
                perimeters.push(path);
            }

            // A locally thin region (couldn't fit all requested walls) gets a
            // single variable-width centreline fill instead of an unprinted
            // gap; when present it covers the whole remaining interior, so
            // there's no separate infill for this region.
            let mut filled_by_thin_wall = false;
            if let Some(vw_settings) = &settings.variable_width {
                if insets_len < wall_count {
                    let last_d = width * (insets_len as f64 - 0.5).max(0.0);
                    if let Some(thin) = thin_wall_fill(&outer, last_d, width, vw_settings) {
                        let mut pts = thin.path.points.clone();
                        if pts.len() >= 3 {
                            pts.push(pts[0]);
                        }
                        let mut path = ExtrusionPath::new(pts, true);
                        path.tool = tool;
                        path.is_bridge = bridge_here;
                        path.width = Some(thin.width);
                        perimeters.push(path);
                        filled_by_thin_wall = true;
                    }
                }
            }

            if filled_by_thin_wall {
                continue;
            }

            // Infill fills the innermost wall (or the contour itself if no walls).
            let innermost = walls.last().cloned().unwrap_or_else(|| contour.clone());
            let region = Contour {
                points: innermost.points.clone(),
            };

            if let (Some(stress_fn), Some(((minx, miny), (maxx, maxy)))) = (stress, region.bbox()) {
                let cx = (minx + maxx) / 2.0;
                let cy = (miny + maxy) / 2.0;
                let s = stress_fn(cx, cy, layer.z).clamp(0.0, 1.0);
                infill_density = infill_density.max(s).min(1.0);
            }

            if infill_density > 0.0 {
                let lines = generate_infill(
                    &region,
                    infill_density,
                    line_spacing,
                    settings.zigzag_infill,
                    45.0,
                );
                infill_lines.extend(lines.into_iter().map(|l| crate::infill::InfillLine {
                    is_bridge: bridge_here,
                    tool,
                    ..l
                }));
            }
        }

        if let (Some(support_layers), Some(support_settings)) =
            (support_layers.as_ref(), settings.supports.as_ref())
        {
            if let Some(support_layer) = support_layers.get(li) {
                perimeters.extend(support_layer.to_paths(support_settings.pillar_half_width));
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
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path::Move;
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

    fn mushroom() -> KernSolid {
        let mut post = box_solid(0.0, 0.0, 0.0, 2.0, 2.0);
        let cap = box_solid(0.0, 0.0, 2.0, 4.0, 5.0);
        let base = post.vertices.len() as u32;
        post.vertices.extend(cap.vertices);
        post.faces
            .extend(cap.faces.iter().map(|f| Face::new(f.a + base, f.b + base, f.c + base)));
        post
    }

    /// A mushroom shape (narrow post topped by wide slab) should produce G-code
    /// with support pillars when support settings are enabled.
    #[test]
    fn mushroom_with_supports_produces_gcode_with_pillars() {
        let solid = mushroom();
        let settings = SliceSettings {
            supports: Some(crate::support::SupportSettings::default()),
            ..SliceSettings::default()
        };
        let res = slice_solid_to_gcode(
            &solid,
            &PrinterProfile::default(),
            &settings,
            &MaterialCalibration::default(),
            None,
            None,
        );
        assert!(res.gcode.text.contains("G1 X"));
        assert!(res.gcode.estimated_filament_mm > 0.0);
        let has_supports = res.layers.iter().any(|l| {
            l.moves
                .iter()
                .any(|m| matches!(m, Move::Extrude { path, .. } if path.closed && path.points.len() == 5))
        });
        assert!(has_supports, "expected support pillar paths in layer plans");
    }

    /// The mushroom's overhanging cap should be flagged as a bridge and print
    /// with bridge speed/cooling when bridging detection is enabled.
    #[test]
    fn overhanging_cap_prints_as_a_bridge() {
        let solid = mushroom();
        let settings = SliceSettings {
            bridging: Some(crate::bridging::BridgeSettings::default()),
            ..SliceSettings::default()
        };
        let res = slice_solid_to_gcode(
            &solid,
            &PrinterProfile::default(),
            &settings,
            &MaterialCalibration::default(),
            None,
            None,
        );
        assert!(res.gcode.text.contains("M106 S255"), "expected full-fan bridge cooling in gcode");
    }

    #[test]
    fn seam_setting_rotates_perimeter_start_point() {
        let s = cube(Vec3::new(0.0, 0.0, 5.0), 5.0);
        let target = crate::layers::P2::new(5.0, 5.0);
        let settings = SliceSettings {
            seam: Some(crate::seam::SeamMode::NearestTo(target)),
            ..SliceSettings::default()
        };
        let res = slice_solid_to_gcode(
            &s,
            &PrinterProfile::default(),
            &settings,
            &MaterialCalibration::default(),
            None,
            None,
        );
        // Some perimeter loop should start very close to the requested seam point.
        let starts_near_target = res.layers.iter().any(|l| {
            l.moves.iter().any(|m| {
                matches!(m, Move::Extrude { path, .. } if path.closed
                    && path.points.first().is_some_and(|p| p.dist(target) < 1.0))
            })
        });
        assert!(starts_near_target, "expected a perimeter seam near the requested point");
    }

    #[test]
    fn region_tags_assign_tool_to_gcode() {
        let s = cube(Vec3::new(0.0, 0.0, 5.0), 5.0);
        let res = slice_solid(&s);
        let n_layers = res.layers.len();
        let regions: Vec<Vec<RegionTag>> = (0..n_layers)
            .map(|_| vec![RegionTag { role: None, tool: 1 }])
            .collect();
        let printer = PrinterProfile {
            extruders: vec![crate::profile::ExtruderProfile {
                tool: 1,
                nozzle_diameter: 0.4,
                x_offset: 10.0,
                y_offset: 0.0,
                temperature: 215.0,
            }],
            ..PrinterProfile::default()
        };
        let out = slice_solid_to_gcode(
            &s,
            &printer,
            &SliceSettings::default(),
            &MaterialCalibration::default(),
            Some(&regions),
            None,
        );
        assert!(out.gcode.text.contains("T1 ; tool change"));
    }

    #[test]
    fn repair_mesh_setting_still_slices_a_clean_cube() {
        let s = cube(Vec3::new(0.0, 0.0, 5.0), 5.0);
        let settings = SliceSettings {
            repair_mesh: true,
            ..SliceSettings::default()
        };
        let res = slice_solid_to_gcode(
            &s,
            &PrinterProfile::default(),
            &settings,
            &MaterialCalibration::default(),
            None,
            None,
        );
        assert!(res.gcode.text.contains("G1 X"));
        assert!(!res.layers.is_empty());
    }

    #[test]
    fn stress_field_raises_infill_density_where_stressed() {
        let s = cube(Vec3::new(0.0, 0.0, 5.0), 5.0);
        let settings = SliceSettings {
            infill_density: 0.1,
            ..SliceSettings::default()
        };
        let stress_fn: &dyn Fn(f64, f64, f64) -> f64 = &|_x, _y, _z| 1.0;
        let res_stressed = slice_solid_to_gcode(
            &s,
            &PrinterProfile::default(),
            &settings,
            &MaterialCalibration::default(),
            None,
            Some(stress_fn),
        );
        let res_plain = slice_solid_to_gcode(
            &s,
            &PrinterProfile::default(),
            &settings,
            &MaterialCalibration::default(),
            None,
            None,
        );
        assert!(res_stressed.gcode.estimated_filament_mm > res_plain.gcode.estimated_filament_mm);
    }
}
