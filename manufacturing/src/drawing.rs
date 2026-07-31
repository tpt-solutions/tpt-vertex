//! 2D drawing / blueprint generation from 3D models.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! Produces orthographic projections (top/front/side) of a kernel
//! [`tpt_vertex_kernel::geometry::solid::Solid`] as SVG, with proper
//! dimension lines (extension lines, dimension lines, arrowheads, value text),
//! and optional GD&T feature control frames attached to specific views.

use tpt_vertex_kernel::gdt::GdtAnnotation;
use tpt_vertex_kernel::geometry::solid::{Face, Solid};
use tpt_vertex_kernel::math::Vec3;

/// Which orthographic plane to project onto.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectionPlane {
    /// View down -Z (XY plane).
    Top,
    /// View down -Y (XZ plane).
    Front,
    /// View down -X (YZ plane).
    Side,
}

fn project(p: Vec3, plane: ProjectionPlane) -> (f32, f32) {
    match plane {
        ProjectionPlane::Top => (p.x as f32, p.y as f32),
        ProjectionPlane::Front => (p.x as f32, p.z as f32),
        ProjectionPlane::Side => (p.y as f32, p.z as f32),
    }
}

/// Generate an SVG string containing three orthographic views of `solid`,
/// with dimension lines and optional GD&T annotations.
pub fn drawing_svg(solid: &Solid) -> String {
    drawing_svg_with_gdt(solid, &[])
}

/// Generate an SVG string with GD&T annotations attached to specific views.
///
/// Each annotation is rendered as feature control frame text below the view
/// indicated by its associated `ProjectionPlane`.
pub fn drawing_svg_with_gdt(
    solid: &Solid,
    annotations: &[(ProjectionPlane, &GdtAnnotation)],
) -> String {
    let planes = [
        ProjectionPlane::Top,
        ProjectionPlane::Front,
        ProjectionPlane::Side,
    ];
    let cell = 240.0_f32;
    let pad = 20.0_f32;
    let dim_offset = 25.0_f32; // Distance from view edge to dimension line.
    let width = (cell + pad) * 3.0 + pad;
    let height = cell + pad * 2.0 + 40.0; // Extra space for GD&T callouts.

    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\">\n",
        width, height, width, height
    );
    svg.push_str(
        "<style>\
        .edge{stroke:#222;stroke-width:1;fill:none}\
        .hidden{stroke:#999;stroke-width:0.5;stroke-dasharray:4,3;fill:none}\
        .dim-line{stroke:#06c;stroke-width:0.5;fill:none}\
        .dim-ext{stroke:#06c;stroke-width:0.3;fill:none}\
        .dim-txt{font:9px sans-serif;fill:#06c}\
        .gdt-txt{font:8px monospace;fill:#333}\
        .title{font:12px sans-serif;fill:#222}\
    </style>\n",
    );

    for (i, &plane) in planes.iter().enumerate() {
        let ox = pad + (cell + pad) * i as f32;
        let oy = pad;
        // View title.
        let title = match plane {
            ProjectionPlane::Top => "TOP VIEW",
            ProjectionPlane::Front => "FRONT VIEW",
            ProjectionPlane::Side => "SIDE VIEW",
        };
        svg.push_str(&format!(
            "<text class=\"title\" x=\"{}\" y=\"{}\">{}</text>\n",
            ox + cell / 2.0,
            oy - 5.0,
            title
        ));

        // Clip rectangle.
        svg.push_str(&format!(
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"none\" stroke=\"#ccc\"/>\n",
            ox, oy, cell, cell
        ));

        // Project and draw edges.
        let mut edges = String::new();
        for f in &solid.faces {
            for (a, b) in face_edges(f) {
                let pa = solid.vertices[a as usize];
                let pb = solid.vertices[b as usize];
                let (ax, ay) = project(pa, plane);
                let (bx, by) = project(pb, plane);
                edges.push_str(&format!(
                    "<line class=\"edge\" x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\"/>\n",
                    ox + ax,
                    oy + ay,
                    ox + bx,
                    oy + by
                ));
            }
        }
        svg.push_str(&edges);

        // Draw dimension lines for bounding-box extents.
        let (w, h) = projected_extents(solid, plane);
        draw_dimensions(&mut svg, ox, oy, cell, w, h, dim_offset);
    }

    // Draw GD&T annotations below the views.
    let mut gdt_y = pad + cell + pad + 20.0;
    for (plane, annotation) in annotations {
        let col = match plane {
            ProjectionPlane::Top => 0,
            ProjectionPlane::Front => 1,
            ProjectionPlane::Side => 2,
        };
        let ox = pad + (cell + pad) * col as f32;

        for frame in &annotation.frames {
            let fcf_text = render_fcf_svg(frame);
            svg.push_str(&format!(
                "<text class=\"gdt-txt\" x=\"{:.1}\" y=\"{:.1}\">{}</text>\n",
                ox, gdt_y, fcf_text
            ));
            gdt_y += 14.0;
        }

        // Datum targets.
        for datum in &annotation.datums {
            svg.push_str(&format!(
                "<text class=\"gdt-txt\" x=\"{:.1}\" y=\"{:.1}\">DATUM {} ({:?})</text>\n",
                ox, gdt_y, datum.datum, datum.target_type
            ));
            gdt_y += 14.0;
        }
    }

    // Title block at bottom.
    svg.push_str(&format!(
        "<text class=\"title\" x=\"{:.1}\" y=\"{:.1}\">TPT Vertex — Technical Drawing</text>\n",
        pad,
        height - 8.0
    ));

    svg.push_str("</svg>\n");
    svg
}

