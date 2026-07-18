//! 2D drawing / blueprint generation from 3D models.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! Produces simple orthographic projections (top/front/side) of a kernel
//! [`vertex_kernel::geometry::solid::Solid`] as SVG. This is a v1 stand-in for
//! full GD&T-aware technical drawings: it projects triangle edges onto each
//! principal plane and annotates the bounding-box dimensions.

use vertex_kernel::geometry::solid::{Face, Solid};
use vertex_kernel::math::Vec3;

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

/// Generate an SVG string containing the three orthographic views of `solid`,
/// laid out left-to-right (top, front, side) with dimension annotations.
pub fn drawing_svg(solid: &Solid) -> String {
    let planes = [
        ProjectionPlane::Top,
        ProjectionPlane::Front,
        ProjectionPlane::Side,
    ];
    let cell = 240.0_f32;
    let pad = 20.0_f32;
    let width = (cell + pad) * 3.0 + pad;
    let height = cell + pad * 2.0;

    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\">\n",
        width, height, width, height
    );
    svg.push_str("<style>.edge{stroke:#222;stroke-width:1;fill:none} .dim{stroke:#06c;stroke-width:0.5;fill:none} .txt{font:10px sans-serif;fill:#06c}</style>\n");

    for (i, &plane) in planes.iter().enumerate() {
        let ox = pad + (cell + pad) * i as f32;
        let oy = pad;
        // Clip rectangle.
        svg.push_str(&format!(
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"none\" stroke=\"#ccc\"/>\n",
            ox, oy, cell, cell
        ));

        let mut edges = String::new();
        for f in &solid.faces {
            for (a, b) in face_edges(f) {
                let pa = solid.vertices[a as usize];
                let pb = solid.vertices[b as usize];
                let (ax, ay) = project(pa, plane);
                let (bx, by) = project(pb, plane);
                edges.push_str(&format!(
                    "<line class=\"edge\" x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\"/>\n",
                    ox + ax, oy + ay, ox + bx, oy + by
                ));
            }
        }
        svg.push_str(&edges);

        // Dimension annotation using bounding box along the projected axes.
        let (w, h) = projected_extents(solid, plane);
        svg.push_str(&format!(
            "<text class=\"txt\" x=\"{}\" y=\"{}\">{} x {}</text>\n",
            ox + 4.0,
            oy + cell - 4.0,
            format!("{w:.1}"),
            format!("{h:.1}")
        ));
    }

    svg.push_str("</svg>\n");
    svg
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
