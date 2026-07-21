//! Planar slicing: intersect a triangle mesh with horizontal Z planes and stitch
//! the resulting segments into closed contours (perimeters).
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0

use tpt_vertex_kernel::geometry::solid::Solid;
use tpt_vertex_kernel::math::Vec3;

/// A 2D point in the slicing plane (XY), carrying its Z for diagnostics.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct P2 {
    pub x: f64,
    pub y: f64,
}

impl P2 {
    pub fn new(x: f64, y: f64) -> Self {
        P2 { x, y }
    }
    pub fn dist(self, other: Self) -> f64 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
    }
}

/// A directed segment (an intersection edge) in the slicing plane.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Seg {
    pub a: P2,
    pub b: P2,
}

/// A closed contour (single polygon boundary).
#[derive(Debug, Clone, PartialEq)]
pub struct Contour {
    pub points: Vec<P2>,
}

impl Contour {
    /// Signed area via the shoelace formula (positive == counter-clockwise).
    pub fn signed_area(&self) -> f64 {
        let n = self.points.len();
        if n < 3 {
            return 0.0;
        }
        let mut area = 0.0;
        for i in 0..n {
            let p = self.points[i];
            let q = self.points[(i + 1) % n];
            area += p.x * q.y - q.x * p.y;
        }
        area / 2.0
    }

    /// True when the contour winds counter-clockwise (outer boundary).
    pub fn is_ccw(&self) -> bool {
        self.signed_area() > 0.0
    }

    /// Bounding box `(min, max)` of the contour.
    pub fn bbox(&self) -> Option<((f64, f64), (f64, f64))> {
        let mut iter = self.points.iter();
        let first = *iter.next()?;
        let (mut minx, mut miny) = (first.x, first.y);
        let (mut maxx, mut maxy) = (first.x, first.y);
        for p in iter {
            minx = minx.min(p.x);
            miny = miny.min(p.y);
            maxx = maxx.max(p.x);
            maxy = maxy.max(p.y);
        }
        Some(((minx, miny), (maxx, maxy)))
    }
}

/// All contours produced for a single Z layer.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Layer {
    pub z: f64,
    pub contours: Vec<Contour>,
}

/// Slice a solid into layers from `z_min` to `z_max` at the given `layer_height`.
///
/// The first layer sits at `z_min` (or `first_layer_height` above it when the
/// solid does not start exactly at the bed). Returns one [`Layer`] per Z plane
/// that intersects at least one triangle.
pub fn slice_solid(
    solid: &Solid,
    z_min: f64,
    z_max: f64,
    layer_height: f64,
    first_layer_height: f64,
) -> Vec<Layer> {
    let mut layers = Vec::new();
    if layer_height <= 0.0 || z_max <= z_min {
        return layers;
    }

    // Build the per-triangle vertex cache once.
    let tris: Vec<[Vec3; 3]> = solid
        .faces
        .iter()
        .map(|f| {
            [
                solid.vertices[f.a as usize],
                solid.vertices[f.b as usize],
                solid.vertices[f.c as usize],
            ]
        })
        .collect();

    // First layer uses the (possibly thicker) first-layer height; subsequent
    // layers advance by the uniform layer_height on the nominal grid.
    let mut z = z_min + first_layer_height;
    while z <= z_max + 1e-9 {
        let contours = slice_at_z(&tris, z);
        if !contours.is_empty() {
            layers.push(Layer { z, contours });
        }
        z += layer_height;
    }
    layers
}

/// Slice a solid at an explicit, caller-supplied sequence of Z heights (top of
/// each layer), rather than a fixed `layer_height` step. Used by adaptive
/// layer-height slicing, where the step between layers varies with local
/// surface slope. Layers whose plane does not cross the mesh are omitted.
pub fn slice_solid_at_zs(solid: &Solid, zs: &[f64]) -> Vec<Layer> {
    let tris: Vec<[Vec3; 3]> = solid
        .faces
        .iter()
        .map(|f| {
            [
                solid.vertices[f.a as usize],
                solid.vertices[f.b as usize],
                solid.vertices[f.c as usize],
            ]
        })
        .collect();

    let mut layers = Vec::with_capacity(zs.len());
    for &z in zs {
        let contours = slice_at_z(&tris, z);
        if !contours.is_empty() {
            layers.push(Layer { z, contours });
        }
    }
    layers
}

/// Intersect all triangles with the horizontal plane `z` and stitch segments into
/// closed contours. Segments are matched greedily by endpoint proximity.
pub fn slice_at_z(tris: &[[Vec3; 3]], z: f64) -> Vec<Contour> {
    let mut segs: Vec<Seg> = Vec::new();
    for tri in tris {
        if let Some(s) = intersect_triangle(*tri, z) {
            segs.push(s);
        }
    }
    if segs.is_empty() {
        return Vec::new();
    }
    stitch_segments(&mut segs)
}

