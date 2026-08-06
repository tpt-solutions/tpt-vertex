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
//! selected edge gets a *cutting tool* — the prism between the edge and its
//! blend surface — and the tools are subtracted from the solid. Tools with
//! disjoint bounds are merged so one boolean can round many edges at once.
//!
//! # Known limits
//!
//! - **Convex edges only.** A subtractive tool can only *remove* material, so
//!   concave edges (which need material *added* by the rolling ball) are
//!   skipped, as are smooth, boundary and non-manifold edges.
//! - **Faceted fillets.** The rolling-ball surface is approximated by
//!   [`DEFAULT_FILLET_SEGMENTS`] facets whose corners lie on the exact fillet
//!   cylinder, so the blend is inscribed in (never proud of) the true surface.
//!   Raise the segment count via [`fillet_edges_segmented`] to converge.
//! - **Corners are overlaps, not blends.** Each tool overshoots its edge
//!   slightly so the corner is fully reached; where several rounded edges meet,
//!   their tools simply overlap. That is close to, but not exactly, the
//!   spherical corner patch an exact kernel would build.
//! - **Radius/distance must be small relative to the feature.** Nothing checks
//!   that the blend still fits on the adjacent faces; an over-large radius will
//!   cut past them. Cuts that would empty the solid are skipped rather than
//!   applied.
//! - **Cost is one boolean per batch of disjoint edges.** Rounding *every* edge
//!   of a dense mesh is expensive and is bounded by
//!   [`ROUNDING_EDGE_BUDGET`]/[`ROUNDING_TRIANGLE_BUDGET`]; past the budget the
//!   remaining edges are left sharp.
//! - **Planar-faced solids.** Classification assumes planar faces; on a
//!   tessellated cylinder each facet boundary is a genuine (small) convex edge
//!   unless it falls inside the smoothness tolerance.

use crate::geometry::csg_bsp::{bsp_subtract_raw, heal_t_junctions, PLANE_EPSILON};
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

/// Outward inflation of a cutting tool past the adjacent faces, as a fraction
/// of the radius/setback. Keeps the tool's flat sides from being *exactly*
/// coplanar with the solid's faces, which is the BSP engine's fragile case.
const TOOL_INFLATE: f64 = 0.25;

/// Axial overshoot of a cutting tool past each end of the edge, as a fraction
/// of the radius/setback. Guarantees the corner is reached (and keeps the end
/// caps off the faces met there); adjacent tools simply overlap at corners.
const TOOL_OVERSHOOT: f64 = 0.5;

/// Build the cutting tool for one edge: the material between the edge and the
/// blend surface, swept along the edge.
///
/// `profile` is the blend cross-section, ordered from a point on face 0 to a
/// point on face 1 (two points for a chamfer, an arc polyline for a fillet).
/// The tool is that cross-section closed back through the edge corner,
/// inflated outward by `size * TOOL_INFLATE` so none of its faces are coplanar
/// with the solid, and extruded along the edge with an overshoot at both ends.
fn edge_tool(frame: &EdgeFrame, length: f64, profile: &[Vec3], size: f64) -> Option<Solid> {
    if profile.len() < 2 || !length.is_finite() || length <= 0.0 {
        return None;
    }
    let delta = size * TOOL_INFLATE;
    let ext = size * TOOL_OVERSHOOT;
    let first = *profile.first()?;
    let last = *profile.last()?;

    // Cross-section loop: outward collar (on face 0, over the corner, on
    // face 1) then back along the blend profile.
    let mut cross: Vec<Vec3> = Vec::with_capacity(profile.len() + 3);
    cross.push(first + frame.n0 * delta);
    cross.push(frame.origin + (frame.n0 + frame.n1) * delta); // the apex
    cross.push(last + frame.n1 * delta);
    cross.extend(profile.iter().rev().copied());
    if cross
        .iter()
        .any(|p| !p.x.is_finite() || !p.y.is_finite() || !p.z.is_finite())
    {
        return None;
    }

    const APEX: usize = 1;
    let n = cross.len();
    let mut tool = Solid::new();
    let base: Vec<u32> = cross
        .iter()
        .map(|p| tool.add_vertex(*p - frame.dir * ext))
        .collect();
    let top: Vec<u32> = cross
        .iter()
        .map(|p| tool.add_vertex(*p + frame.dir * (length + ext)))
        .collect();

    for i in 0..n {
        let j = (i + 1) % n;
        tool.faces
            .push(crate::geometry::solid::Face::new(base[i], base[j], top[j]));
        tool.faces
            .push(crate::geometry::solid::Face::new(base[i], top[j], top[i]));
    }
    // The cross-section is star-shaped about the apex, so both caps fan from it.
    for i in 0..n {
        let j = (i + 1) % n;
        if i == APEX || j == APEX {
            continue;
        }
        tool.faces.push(crate::geometry::solid::Face::new(
            base[APEX], base[j], base[i],
        ));
        tool.faces
            .push(crate::geometry::solid::Face::new(top[APEX], top[i], top[j]));
    }

    let vol = tool.volume();
    if !vol.is_finite() || vol.abs() < 1e-15 {
        return None;
    }
    if vol < 0.0 {
        tool.reverse_winding();
    }
    Some(tool)
}