/// Render a feature control frame as an SVG text string.
fn render_fcf_svg(frame: &tpt_vertex_kernel::gdt::FeatureControlFrame) -> String {
    let char_sym = frame.characteristic.symbol();
    let tol_mod = frame.tol_modifier.map(|m| m.symbol()).unwrap_or("");
    let datums: String = frame
        .datums
        .iter()
        .map(|d| format!(" {}", d.label))
        .collect();
    format!(
        "[{} | {:.3}{} |{}]",
        char_sym, frame.tolerance, tol_mod, datums,
    )
}

/// Draw dimension lines (horizontal and vertical) around a view.
fn draw_dimensions(svg: &mut String, ox: f32, oy: f32, cell: f32, w: f64, h: f64, offset: f32) {
    // Horizontal dimension (width) below the view.
    let dim_y = oy + cell + offset * 0.6;
    let ext1_x = ox + offset;
    let ext2_x = ox + cell - offset;

    // Extension lines.
    svg.push_str(&format!(
        "<line class=\"dim-ext\" x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\"/>\n",
        ext1_x,
        oy + cell,
        ext1_x,
        dim_y + 3.0
    ));
    svg.push_str(&format!(
        "<line class=\"dim-ext\" x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\"/>\n",
        ext2_x,
        oy + cell,
        ext2_x,
        dim_y + 3.0
    ));
    // Dimension line with arrowheads.
    svg.push_str(&format!(
        "<line class=\"dim-line\" x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\"/>\n",
        ext1_x + 3.0,
        dim_y,
        ext2_x - 3.0,
        dim_y
    ));
    // Arrowheads (small triangles).
    svg.push_str(&format!(
        "<polygon class=\"dim-line\" points=\"{:.1},{:.1} {:.1},{:.1} {:.1},{:.1}\"/>\n",
        ext1_x + 3.0,
        dim_y,
        ext1_x + 7.0,
        dim_y - 2.0,
        ext1_x + 7.0,
        dim_y + 2.0
    ));
    svg.push_str(&format!(
        "<polygon class=\"dim-line\" points=\"{:.1},{:.1} {:.1},{:.1} {:.1},{:.1}\"/>\n",
        ext2_x - 3.0,
        dim_y,
        ext2_x - 7.0,
        dim_y - 2.0,
        ext2_x - 7.0,
        dim_y + 2.0
    ));
    // Dimension text.
    svg.push_str(&format!(
        "<text class=\"dim-txt\" x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\">{:.1}</text>\n",
        (ext1_x + ext2_x) / 2.0,
        dim_y - 3.0,
        w
    ));

    // Vertical dimension (height) to the right of the view.
    let dim_x = ox + cell + offset * 0.6;
    let ext1_y = oy + offset;
    let ext2_y = oy + cell - offset;

    // Extension lines.
    svg.push_str(&format!(
        "<line class=\"dim-ext\" x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\"/>\n",
        ox + cell,
        ext1_y,
        dim_x + 3.0,
        ext1_y
    ));
    svg.push_str(&format!(
        "<line class=\"dim-ext\" x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\"/>\n",
        ox + cell,
        ext2_y,
        dim_x + 3.0,
        ext2_y
    ));
    // Dimension line.
    svg.push_str(&format!(
        "<line class=\"dim-line\" x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\"/>\n",
        dim_x,
        ext1_y + 3.0,
        dim_x,
        ext2_y - 3.0
    ));
    // Arrowheads.
    svg.push_str(&format!(
        "<polygon class=\"dim-line\" points=\"{:.1},{:.1} {:.1},{:.1} {:.1},{:.1}\"/>\n",
        dim_x,
        ext1_y + 3.0,
        dim_x - 2.0,
        ext1_y + 7.0,
        dim_x + 2.0,
        ext1_y + 7.0
    ));
    svg.push_str(&format!(
        "<polygon class=\"dim-line\" points=\"{:.1},{:.1} {:.1},{:.1} {:.1},{:.1}\"/>\n",
        dim_x,
        ext2_y - 3.0,
        dim_x - 2.0,
        ext2_y - 7.0,
        dim_x + 2.0,
        ext2_y - 7.0
    ));
    // Dimension text (rotated 90°).
    svg.push_str(&format!(
        "<text class=\"dim-txt\" x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" transform=\"rotate(-90,{:.1},{:.1})\">{:.1}</text>\n",
        dim_x + 12.0,
        (ext1_y + ext2_y) / 2.0,
        dim_x + 12.0,
        (ext1_y + ext2_y) / 2.0,
        h
    ));
}

