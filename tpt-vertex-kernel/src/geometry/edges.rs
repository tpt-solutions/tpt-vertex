//! Edge extraction/classification and edge-based fillet & chamfer.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! The faceted [`Solid`] stores only triangles, so edges are *derived*: every
//! triangle contributes three directed half-edges, and half-edges sharing the
//! same (welded) endpoint pair are the same edge. Each edge is then classified
//! by the dihedral angle between its two adjacent faces — the information a
//! fillet/chamfer feature needs in order to know which edges it may round.
//!
//! Fillet and chamfer are implemented as real geometry operations on top of the
//! BSP boolean engine (see [`crate::geometry::csg_bsp`], ADR-0013): each
//! selected edge contributes one or more cutting planes, and the solid is
//! intersected with the corresponding half-spaces.
//!
//! # Known limits
//!
//! - **Convex edges only.** A plane cut can only *remove* material, so concave
//!   edges (which need material *added* by the rolling ball) are skipped, as
//!   are smooth, boundary and non-manifold edges.
//! - **Faceted fillets.** The rolling-ball fillet surface is approximated by a
//!   fan of planes tangent to the fillet cylinder. It is a circumscribed
//!   approximation (very slightly proud of the true arc), refined by raising
//!   the segment count.
//! - **Radius/distance must be small relative to the feature.** A plane cut is
//!   global: an over-large radius will slice through unrelated geometry. Cuts
//!   that would empty the solid are skipped rather than applied.
//! - **Planar-faced solids.** Classification assumes planar faces; on a
//!   tessellated cylinder each facet boundary is a genuine (small) convex edge
//!   unless it falls inside the smoothness tolerance.

use crate::geometry::csg_bsp::cut_half_space;
use crate::geometry::solid::Solid;
use crate::math::Vec3;
use std::collections::HashMap;

/// Default angular tolerance (radians) below which an edge counts as smooth
/// (tangent-continuous / effectively coplanar).
pub const DEFAULT_SMOOTH_TOL_RAD: f64 = 1e-6;

/// Position tolerance used when welding vertices for edge extraction.
pub const WELD_TOL: f64 = 1e-9;

/// Number of tangent planes used per filleted edge by [`fillet_edges`].
pub const DEFAULT_FILLET_SEGMENTS: usize = 4;

/// How an edge sits between its adjacent faces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeKind {
    /// Material is on the inside of the dihedral: the edge sticks out.
    Convex,
    /// The dihedral folds inward: the edge is a notch/pocket corner.
    Concave,
    /// The adjacent faces are (within tolerance) coplanar.
    Smooth,
    /// Only one adjacent face: an open boundary of a non-closed shell.
    Boundary,
    /// Three or more adjacent faces: the mesh is non-manifold here.
    NonManifold,
}

/// A derived, classified edge of a [`Solid`].
#[derive(Debug, Clone, PartialEq)]
pub struct EdgeClass {
    /// Start vertex index, in the winding order of `faces[0]`.
    pub v0: u32,
    /// End vertex index, in the winding order of `faces[0]`.
    pub v1: u32,
    /// Indices of the faces sharing this edge, in first-seen order.
    pub faces: Vec<usize>,
    /// Classification.
    pub kind: EdgeKind,
    /// Signed dihedral deviation from flat, in radians: positive for convex,
    /// negative for concave, zero for smooth/boundary/non-manifold.
    pub angle: f64,
    /// Edge length.
    pub length: f64,
}

impl EdgeClass {
    /// Whether this edge can be filleted/chamfered by the plane-cut engine.
    pub fn is_roundable(&self) -> bool {
        self.kind == EdgeKind::Convex && self.faces.len() == 2 && self.length > WELD_TOL
    }
}

/// Outward normal of a face (zero-length for degenerate triangles).
pub fn face_normal(solid: &Solid, face: usize) -> Vec3 {
    let f = match solid.faces.get(face) {
        Some(f) => f,
        None => return Vec3::ZERO,
    };
    let n = solid.vertices.len() as u32;
    if f.a >= n || f.b >= n || f.c >= n {
        return Vec3::ZERO;
    }
    let a = solid.vertices[f.a as usize];
    let b = solid.vertices[f.b as usize];
    let c = solid.vertices[f.c as usize];
    (b - a).cross(c - a).normalize()
}