/// Subtract one cutting tool, keeping the previous solid if the cut would
/// destroy it (over-large radius, degenerate result, numeric failure).
///
/// Uses the *raw* (unhealed) boolean: healing between cuts multiplies the
/// triangle count without improving the final mesh, so rounding heals once at
/// the end instead.
fn guarded_subtract(current: &Solid, tool: &Solid) -> Solid {
    let candidate = bsp_subtract_raw(current, tool);
    let v = candidate.volume();
    if candidate.faces.is_empty() || !v.is_finite() || v <= 0.0 {
        current.clone()
    } else {
        candidate
    }
}

/// Maximum number of edges rounded by a single [`fillet_edges`] /
/// [`chamfer_edges`] call. Each edge costs a boolean, so an unbounded selection
/// on a dense mesh would run effectively forever; past the budget the remaining
/// edges are left sharp rather than hanging the caller.
pub const ROUNDING_EDGE_BUDGET: usize = 1024;

/// Rounding stops once the working mesh passes this triangle count.
pub const ROUNDING_TRIANGLE_BUDGET: usize = 200_000;

fn aabb_disjoint(a: (Vec3, Vec3), b: (Vec3, Vec3)) -> bool {
    a.1.x <= b.0.x
        || b.1.x <= a.0.x
        || a.1.y <= b.0.y
        || b.1.y <= a.0.y
        || a.1.z <= b.0.z
        || b.1.z <= a.0.z
}

/// Merge cutting tools with pairwise-disjoint bounds into single multi-part
/// cutters, so one boolean can round many edges at once.
///
/// Concatenating *overlapping* closed meshes would produce a self-intersecting
/// cutter (undefined for the BSP engine), so tools that touch are kept in
/// separate batches; only bounding-box-disjoint tools are merged.
fn batch_tools(tools: Vec<Solid>) -> Vec<Solid> {
    let mut batches: Vec<(Vec<(Vec3, Vec3)>, Solid)> = Vec::new();
    'next: for tool in tools {
        let Some(tb) = tool.bounds() else {
            continue;
        };
        for (boxes, batch) in batches.iter_mut() {
            if boxes.iter().all(|b| aabb_disjoint(*b, tb)) {
                boxes.push(tb);
                batch.extend(&tool);
                continue 'next;
            }
        }
        batches.push((vec![tb], tool));
    }
    batches.into_iter().map(|(_, s)| s).collect()
}

/// Subtract every cutting tool from `solid` and heal the result once.
fn apply_tools(solid: &Solid, tools: Vec<Solid>) -> Solid {
    if tools.is_empty() {
        return solid.clone();
    }
    let mut out = solid.clone();
    for batch in batch_tools(tools) {
        if out.triangle_count() > ROUNDING_TRIANGLE_BUDGET {
            break;
        }
        out = guarded_subtract(&out, &batch);
    }
    heal_t_junctions(&out, PLANE_EPSILON)
}

/// Chamfer (bevel) the selected edges of a planar-faced solid.
///
/// `distance` is the setback measured along each adjacent face from the edge;
/// for a 90° dihedral this is the classic symmetric 45° chamfer, and for other
/// dihedrals it is the equal-setback bevel through both offset lines. Each
/// selected edge is realised as one boolean subtraction of a local prism tool.
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
    let mut tools = Vec::new();
    for i in picked {
        if tools.len() >= ROUNDING_EDGE_BUDGET {
            break;
        }
        let Some(frame) = edge_frame(solid, &edges[i]) else {
            continue;
        };
        let profile = [
            frame.origin + frame.t0 * distance,
            frame.origin + frame.t1 * distance,
        ];
        if let Some(tool) = edge_tool(&frame, edges[i].length, &profile, distance) {
            tools.push(tool);
        }
    }
    apply_tools(solid, tools)
}

/// Fillet (round) the selected edges of a planar-faced solid, approximating the
/// rolling-ball surface with [`DEFAULT_FILLET_SEGMENTS`] facets.
///
/// See [`fillet_edges_segmented`] to control the facet count.
pub fn fillet_edges(solid: &Solid, radius: f64, edge_filter: &[usize]) -> Solid {
    fillet_edges_segmented(solid, radius, edge_filter, DEFAULT_FILLET_SEGMENTS)
}