/// Compute the intersection of a single triangle with the plane `z`, returning a
/// segment, or `None` if the triangle does not cross the plane.
pub fn intersect_triangle(tri: [Vec3; 3], z: f64) -> Option<Seg> {
    // Classify vertices relative to the plane. Vertices exactly on the plane are
    // treated as a (rare) boundary case and ignored: a triangle with exactly one
    // on-plane vertex still has one above + one below, which yields the correct
    // single crossing segment below.
    let mut above = [Vec3::ZERO; 3];
    let mut below = [Vec3::ZERO; 3];
    let mut n_above = 0usize;
    let mut n_below = 0usize;
    for &v in &tri {
        if (v.z - z).abs() < 1e-9 {
            continue;
        } else if v.z > z {
            above[n_above] = v;
            n_above += 1;
        } else {
            below[n_below] = v;
            n_below += 1;
        }
    }

    // A valid crossing requires at least one vertex above and one below. A
    // triangle entirely on one side of the plane does not intersect it.
    if n_above == 0 || n_below == 0 || n_above == 3 || n_below == 3 {
        return None;
    }

    // Interpolate the crossing points for every above/below pair. For a triangle
    // straddling the plane this is exactly two points.
    let mut pts: Vec<P2> = Vec::with_capacity(2);
    for &a in above.iter().take(n_above) {
        for &b in below.iter().take(n_below) {
            let t = (z - a.z) / (b.z - a.z);
            pts.push(P2::new(
                a.x + t * (b.x - a.x),
                a.y + t * (b.y - a.y),
            ));
        }
    }
    if pts.len() < 2 {
        return None;
    }

    Some(Seg {
        a: pts[0],
        b: pts[1],
    })
}

/// Greedily stitch segments into closed contours by matching endpoints.
///
/// Segments whose endpoints are within `tol` are joined. Orientation is made
/// consistent so consecutive points form a continuous loop. O(n²) is acceptable
/// for v1 slice densities; a spatial hash is a later refinement.
pub fn stitch_segments(segs: &mut [Seg]) -> Vec<Contour> {
    let tol = 1e-6;
    let n = segs.len();
    let mut used = vec![false; n];
    let mut contours = Vec::new();

    for start in 0..n {
        if used[start] {
            continue;
        }
        used[start] = true;
        // Build the loop starting from this segment. We keep the segment as-is
        // (a->b) and walk forward, always appending the far endpoint of the next
        // connecting segment.
        let mut pts = vec![segs[start].a, segs[start].b];

        loop {
            let tail = *pts.last().unwrap();
            // Find an unused segment that connects to the tail.
            let mut found: Option<usize> = None;
            let mut far = P2::new(0.0, 0.0);
            for i in 0..n {
                if used[i] {
                    continue;
                }
                if segs[i].a.dist(tail) <= tol {
                    found = Some(i);
                    far = segs[i].b;
                    break;
                } else if segs[i].b.dist(tail) <= tol {
                    found = Some(i);
                    far = segs[i].a;
                    break;
                }
            }

            if let Some(i) = found {
                used[i] = true;
                // Closed loop: we returned to the very first point.
                if far.dist(pts[0]) <= tol {
                    break;
                }
                pts.push(far);
            } else {
                break;
            }
        }

        if pts.len() >= 3 {
            contours.push(Contour { points: pts });
        }
    }
    contours
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_vertex_kernel::geometry::solid::Solid as KernSolid;

    fn cube(center: Vec3, half: f64) -> KernSolid {
        let mut s = KernSolid::new();
        let (x0, y0, z0) = (center.x - half, center.y - half, center.z - half);
        let (x1, y1, z1) = (center.x + half, center.y + half, center.z + half);
        let mut v = |x: f64, y: f64, z: f64| s.add_vertex(Vec3::new(x, y, z));
        let p = [
            v(x0, y0, z0),
            v(x1, y0, z0),
            v(x1, y1, z0),
            v(x0, y1, z0),
            v(x0, y0, z1),
            v(x1, y0, z1),
            v(x1, y1, z1),
            v(x0, y1, z1),
        ];
        let mut f = |a: u32, b: u32, c: u32| s.faces.push(tpt_vertex_kernel::geometry::solid::Face::new(a, b, c));
        // bottom
        f(p[0], p[1], p[2]);
        f(p[0], p[2], p[3]);
        // top
        f(p[4], p[6], p[5]);
        f(p[4], p[7], p[6]);
        // sides
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
    fn cube_mid_slice_is_square() {
        let s = cube(Vec3::new(0.0, 0.0, 0.5), 1.0);
        let tris: Vec<[Vec3; 3]> = s
            .faces
            .iter()
            .map(|f| {
                [
                    s.vertices[f.a as usize],
                    s.vertices[f.b as usize],
                    s.vertices[f.c as usize],
                ]
            })
            .collect();
        let contours = slice_at_z(&tris, 0.5);
        assert_eq!(contours.len(), 1);
        let c = &contours[0];
        // 2x2 square centred at origin.
        assert!((c.signed_area().abs() - 4.0).abs() < 1e-6, "area {}", c.signed_area());
    }

    #[test]
    fn cube_layers_count() {
        let s = cube(Vec3::new(0.0, 0.0, 1.0), 1.0);
        let layers = slice_solid(&s, 0.0, 2.0, 0.2, 0.2);
        // z from 0.2 .. 2.0 stepping 0.2 => ~10 layers, each with one contour.
        assert!(layers.len() >= 9 && layers.len() <= 11);
        for l in &layers {
            assert_eq!(l.contours.len(), 1);
        }
    }
}