/// Centroid of a face (origin for degenerate/out-of-range faces).
pub fn face_centroid(solid: &Solid, face: usize) -> Vec3 {
    let f = match solid.faces.get(face) {
        Some(f) => f,
        None => return Vec3::ZERO,
    };
    let n = solid.vertices.len() as u32;
    if f.a >= n || f.b >= n || f.c >= n {
        return Vec3::ZERO;
    }
    (solid.vertices[f.a as usize] + solid.vertices[f.b as usize] + solid.vertices[f.c as usize])
        * (1.0 / 3.0)
}

/// Map every vertex index onto a canonical index for its position, so meshes
/// that duplicate coincident vertices still yield a shared edge topology.
pub fn welded_indices(solid: &Solid, tol: f64) -> Vec<u32> {
    let mut map: Vec<u32> = Vec::with_capacity(solid.vertices.len());
    let mut cells: HashMap<(i64, i64, i64), Vec<u32>> = HashMap::new();
    let q = tol.max(f64::MIN_POSITIVE);
    for (i, v) in solid.vertices.iter().enumerate() {
        let key = (quantize(v.x, q), quantize(v.y, q), quantize(v.z, q));
        let mut found = None;
        'probe: for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    if let Some(bucket) = cells.get(&(key.0 + dx, key.1 + dy, key.2 + dz)) {
                        for &j in bucket {
                            if solid.vertices[j as usize].distance(*v) <= tol {
                                found = Some(j);
                                break 'probe;
                            }
                        }
                    }
                }
            }
        }
        match found {
            Some(j) => map.push(j),
            None => {
                cells.entry(key).or_default().push(i as u32);
                map.push(i as u32);
            }
        }
    }
    map
}

fn quantize(x: f64, q: f64) -> i64 {
    let s = x / q;
    if s.is_finite() {
        s.round().clamp(i64::MIN as f64, i64::MAX as f64) as i64
    } else {
        0
    }
}

/// Classify every edge of `solid` with the default smoothness tolerance.
pub fn classify_edges(solid: &Solid) -> Vec<EdgeClass> {
    classify_edges_with_tolerance(solid, DEFAULT_SMOOTH_TOL_RAD)
}

/// Classify every edge of `solid`, treating dihedral deviations below
/// `smooth_tol_rad` as [`EdgeKind::Smooth`].
///
/// Edges are returned in first-seen (face, corner) order, which is stable for a
/// given mesh — callers may use the position in this list as an edge id (that
/// is what `edge_filter` in [`fillet_edges`]/[`chamfer_edges`] expects).
pub fn classify_edges_with_tolerance(solid: &Solid, smooth_tol_rad: f64) -> Vec<EdgeClass> {
    let weld = welded_indices(solid, WELD_TOL);
    let mut index: HashMap<(u32, u32), usize> = HashMap::new();
    let mut edges: Vec<EdgeClass> = Vec::new();

    for (fi, f) in solid.faces.iter().enumerate() {
        let idx = f.indices();
        for k in 0..3 {
            let (a, b) = (idx[k], idx[(k + 1) % 3]);
            let (ca, cb) = match (weld.get(a as usize), weld.get(b as usize)) {
                (Some(&ca), Some(&cb)) => (ca, cb),
                _ => continue,
            };
            if ca == cb {
                continue; // degenerate half-edge
            }
            let key = if ca < cb { (ca, cb) } else { (cb, ca) };
            match index.get(&key) {
                Some(&ei) => edges[ei].faces.push(fi),
                None => {
                    index.insert(key, edges.len());
                    edges.push(EdgeClass {
                        v0: ca,
                        v1: cb,
                        faces: vec![fi],
                        kind: EdgeKind::Boundary,
                        angle: 0.0,
                        length: solid.vertices[ca as usize].distance(solid.vertices[cb as usize]),
                    });
                }
            }
        }
    }

    for e in &mut edges {
        e.kind = match e.faces.len() {
            0 => EdgeKind::Boundary,
            1 => EdgeKind::Boundary,
            2 => {
                let n0 = face_normal(solid, e.faces[0]);
                let n1 = face_normal(solid, e.faces[1]);
                if n0 == Vec3::ZERO || n1 == Vec3::ZERO {
                    EdgeKind::Smooth
                } else {
                    let cos = n0.dot(n1).clamp(-1.0, 1.0);
                    let ang = cos.acos();
                    if ang <= smooth_tol_rad {
                        EdgeKind::Smooth
                    } else {
                        // Edge direction as traversed by faces[0]; the sign of
                        // (n0 x n1) . d distinguishes a ridge from a valley.
                        let d = (solid.vertices[e.v1 as usize] - solid.vertices[e.v0 as usize])
                            .normalize();
                        if n0.cross(n1).dot(d) > 0.0 {
                            e.angle = ang;
                            EdgeKind::Convex
                        } else {
                            e.angle = -ang;
                            EdgeKind::Concave
                        }
                    }
                }
            }
            _ => EdgeKind::NonManifold,
        };
    }
    edges
}