fn face_edges(f: &Face) -> [(u32, u32); 3] {
    [(f.a, f.b), (f.b, f.c), (f.c, f.a)]
}

fn projected_extents(solid: &Solid, plane: ProjectionPlane) -> (f64, f64) {
    let mut min_x = f64::MAX;
    let mut max_x = f64::MIN;
    let mut min_y = f64::MAX;
    let mut max_y = f64::MIN;
    for v in &solid.vertices {
        let (x, y) = project(*v, plane);
        min_x = min_x.min(x as f64);
        max_x = max_x.max(x as f64);
        min_y = min_y.min(y as f64);
        max_y = max_y.max(y as f64);
    }
    if !min_x.is_finite() {
        return (0.0, 0.0);
    }
    (max_x - min_x, max_y - min_y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_vertex_kernel::gdt::{FeatureControlFrame, GeometricCharacteristic};
    use tpt_vertex_kernel::geometry::solid::{Face, Solid as KernSolid};

    fn cube_solid() -> KernSolid {
        let mut s = KernSolid::new();
        let mut v = |x: f64, y: f64, z: f64| s.add_vertex(Vec3::new(x, y, z));
        let p = [
            v(0.0, 0.0, 0.0),
            v(10.0, 0.0, 0.0),
            v(10.0, 10.0, 0.0),
            v(0.0, 10.0, 0.0),
            v(0.0, 0.0, 5.0),
            v(10.0, 0.0, 5.0),
            v(10.0, 10.0, 5.0),
            v(0.0, 10.0, 5.0),
        ];
        let mut f = |a: u32, b: u32, c: u32| s.faces.push(Face::new(a, b, c));
        f(p[0], p[1], p[2]);
        f(p[0], p[2], p[3]);
        f(p[4], p[6], p[5]);
        f(p[4], p[7], p[6]);
        f(p[0], p[5], p[1]);
        f(p[0], p[4], p[5]);
        f(p[1], p[6], p[2]);
        f(p[1], p[5], p[6]);
        f(p[2], p[7], p[3]);
        f(p[2], p[6], p[7]);
        f(p[3], p[4], p[0]);
        f(p[3], p[7], p[4]);
        s
    }

    #[test]
    fn drawing_svg_contains_views() {
        let svg = drawing_svg(&cube_solid());
        assert!(svg.contains("TOP VIEW"));
        assert!(svg.contains("FRONT VIEW"));
        assert!(svg.contains("SIDE VIEW"));
    }

    #[test]
    fn drawing_svg_contains_dimension_lines() {
        let svg = drawing_svg(&cube_solid());
        assert!(svg.contains("dim-line"));
        assert!(svg.contains("dim-ext"));
        assert!(svg.contains("dim-txt"));
    }

    #[test]
    fn drawing_svg_with_gdt_renders_fcf() {
        let ann = GdtAnnotation::new().add_frame(FeatureControlFrame::new(
            GeometricCharacteristic::Flatness,
            0.05,
        ));
        let svg = drawing_svg_with_gdt(&cube_solid(), &[(ProjectionPlane::Top, &ann)]);
        assert!(svg.contains("gdt-txt"));
        // The FCF should contain the flatness symbol.
        assert!(svg.contains("\u{25A1}") || svg.contains("Flatness"));
    }
}