/// Fillet with an explicit number of facets per edge.
///
/// For each selected convex edge the engine computes the axis of the inscribed
/// rolling-ball cylinder — the line at distance `radius` from both adjacent
/// face planes — samples the tangent arc between the two faces into `segments`
/// facets, and subtracts the prism between the edge and that arc. `segments ==
/// 1` degenerates to a tangent chamfer. The arc samples lie *on* the exact
/// cylinder, so the facetted blend is inscribed in (never proud of) the true
/// rolling-ball surface; raise the segment count to converge on it.
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
    let mut tools = Vec::new();
    for i in picked {
        if tools.len() >= ROUNDING_EDGE_BUDGET {
            break;
        }
        let Some(frame) = edge_frame(solid, &edges[i]) else {
            continue;
        };
        let cos = frame.n0.dot(frame.n1).clamp(-1.0, 1.0);
        let denom = 1.0 + cos;
        if denom < 1e-9 {
            continue; // 180° fold: no inscribed cylinder
        }
        // Axis of the rolling ball: distance `radius` inside both face planes.
        let center = frame.origin - (frame.n0 + frame.n1) * (radius / denom);
        let ang = cos.acos();
        let sin_ang = ang.sin();
        if ang < 1e-9 || sin_ang.abs() < 1e-12 {
            continue;
        }
        // Sample the tangent arc from the face-0 tangent point to the face-1 one.
        let mut profile = Vec::with_capacity(segments + 1);
        for k in 0..=segments {
            let t = k as f64 / segments as f64;
            let m =
                (frame.n0 * ((1.0 - t) * ang).sin() + frame.n1 * (t * ang).sin()) * (1.0 / sin_ang);
            let m = m.normalize();
            if m == Vec3::ZERO {
                profile.clear();
                break;
            }
            profile.push(center + m * radius);
        }
        if profile.len() < 2 {
            continue;
        }
        if let Some(tool) = edge_tool(&frame, edges[i].length, &profile, radius) {
            tools.push(tool);
        }
    }
    apply_tools(solid, tools)
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

    /// Edges used by exactly one face.
    fn open_edges(s: &Solid) -> usize {
        use std::collections::HashMap;
        let mut counts: HashMap<(u32, u32), u32> = HashMap::new();
        for f in &s.faces {
            let idx = f.indices();
            for k in 0..3 {
                let (a, b) = (idx[k], idx[(k + 1) % 3]);
                *counts
                    .entry(if a < b { (a, b) } else { (b, a) })
                    .or_insert(0) += 1;
            }
        }
        counts.values().filter(|c| **c == 1).count()
    }

    #[test]
    fn rounded_solids_stay_closed_and_manifold() {
        let c = cube();
        for (label, s) in [
            ("fillet", fillet_edges(&c, 0.1, &[])),
            ("chamfer", chamfer_edges(&c, 0.1, &[])),
            ("big fillet", fillet_edges(&c, 0.3, &[])),
            ("deep chamfer", chamfer_edges(&c, 0.45, &[])),
        ] {
            assert_eq!(open_edges(&s), 0, "{label} left open edges");
            assert!(s.volume() > 0.0 && s.volume() < 1.0, "{label} volume");
        }
    }

    #[test]
    fn fillet_converges_towards_the_exact_rolling_ball_volume() {
        // Exact volume of a unit cube with radius-r rounds on its 12 edges,
        // ignoring the corner blends: each edge loses a (1 - pi/4) r^2 prism
        // over the (1 - 2r) length that is not shared with a corner.
        let r: f64 = 0.1;
        let edge_loss = 12.0 * (1.0 - std::f64::consts::FRAC_PI_4) * r * r * (1.0 - 2.0 * r);
        let target = 1.0 - edge_loss;
        let coarse = fillet_edges_segmented(&cube(), r, &[], 2).volume();
        let fine = fillet_edges_segmented(&cube(), r, &[], 8).volume();
        // Inscribed facets remove slightly too much; refining reduces the gap.
        assert!(coarse < fine, "refining should keep more material");
        assert!(
            (fine - target).abs() < (coarse - target).abs(),
            "fine {fine} should be closer to {target} than coarse {coarse}"
        );
        assert!((fine - target).abs() < 0.005, "fine fillet volume {fine}");
    }

    #[test]
    fn tool_batching_matches_sequential_cuts() {
        // Two far-apart cubes: their edge tools are disjoint and get merged
        // into batches, which must give the same result as cutting one by one.
        let mut two = cube();
        two.extend(&box_solid(
            Vec3::new(4.0, 0.0, 0.0),
            Vec3::new(5.0, 1.0, 1.0),
        ));
        let out = chamfer_edges(&two, 0.1, &[]);
        let single = chamfer_edges(&cube(), 0.1, &[]);
        assert!(
            (out.volume() - 2.0 * single.volume()).abs() < 1e-9,
            "batched volume {} vs 2x {}",
            out.volume(),
            single.volume()
        );
        assert_eq!(open_edges(&out), 0);
    }
}