/// Indices (into [`classify_edges`]) of the edges that can be rounded.
pub fn roundable_edges(solid: &Solid) -> Vec<usize> {
    classify_edges(solid)
        .iter()
        .enumerate()
        .filter(|(_, e)| e.is_roundable())
        .map(|(i, _)| i)
        .collect()
}

/// Resolve an `edge_filter`: empty means "every edge".
fn selection(edges: &[EdgeClass], edge_filter: &[usize]) -> Vec<usize> {
    if edge_filter.is_empty() {
        (0..edges.len()).collect()
    } else {
        let mut v: Vec<usize> = edge_filter
            .iter()
            .copied()
            .filter(|i| *i < edges.len())
            .collect();
        v.sort_unstable();
        v.dedup();
        v
    }
}

/// Geometry of a convex edge needed to place cutting planes.
struct EdgeFrame {
    /// A point on the edge line.
    origin: Vec3,
    /// Unit edge direction (as traversed by `faces[0]`).
    dir: Vec3,
    /// Unit outward normals of the two adjacent faces.
    n0: Vec3,
    n1: Vec3,
    /// Unit in-face directions pointing away from the edge into each face.
    t0: Vec3,
    t1: Vec3,
}

fn edge_frame(solid: &Solid, e: &EdgeClass) -> Option<EdgeFrame> {
    if !e.is_roundable() {
        return None;
    }
    let p0 = *solid.vertices.get(e.v0 as usize)?;
    let p1 = *solid.vertices.get(e.v1 as usize)?;
    let dir = (p1 - p0).normalize();
    if dir == Vec3::ZERO {
        return None;
    }
    let n0 = face_normal(solid, e.faces[0]);
    let n1 = face_normal(solid, e.faces[1]);
    if n0 == Vec3::ZERO || n1 == Vec3::ZERO {
        return None;
    }
    let t0 = in_face_direction(n0, dir, face_centroid(solid, e.faces[0]), p0)?;
    let t1 = in_face_direction(n1, dir, face_centroid(solid, e.faces[1]), p0)?;
    Some(EdgeFrame {
        origin: p0,
        dir,
        n0,
        n1,
        t0,
        t1,
    })
}

/// Unit vector lying in the face plane, perpendicular to the edge, pointing
/// away from the edge towards the face interior.
fn in_face_direction(n: Vec3, dir: Vec3, centroid: Vec3, edge_point: Vec3) -> Option<Vec3> {
    let t = n.cross(dir);
    let len = t.length();
    if len < 1e-12 {
        return None;
    }
    let t = t * (1.0 / len);
    if t.dot(centroid - edge_point) >= 0.0 {
        Some(t)
    } else {
        Some(-t)
    }
}

/// Apply one half-space cut, keeping the previous solid if the cut would
/// destroy it (over-large radius, degenerate result, numeric failure).
fn guarded_cut(current: &Solid, point: Vec3, normal: Vec3) -> Solid {
    let candidate = cut_half_space(current, point, normal);
    let v = candidate.volume();
    if candidate.faces.is_empty() || !v.is_finite() || v <= 0.0 {
        current.clone()
    } else {
        candidate
    }
}

/// Chamfer (45°-style bevel) the selected edges of a planar-faced solid.
///
/// `distance` is the setback measured along each adjacent face from the edge;
/// for a 90° dihedral this is the classic symmetric 45° chamfer, and for other
/// dihedrals it is the equal-setback bevel through both offset lines.
///
/// `edge_filter` holds indices into [`classify_edges`]; an empty slice selects
/// every roundable (convex, manifold) edge. Non-convex edges in the filter are
/// silently skipped — see the module docs for the limits.
pub fn chamfer_edges(solid: &Solid, distance: f64, edge_filter: &[usize]) -> Solid {
    if !distance.is_finite() || distance <= 0.0 || solid.faces.is_empty() {
        return solid.clone();
    }
    let edges = classify_edges(solid);
    let picked = selection(&edges, edge_filter);
    let mut out = solid.clone();
    for i in picked {
        let Some(frame) = edge_frame(solid, &edges[i]) else {
            continue;
        };
        // The bevel plane passes through both setback points and contains the
        // edge direction.
        let a = frame.origin + frame.t0 * distance;
        let b = frame.origin + frame.t1 * distance;
        let m = frame.dir.cross(b - a);
        if m.length() < 1e-12 {
            continue;
        }
        let mut m = m.normalize();
        if m.dot(frame.n0 + frame.n1) < 0.0 {
            m = -m;
        }
        out = guarded_cut(&out, a, m);
    }
    out
}

/// Fillet (round) the selected edges of a planar-faced solid, approximating the
/// rolling-ball surface with [`DEFAULT_FILLET_SEGMENTS`] tangent planes.
///
/// See [`fillet_edges_segmented`] to control the facet count.
pub fn fillet_edges(solid: &Solid, radius: f64, edge_filter: &[usize]) -> Solid {
    fillet_edges_segmented(solid, radius, edge_filter, DEFAULT_FILLET_SEGMENTS)
}

/// Fillet with an explicit number of tangent planes per edge.
///
/// For each selected convex edge the engine computes the axis of the inscribed
/// rolling-ball cylinder — the line at distance `radius` from both adjacent
/// face planes — and cuts the solid with `segments` planes tangent to that
/// cylinder, spaced evenly across the dihedral arc. `segments == 1` degenerates
/// to a tangent chamfer; the approximation is circumscribed, so it is always a
/// little proud of the exact arc (error `radius * (1/cos(Δ/2) - 1)`).
pub fn fillet_edges_segmented(
    solid: &Solid,
    radius: f64,
    edge_filter: &[usize],
    segments: usize,
) -> Solid {
    if !radius.is_finite() || radius <= 0.0 || solid.faces.is_empty() {
        return solid.clone();
    }
    let segments = segments.max(1);
    let edges = classify_edges(solid);
    let picked = selection(&edges, edge_filter);
    let mut out = solid.clone();
    for i in picked {
        let Some(frame) = edge_frame(solid, &edges[i]) else {
            continue;
        };
        let cos = frame.n0.dot(frame.n1).clamp(-1.0, 1.0);
        let denom = 1.0 + cos;
        if denom < 1e-9 {
            continue; // 180° fold: no inscribed cylinder
        }
        // Axis point: distance `radius` inside both face planes.
        let center = frame.origin - (frame.n0 + frame.n1) * (radius / denom);
        let ang = cos.acos();
        if ang < 1e-9 {
            continue;
        }
        let sin_ang = ang.sin();
        if sin_ang.abs() < 1e-12 {
            continue;
        }
        for k in 0..segments {
            let t = (k as f64 + 0.5) / segments as f64;
            // Slerp the plane normal from n0 to n1 across the dihedral.
            let m =
                (frame.n0 * ((1.0 - t) * ang).sin() + frame.n1 * (t * ang).sin()) * (1.0 / sin_ang);
            let m = m.normalize();
            if m == Vec3::ZERO {
                continue;
            }
            out = guarded_cut(&out, center + m * radius, m);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::csg_bsp::box_solid;

    fn cube() -> Solid {
        box_solid(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0))
    }

    #[test]
    fn cube_has_eighteen_triangle_edges_twelve_convex() {
        let edges = classify_edges(&cube());
        // 12 box edges + 6 diagonals introduced by triangulation.
        assert_eq!(edges.len(), 18);
        let convex = edges.iter().filter(|e| e.kind == EdgeKind::Convex).count();
        let smooth = edges.iter().filter(|e| e.kind == EdgeKind::Smooth).count();
        assert_eq!(convex, 12, "expected 12 convex box edges");
        assert_eq!(smooth, 6, "expected 6 coplanar face diagonals");
        assert!(edges.iter().all(|e| e.faces.len() == 2));
        for e in edges.iter().filter(|e| e.kind == EdgeKind::Convex) {
            assert!(
                (e.angle - std::f64::consts::FRAC_PI_2).abs() < 1e-9,
                "cube dihedral should be 90 degrees, got {}",
                e.angle
            );
        }
    }

    #[test]
    fn concave_edges_are_detected() {
        // An L-shaped block: cube minus a corner cube leaves concave edges.
        let a = box_solid(Vec3::ZERO, Vec3::new(2.0, 2.0, 2.0));
        let b = box_solid(Vec3::new(1.0, 1.0, -1.0), Vec3::new(3.0, 3.0, 3.0));
        let l = crate::geometry::csg_bsp::bsp_subtract(&a, &b);
        let edges = classify_edges(&l);
        assert!(
            edges.iter().any(|e| e.kind == EdgeKind::Concave),
            "expected at least one concave edge in an L-block"
        );
    }

    #[test]
    fn open_shell_has_boundary_edges() {
        let mut s = Solid::new();
        s.add_triangle(Vec3::ZERO, Vec3::X, Vec3::Y);
        let edges = classify_edges(&s);
        assert_eq!(edges.len(), 3);
        assert!(edges.iter().all(|e| e.kind == EdgeKind::Boundary));
    }

    #[test]
    fn chamfer_cube_removes_expected_volume() {
        let c = cube();
        let d = 0.1;
        let out = chamfer_edges(&c, d, &[]);
        // Each of the 12 edges loses a right-prism wedge (d^2/2 per unit
        // length), and the 8 corners get a small extra cut; check the leading
        // term with a loose tolerance for the corner interaction.
        let approx = 1.0 - 12.0 * (d * d / 2.0);
        assert!(out.volume() < 1.0);
        assert!(
            (out.volume() - approx).abs() < 0.02,
            "chamfered volume {} vs approx {}",
            out.volume(),
            approx
        );
        assert!(out.vertex_count() > c.vertex_count());
    }

    #[test]
    fn fillet_cube_is_between_chamfer_and_original() {
        let c = cube();
        let r = 0.1;
        let filleted = fillet_edges(&c, r, &[]);
        let chamfered = chamfer_edges(&c, r, &[]);
        assert!(filleted.volume() < c.volume());
        // A round removes less material than a full-setback bevel.
        assert!(
            filleted.volume() > chamfered.volume(),
            "fillet {} should keep more material than chamfer {}",
            filleted.volume(),
            chamfered.volume()
        );
        assert!(filleted.vertex_count() > c.vertex_count());
    }

    #[test]
    fn fillet_single_edge_only_touches_that_edge() {
        let c = cube();
        let edges = classify_edges(&c);
        let first = edges
            .iter()
            .position(|e| e.kind == EdgeKind::Convex)
            .unwrap();
        let out = fillet_edges(&c, 0.1, &[first]);
        assert!(out.volume() < c.volume());
        assert!(out.volume() > 0.98, "only one edge should be rounded");
    }

    #[test]
    fn degenerate_inputs_do_not_panic() {
        let c = cube();
        assert_eq!(fillet_edges(&c, 0.0, &[]), c);
        assert_eq!(fillet_edges(&c, -1.0, &[]), c);
        assert_eq!(chamfer_edges(&c, f64::NAN, &[]), c);
        assert_eq!(chamfer_edges(&Solid::new(), 1.0, &[]).triangle_count(), 0);
        // Absurd radius: guarded cuts keep the last non-degenerate solid.
        let huge = fillet_edges(&c, 50.0, &[]);
        assert!(huge.volume().is_finite());
        // Out-of-range filter indices are ignored.
        assert_eq!(chamfer_edges(&c, 0.1, &[9999]), c);
    }
}
