//! BSP-tree triangle-mesh boolean engine (CSG union/subtract/intersect).
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! This is the real boolean engine described in ADR-0013. It implements the
//! classic Naylor/Thibault/Amanatides BSP CSG formulation (the same
//! split/clip/invert dance popularised by `csg.js`) over the kernel's faceted
//! [`Solid`] representation.
//!
//! # How it works
//!
//! 1. Each solid's triangles are lifted into [`Polygon`]s (convex polygons with
//!    a supporting [`Plane`]). Polygons — not triangles — are the working unit
//!    because splitting a convex polygon by a plane yields convex polygons, and
//!    keeping them un-triangulated avoids a cascade of slivers.
//! 2. A [`BspTree`] is built for each operand: every node owns a partition
//!    plane (taken from the first polygon inserted into it), the polygons
//!    *coincident* with that plane, and front/back subtrees holding the
//!    polygons in front of / behind it. Spanning polygons are split and the
//!    pieces recurse into both subtrees.
//! 3. `clip_to` removes the parts of one tree's polygons that fall inside the
//!    other solid; `invert` turns a solid inside out (flipping planes, polygon
//!    winding and swapping front/back). Union, difference and intersection are
//!    all expressed as sequences of those three primitives.
//! 4. The surviving polygons are fan-triangulated and welded back into a
//!    [`Solid`] so the rest of the kernel keeps seeing plain triangles.
//!
//! # Implementation notes
//!
//! The tree is stored in an *arena* (`Vec<BspNode>`, children referenced by
//! index) and every traversal is iterative with an explicit work stack. The
//! textbook recursive formulation blows the native stack on large or
//! adversarially-ordered meshes; an arena keeps the recursion depth off the
//! call stack entirely.
//!
//! # Known limits (see ADR-0013)
//!
//! - Accuracy is tessellation-dependent: this is a *mesh* boolean, not an exact
//!   B-rep boolean. Curved faces are only as round as their facets.
//! - Inputs must be closed, consistently outward-oriented meshes. Non-manifold
//!   or inside-out inputs produce undefined (but non-panicking) output.
//! - Exactly coplanar overlapping faces are handled by the coplanar-front /
//!   coplanar-back split rule, which is correct for the usual cases but can
//!   leave coincident faces on the boundary between the operands.
//! - Near-degenerate (sliver) triangles are dropped on output rather than
//!   repaired.

use crate::geometry::solid::{Face, Solid};
use crate::math::Vec3;
use std::collections::HashMap;

/// Distance below which a point counts as lying *on* a plane.
///
/// Absolute (not relative): kernel models are `f64` and typically sized in
/// millimetres, so 1e-9 sits far above `f64` round-off (~1e-13 at 1e3 mm) while
/// staying far below any meaningful modelling dimension.
pub const PLANE_EPSILON: f64 = 1e-9;

/// Grid quantum used when welding coincident output vertices.
const WELD_QUANTUM: f64 = 1e-9;

/// Squared-area floor below which an output triangle is discarded as a sliver.
const DEGENERATE_CROSS: f64 = 1e-24;

// Vertex/polygon classification bit flags (values chosen so they can be OR-ed).
const COPLANAR: u8 = 0;
const FRONT: u8 = 1;
const BACK: u8 = 2;
const SPANNING: u8 = 3;

/// An oriented plane in Hessian normal form: `dot(normal, x) == w`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plane {
    pub normal: Vec3,
    pub w: f64,
}

impl Plane {
    /// Plane through three points, using the triangle's own normal. Returns
    /// `None` for degenerate (collinear/zero-area) triangles.
    pub fn from_points(a: Vec3, b: Vec3, c: Vec3) -> Option<Plane> {
        let n = (b - a).cross(c - a);
        let len = n.length();
        if !len.is_finite() || len * len < DEGENERATE_CROSS {
            return None;
        }
        let normal = n * (1.0 / len);
        let w = normal.dot(a);
        if !w.is_finite() {
            return None;
        }
        Some(Plane { normal, w })
    }

    /// Signed distance of `p` from the plane (positive on the normal side).
    pub fn distance(&self, p: Vec3) -> f64 {
        self.normal.dot(p) - self.w
    }

    /// Reverse the plane's orientation in place.
    pub fn flip(&mut self) {
        self.normal = -self.normal;
        self.w = -self.w;
    }

    /// Split `poly` by this plane, appending the pieces to `out`.
    ///
    /// A polygon entirely on one side is moved wholesale; a polygon lying in
    /// the plane is sorted into `coplanar_front`/`coplanar_back` by whether its
    /// own normal agrees with this plane's; a spanning polygon is cut in two
    /// along the intersection segment.
    pub fn split_polygon(&self, poly: &Polygon, out: &mut SplitResult) {
        let mut polygon_type: u8 = 0;
        let mut types: Vec<u8> = Vec::with_capacity(poly.vertices.len());
        for v in &poly.vertices {
            let d = self.distance(*v);
            let t = if d < -PLANE_EPSILON {
                BACK
            } else if d > PLANE_EPSILON {
                FRONT
            } else {
                COPLANAR
            };
            polygon_type |= t;
            types.push(t);
        }

        match polygon_type {
            COPLANAR => {
                if self.normal.dot(poly.plane.normal) > 0.0 {
                    out.coplanar_front.push(poly.clone());
                } else {
                    out.coplanar_back.push(poly.clone());
                }
            }
            FRONT => out.front.push(poly.clone()),
            BACK => out.back.push(poly.clone()),
            _ => {
                // SPANNING: walk the loop, emitting each vertex to the side(s)
                // it belongs to and inserting the crossing point on edges that
                // straddle the plane.
                let n = poly.vertices.len();
                let mut f: Vec<Vec3> = Vec::with_capacity(n + 1);
                let mut b: Vec<Vec3> = Vec::with_capacity(n + 1);
                for i in 0..n {
                    let j = (i + 1) % n;
                    let (ti, tj) = (types[i], types[j]);
                    let (vi, vj) = (poly.vertices[i], poly.vertices[j]);
                    if ti != BACK {
                        f.push(vi);
                    }
                    if ti != FRONT {
                        b.push(vi);
                    }
                    if (ti | tj) == SPANNING {
                        let denom = self.normal.dot(vj - vi);
                        if denom.abs() > f64::MIN_POSITIVE {
                            let t = (self.w - self.normal.dot(vi)) / denom;
                            let v = vi + (vj - vi) * t;
                            f.push(v);
                            b.push(v);
                        }
                    }
                }
                if f.len() >= 3 {
                    out.front.push(Polygon::with_plane(f, poly.plane));
                }
                if b.len() >= 3 {
                    out.back.push(Polygon::with_plane(b, poly.plane));
                }
            }
        }
    }
}

/// The four buckets a plane split can produce.
#[derive(Debug, Clone, Default)]
pub struct SplitResult {
    /// Coincident with the plane and facing the same way.
    pub coplanar_front: Vec<Polygon>,
    /// Coincident with the plane and facing the opposite way.
    pub coplanar_back: Vec<Polygon>,
    /// Strictly in front of the plane.
    pub front: Vec<Polygon>,
    /// Strictly behind the plane.
    pub back: Vec<Polygon>,
}

impl SplitResult {
    pub fn is_empty(&self) -> bool {
        self.coplanar_front.is_empty()
            && self.coplanar_back.is_empty()
            && self.front.is_empty()
            && self.back.is_empty()
    }

    fn clear(&mut self) {
        self.coplanar_front.clear();
        self.coplanar_back.clear();
        self.front.clear();
        self.back.clear();
    }
}

/// A convex, planar polygon: the BSP engine's working face primitive.
#[derive(Debug, Clone, PartialEq)]
pub struct Polygon {
    pub vertices: Vec<Vec3>,
    pub plane: Plane,
}

impl Polygon {
    /// Build a polygon from an ordered vertex loop, deriving the plane from the
    /// first non-degenerate vertex triple. `None` if no such triple exists.
    pub fn new(vertices: Vec<Vec3>) -> Option<Polygon> {
        if vertices.len() < 3 {
            return None;
        }
        if vertices.iter().any(|v| !is_finite(*v)) {
            return None;
        }
        let a = vertices[0];
        let mut plane = None;
        for i in 1..vertices.len() - 1 {
            if let Some(p) = Plane::from_points(a, vertices[i], vertices[i + 1]) {
                plane = Some(p);
                break;
            }
        }
        plane.map(|plane| Polygon { vertices, plane })
    }

    /// Build a polygon with an explicit supporting plane (used by splitting, so
    /// the children inherit the parent's plane instead of re-deriving a
    /// slightly different one from near-degenerate fragments).
    pub fn with_plane(vertices: Vec<Vec3>, plane: Plane) -> Polygon {
        Polygon { vertices, plane }
    }

    /// A triangle polygon, or `None` if degenerate.
    pub fn triangle(a: Vec3, b: Vec3, c: Vec3) -> Option<Polygon> {
        Plane::from_points(a, b, c).map(|plane| Polygon {
            vertices: vec![a, b, c],
            plane,
        })
    }

    /// Reverse winding and plane orientation.
    pub fn flip(&mut self) {
        self.vertices.reverse();
        self.plane.flip();
    }

    /// Fan-triangulate the polygon (valid because splits keep polygons convex).
    pub fn triangulate(&self) -> Vec<[Vec3; 3]> {
        let mut tris = Vec::new();
        for i in 1..self.vertices.len().saturating_sub(1) {
            tris.push([self.vertices[0], self.vertices[i], self.vertices[i + 1]]);
        }
        tris
    }

    /// Split this polygon by `plane`.
    pub fn split(&self, plane: &Plane) -> SplitResult {
        let mut out = SplitResult::default();
        plane.split_polygon(self, &mut out);
        out
    }
}

/// One node of the BSP tree.
///
/// Children are arena indices into [`BspTree::nodes`] rather than `Box`es so
/// that build/clip traversals can be iterative (no native-stack recursion).
#[derive(Debug, Clone, Default)]
pub struct BspNode {
    /// The partition plane (taken from the first polygon inserted). `None` for
    /// an empty node.
    pub plane: Option<Plane>,
    /// Polygons coincident with `plane`.
    pub polygons: Vec<Polygon>,
    /// Subtree holding everything in front of `plane`.
    pub front: Option<usize>,
    /// Subtree holding everything behind `plane`.
    pub back: Option<usize>,
}

/// A BSP tree over a set of polygons: the solid's space partition.
#[derive(Debug, Clone, Default)]
pub struct BspTree {
    nodes: Vec<BspNode>,
}

impl BspTree {
    /// An empty tree.
    pub fn new() -> BspTree {
        BspTree { nodes: Vec::new() }
    }

    /// Build a tree from a polygon soup.
    pub fn from_polygons(polygons: Vec<Polygon>) -> BspTree {
        let mut tree = BspTree::new();
        tree.build(polygons);
        tree
    }

    /// Build a tree from a solid's triangles.
    pub fn from_solid(solid: &Solid) -> BspTree {
        BspTree::from_polygons(solid_to_polygons(solid))
    }

    /// Number of nodes in the arena (mostly useful for tests/diagnostics).
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.iter().all(|n| n.polygons.is_empty())
    }

    /// Read-only access to the arena.
    pub fn nodes(&self) -> &[BspNode] {
        &self.nodes
    }

    /// Insert `polygons`, recursively partitioning them into the tree.
    ///
    /// Iterative: the work stack holds `(node, polygons)` pairs, so tree depth
    /// is bounded by heap, not by the call stack.
    pub fn build(&mut self, polygons: Vec<Polygon>) {
        if polygons.is_empty() && !self.nodes.is_empty() {
            return;
        }
        if self.nodes.is_empty() {
            self.nodes.push(BspNode::default());
        }
        let mut split = SplitResult::default();
        let mut stack: Vec<(usize, Vec<Polygon>)> = vec![(0, polygons)];
        while let Some((idx, polys)) = stack.pop() {
            if polys.is_empty() {
                continue;
            }
            if self.nodes[idx].plane.is_none() {
                self.nodes[idx].plane = Some(polys[0].plane);
            }
            let plane = self.nodes[idx].plane.expect("plane set above");
            split.clear();
            for p in &polys {
                plane.split_polygon(p, &mut split);
            }
            // Everything coincident with the partition plane lives at the node.
            self.nodes[idx].polygons.append(&mut split.coplanar_front);
            self.nodes[idx].polygons.append(&mut split.coplanar_back);

            if !split.front.is_empty() {
                let child = self.child_front(idx);
                stack.push((child, std::mem::take(&mut split.front)));
            }
            if !split.back.is_empty() {
                let child = self.child_back(idx);
                stack.push((child, std::mem::take(&mut split.back)));
            }
        }
    }

    fn child_front(&mut self, idx: usize) -> usize {
        match self.nodes[idx].front {
            Some(c) => c,
            None => {
                self.nodes.push(BspNode::default());
                let c = self.nodes.len() - 1;
                self.nodes[idx].front = Some(c);
                c
            }
        }
    }

    fn child_back(&mut self, idx: usize) -> usize {
        match self.nodes[idx].back {
            Some(c) => c,
            None => {
                self.nodes.push(BspNode::default());
                let c = self.nodes.len() - 1;
                self.nodes[idx].back = Some(c);
                c
            }
        }
    }

    /// Turn the solid this tree represents inside out.
    pub fn invert(&mut self) {
        for node in &mut self.nodes {
            for p in &mut node.polygons {
                p.flip();
            }
            if let Some(plane) = &mut node.plane {
                plane.flip();
            }
            std::mem::swap(&mut node.front, &mut node.back);
        }
    }

    /// Remove the parts of `polygons` that lie *inside* this tree's solid,
    /// returning the surviving fragments.
    pub fn clip_polygons(&self, polygons: Vec<Polygon>) -> Vec<Polygon> {
        if self.nodes.is_empty() {
            return polygons;
        }
        let mut result: Vec<Polygon> = Vec::new();
        let mut split = SplitResult::default();
        let mut stack: Vec<(usize, Vec<Polygon>)> = vec![(0, polygons)];
        while let Some((idx, polys)) = stack.pop() {
            if polys.is_empty() {
                continue;
            }
            let plane = match self.nodes[idx].plane {
                Some(p) => p,
                // An empty node bounds nothing: everything survives.
                None => {
                    result.extend(polys);
                    continue;
                }
            };
            split.clear();
            for p in &polys {
                plane.split_polygon(p, &mut split);
            }
            let mut front = std::mem::take(&mut split.front);
            let mut back = std::mem::take(&mut split.back);
            // Coplanar-and-aligned counts as outside, coplanar-and-opposed as
            // inside, matching the reference CSG formulation.
            front.append(&mut split.coplanar_front);
            back.append(&mut split.coplanar_back);

            match self.nodes[idx].front {
                Some(c) => stack.push((c, front)),
                None => result.extend(front),
            }
            // No back subtree means "solid behind this plane": drop `back`.
            if let Some(c) = self.nodes[idx].back {
                stack.push((c, back));
            }
        }
        result
    }

    /// Alias for [`BspTree::clip_polygons`].
    pub fn clip(&self, polygons: Vec<Polygon>) -> Vec<Polygon> {
        self.clip_polygons(polygons)
    }

    /// Remove every polygon of this tree that lies inside `other`.
    pub fn clip_to(&mut self, other: &BspTree) {
        for i in 0..self.nodes.len() {
            let polys = std::mem::take(&mut self.nodes[i].polygons);
            if polys.is_empty() {
                continue;
            }
            self.nodes[i].polygons = other.clip_polygons(polys);
        }
    }

    /// All polygons currently held by the tree.
    pub fn all_polygons(&self) -> Vec<Polygon> {
        let mut out = Vec::new();
        for node in &self.nodes {
            out.extend(node.polygons.iter().cloned());
        }
        out
    }

    /// Reassemble the tree's polygons into a triangle [`Solid`].
    pub fn to_solid(&self) -> Solid {
        polygons_to_solid(&self.all_polygons())
    }
}

/// Build a BSP tree from a solid (free-function spelling used by ADR-0013).
pub fn build(solid: &Solid) -> BspTree {
    BspTree::from_solid(solid)
}

/// Lift a solid's triangles into BSP polygons, skipping degenerate faces.
pub fn solid_to_polygons(solid: &Solid) -> Vec<Polygon> {
    let mut polys = Vec::with_capacity(solid.faces.len());
    let nv = solid.vertices.len() as u32;
    for f in &solid.faces {
        if f.a >= nv || f.b >= nv || f.c >= nv {
            continue;
        }
        let a = solid.vertices[f.a as usize];
        let b = solid.vertices[f.b as usize];
        let c = solid.vertices[f.c as usize];
        if !is_finite(a) || !is_finite(b) || !is_finite(c) {
            continue;
        }
        if let Some(p) = Polygon::triangle(a, b, c) {
            polys.push(p);
        }
    }
    polys
}

/// Fan-triangulate polygons back into a solid, welding coincident vertices and
/// dropping sliver/degenerate triangles.
pub fn polygons_to_solid(polygons: &[Polygon]) -> Solid {
    let mut solid = Solid::new();
    let mut welder = Welder::default();
    for poly in polygons {
        for tri in poly.triangulate() {
            let [a, b, c] = tri;
            if !is_finite(a) || !is_finite(b) || !is_finite(c) {
                continue;
            }
            let cross = (b - a).cross(c - a);
            let area2 = cross.dot(cross);
            if !area2.is_finite() || area2 < DEGENERATE_CROSS {
                continue;
            }
            let ia = welder.weld(&mut solid, a);
            let ib = welder.weld(&mut solid, b);
            let ic = welder.weld(&mut solid, c);
            if ia == ib || ib == ic || ia == ic {
                continue;
            }
            solid.faces.push(Face::new(ia, ib, ic));
        }
    }
    solid
}

/// Spatial-hash vertex welder: O(1) amortised dedup (the `Solid::add_triangle`
/// helper is a linear scan, which is O(n^2) for boolean-sized outputs).
#[derive(Default)]
struct Welder {
    cells: HashMap<(i64, i64, i64), Vec<u32>>,
}

impl Welder {
    fn weld(&mut self, solid: &mut Solid, v: Vec3) -> u32 {
        let key = cell_key(v);
        // Probe the 27 neighbouring cells: two computations of the same point
        // can straddle a cell boundary by a few ULPs.
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    let k = (key.0 + dx, key.1 + dy, key.2 + dz);
                    if let Some(bucket) = self.cells.get(&k) {
                        for &i in bucket {
                            if solid.vertices[i as usize].distance(v) <= WELD_QUANTUM {
                                return i;
                            }
                        }
                    }
                }
            }
        }
        let idx = solid.add_vertex(v);
        self.cells.entry(key).or_default().push(idx);
        idx
    }
}

fn cell_key(v: Vec3) -> (i64, i64, i64) {
    let q = |x: f64| -> i64 {
        let s = x / WELD_QUANTUM;
        if s.is_finite() {
            s.round().clamp(i64::MIN as f64, i64::MAX as f64) as i64
        } else {
            0
        }
    };
    (q(v.x), q(v.y), q(v.z))
}

fn is_finite(v: Vec3) -> bool {
    v.x.is_finite() && v.y.is_finite() && v.z.is_finite()
}

/// Maximum number of healing passes (each pass can split every open edge once).
const HEAL_PASSES: usize = 8;
/// Growth cap for [`heal_t_junctions`], as a multiple of the input face count.
const HEAL_MAX_GROWTH: usize = 8;

fn edge_key(a: u32, b: u32) -> (u32, u32) {
    if a < b {
        (a, b)
    } else {
        (b, a)
    }
}

/// Repair T-junctions: split triangle edges at vertices that lie in their
/// interior, so a mesh that is *geometrically* closed also becomes
/// *combinatorially* closed (every edge shared by exactly two faces).
///
/// BSP splitting is one-sided — when one side of a shared edge is subdivided
/// and the other is not, the result is a T-junction: no gap in area, but an
/// edge used by a single face. Slicing, edge classification and STL consumers
/// all prefer them gone.
///
/// Only *open* edges (used by exactly one face) are considered, and only
/// vertices already sitting on such an edge are candidate split points, so a
/// mesh that is already manifold is returned untouched. Input indices are
/// assumed welded (as produced by [`polygons_to_solid`]).
pub fn heal_t_junctions(solid: &Solid, tol: f64) -> Solid {
    if solid.faces.len() < 2 {
        return solid.clone();
    }
    let verts = &solid.vertices;
    let cap = solid.faces.len() * HEAL_MAX_GROWTH + 64;
    let mut faces = solid.faces.clone();

    for _pass in 0..HEAL_PASSES {
        let mut counts: HashMap<(u32, u32), u32> = HashMap::new();
        for f in &faces {
            let idx = f.indices();
            for k in 0..3 {
                *counts
                    .entry(edge_key(idx[k], idx[(k + 1) % 3]))
                    .or_insert(0) += 1;
            }
        }
        let open: Vec<(u32, u32)> = counts
            .iter()
            .filter(|(_, c)| **c == 1)
            .map(|(k, _)| *k)
            .collect();
        if open.is_empty() {
            break;
        }
        let open_set: std::collections::HashSet<(u32, u32)> = open.iter().copied().collect();
        let mut candidates: Vec<u32> = open.iter().flat_map(|(a, b)| [*a, *b]).collect();
        candidates.sort_unstable();
        candidates.dedup();

        let mut next: Vec<Face> = Vec::with_capacity(faces.len());
        let mut changed = false;
        for f in &faces {
            if next.len() >= cap {
                next.push(*f);
                continue;
            }
            let idx = f.indices();
            let mut found = None;
            for k in 0..3 {
                let (ia, ib) = (idx[k], idx[(k + 1) % 3]);
                if !open_set.contains(&edge_key(ia, ib)) {
                    continue;
                }
                if let Some(v) = vertex_inside_segment(verts, ia, ib, &idx, &candidates, tol) {
                    found = Some((k, v));
                    break;
                }
            }
            match found {
                Some((k, v)) => {
                    changed = true;
                    let (t0, t1, t2) = (idx[0], idx[1], idx[2]);
                    match k {
                        0 => {
                            next.push(Face::new(t0, v, t2));
                            next.push(Face::new(v, t1, t2));
                        }
                        1 => {
                            next.push(Face::new(t0, t1, v));
                            next.push(Face::new(t0, v, t2));
                        }
                        _ => {
                            next.push(Face::new(t0, t1, v));
                            next.push(Face::new(v, t1, t2));
                        }
                    }
                }
                None => next.push(*f),
            }
        }
        faces = next;
        if !changed {
            break;
        }
    }

    Solid {
        vertices: verts.clone(),
        faces,
    }
}

/// The candidate vertex nearest the start of segment `ia -> ib` that lies
/// strictly inside it (within `tol` of the line and `tol` from either end).
fn vertex_inside_segment(
    verts: &[Vec3],
    ia: u32,
    ib: u32,
    exclude: &[u32; 3],
    candidates: &[u32],
    tol: f64,
) -> Option<u32> {
    let a = verts[ia as usize];
    let b = verts[ib as usize];
    let d = b - a;
    let len2 = d.dot(d);
    if len2 <= tol * tol {
        return None;
    }
    let len = len2.sqrt();
    let mut best: Option<(f64, u32)> = None;
    for &c in candidates {
        if exclude.contains(&c) {
            continue;
        }
        let p = verts[c as usize];
        let t = d.dot(p - a) / len2;
        if t * len <= tol || (1.0 - t) * len <= tol {
            continue;
        }
        if (p - (a + d * t)).length() > tol {
            continue;
        }
        if best.map(|(bt, _)| t < bt).unwrap_or(true) {
            best = Some((t, c));
        }
    }
    best.map(|(_, c)| c)
}

/// True when the two solids' axis-aligned bounds do not overlap (touching
/// counts as separated).
fn bounds_separated(a: &Solid, b: &Solid) -> bool {
    let (Some((amin, amax)), Some((bmin, bmax))) = (a.bounds(), b.bounds()) else {
        return true;
    };
    let e = PLANE_EPSILON;
    amax.x <= bmin.x + e
        || bmax.x <= amin.x + e
        || amax.y <= bmin.y + e
        || bmax.y <= amin.y + e
        || amax.z <= bmin.z + e
        || bmax.z <= amin.z + e
}

/// Boolean union `A ∪ B` over triangle meshes.
///
/// Disjoint operands take a fast path (plain concatenation), which is both
/// faster and lossless — the BSP path would needlessly re-split every face.
pub fn bsp_union(a: &Solid, b: &Solid) -> Solid {
    if a.faces.is_empty() {
        return b.clone();
    }
    if b.faces.is_empty() {
        return a.clone();
    }
    if bounds_separated(a, b) {
        let mut out = a.clone();
        out.extend(b);
        return out;
    }
    let mut ta = BspTree::from_solid(a);
    let mut tb = BspTree::from_solid(b);
    ta.clip_to(&tb);
    tb.clip_to(&ta);
    tb.invert();
    tb.clip_to(&ta);
    tb.invert();
    ta.build(tb.all_polygons());
    heal_t_junctions(&ta.to_solid(), PLANE_EPSILON)
}

/// Boolean difference `A − B` over triangle meshes.
pub fn bsp_subtract(a: &Solid, b: &Solid) -> Solid {
    if a.faces.is_empty() {
        return Solid::new();
    }
    if b.faces.is_empty() || bounds_separated(a, b) {
        return a.clone();
    }
    let mut ta = BspTree::from_solid(a);
    let mut tb = BspTree::from_solid(b);
    ta.invert();
    ta.clip_to(&tb);
    tb.clip_to(&ta);
    tb.invert();
    tb.clip_to(&ta);
    tb.invert();
    ta.build(tb.all_polygons());
    ta.invert();
    heal_t_junctions(&ta.to_solid(), PLANE_EPSILON)
}

/// Boolean intersection `A ∩ B` over triangle meshes.
pub fn bsp_intersect(a: &Solid, b: &Solid) -> Solid {
    if a.faces.is_empty() || b.faces.is_empty() || bounds_separated(a, b) {
        return Solid::new();
    }
    let mut ta = BspTree::from_solid(a);
    let mut tb = BspTree::from_solid(b);
    ta.invert();
    tb.clip_to(&ta);
    tb.invert();
    ta.clip_to(&tb);
    tb.clip_to(&ta);
    ta.build(tb.all_polygons());
    ta.invert();
    heal_t_junctions(&ta.to_solid(), PLANE_EPSILON)
}

/// An axis-aligned box solid with outward-facing triangles.
pub fn box_solid(min: Vec3, max: Vec3) -> Solid {
    parallelepiped(
        min,
        Vec3::new(max.x - min.x, 0.0, 0.0),
        Vec3::new(0.0, max.y - min.y, 0.0),
        Vec3::new(0.0, 0.0, max.z - min.z),
    )
}

/// A (possibly oblique) box spanned by `origin` and three edge vectors, with
/// outward-facing triangles regardless of the frame's handedness.
pub fn parallelepiped(origin: Vec3, e1: Vec3, e2: Vec3, e3: Vec3) -> Solid {
    let mut s = Solid::new();
    let corners = [
        origin,                // 0
        origin + e1,           // 1
        origin + e1 + e2,      // 2
        origin + e2,           // 3
        origin + e3,           // 4
        origin + e1 + e3,      // 5
        origin + e1 + e2 + e3, // 6
        origin + e2 + e3,      // 7
    ];
    for c in corners {
        s.add_vertex(c);
    }
    // Faces wound consistently for a right-handed (e1, e2, e3) frame.
    const QUADS: [[u32; 4]; 6] = [
        [0, 3, 2, 1], // -e3
        [4, 5, 6, 7], // +e3
        [0, 1, 5, 4], // -e2
        [3, 7, 6, 2], // +e2
        [0, 4, 7, 3], // -e1
        [1, 2, 6, 5], // +e1
    ];
    for q in QUADS {
        s.faces.push(Face::new(q[0], q[1], q[2]));
        s.faces.push(Face::new(q[0], q[2], q[3]));
    }
    // A left-handed frame produces an inside-out shell; flip it so the boolean
    // engine always sees outward normals.
    if s.volume() < 0.0 {
        s.reverse_winding();
    }
    s
}

/// Intersect `solid` with the half-space `dot(normal, x - point) <= 0`
/// (i.e. cut the solid with a plane and keep the material behind it).
///
/// Implemented as a boolean intersection against a box large enough to cover
/// the solid, so the cut face is properly capped and the result stays a closed
/// mesh. Returns the input unchanged if nothing lies in front of the plane.
pub fn cut_half_space(solid: &Solid, point: Vec3, normal: Vec3) -> Solid {
    let n = normal.normalize();
    if n == Vec3::ZERO || !is_finite(n) || !is_finite(point) || solid.faces.is_empty() {
        return solid.clone();
    }
    let Some((min, max)) = solid.bounds() else {
        return solid.clone();
    };
    // Nothing on the discard side => no cut required. (A non-finite maximum
    // means an empty or corrupt vertex list; leave the solid alone.)
    let max_d = solid
        .vertices
        .iter()
        .map(|v| n.dot(*v - point))
        .fold(f64::NEG_INFINITY, f64::max);
    if !max_d.is_finite() || max_d <= PLANE_EPSILON {
        return solid.clone();
    }
    let center = (min + max) * 0.5;
    let diag = (max - min).length().max(1.0);
    let l = diag * 4.0;
    let u = perpendicular(n);
    let v = n.cross(u).normalize();
    // Anchor the box on the plane, spanning it laterally and extending well
    // past the solid on the keep side (-n).
    let anchor = center - n * n.dot(center - point);
    let origin = anchor - u * l - v * l;
    let keep_box = parallelepiped(origin, u * (2.0 * l), v * (2.0 * l), -n * (2.0 * l));
    bsp_intersect(solid, &keep_box)
}

/// Any unit vector perpendicular to `n` (assumed unit length).
fn perpendicular(n: Vec3) -> Vec3 {
    let axis = if n.x.abs() < 0.9 { Vec3::X } else { Vec3::Y };
    let p = axis.cross(n);
    let len = p.length();
    if len < 1e-12 {
        Vec3::Z.cross(n).normalize()
    } else {
        p * (1.0 / len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit_cube(offset: f64) -> Solid {
        box_solid(
            Vec3::new(offset, offset, offset),
            Vec3::new(offset + 1.0, offset + 1.0, offset + 1.0),
        )
    }

    #[test]
    fn box_solid_is_outward_and_unit_volume() {
        let c = unit_cube(0.0);
        assert_eq!(c.triangle_count(), 12);
        assert!((c.volume() - 1.0).abs() < 1e-12, "volume {}", c.volume());
        assert!((c.surface_area() - 6.0).abs() < 1e-12);
    }

    #[test]
    fn plane_splits_spanning_triangle() {
        let poly = Polygon::triangle(
            Vec3::new(-1.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        )
        .unwrap();
        let plane = Plane {
            normal: Vec3::X,
            w: 0.0,
        };
        let r = poly.split(&plane);
        assert_eq!(r.front.len(), 1);
        assert_eq!(r.back.len(), 1);
        assert!(r.coplanar_front.is_empty() && r.coplanar_back.is_empty());
    }

    #[test]
    fn plane_sorts_coplanar_polygon() {
        let poly = Polygon::triangle(Vec3::ZERO, Vec3::X, Vec3::Y).unwrap();
        let plane = poly.plane;
        let r = poly.split(&plane);
        assert_eq!(r.coplanar_front.len(), 1);
        let mut flipped = plane;
        flipped.flip();
        let r2 = poly.split(&flipped);
        assert_eq!(r2.coplanar_back.len(), 1);
    }

    #[test]
    fn tree_roundtrip_preserves_volume() {
        let cube = unit_cube(0.0);
        let tree = BspTree::from_solid(&cube);
        let back = tree.to_solid();
        assert!(
            (back.volume() - 1.0).abs() < 1e-9,
            "volume {}",
            back.volume()
        );
    }

    #[test]
    fn invert_twice_is_identity_on_volume() {
        let cube = unit_cube(0.0);
        let mut tree = BspTree::from_solid(&cube);
        tree.invert();
        assert!((tree.to_solid().volume() + 1.0).abs() < 1e-9);
        tree.invert();
        assert!((tree.to_solid().volume() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn union_of_overlapping_cubes() {
        let a = unit_cube(0.0);
        let b = unit_cube(0.5);
        let u = bsp_union(&a, &b);
        // 1 + 1 - 0.5^3 overlap.
        assert!((u.volume() - 1.875).abs() < 1e-6, "volume {}", u.volume());
    }

    #[test]
    fn subtract_overlapping_cubes() {
        let a = unit_cube(0.0);
        let b = unit_cube(0.5);
        let d = bsp_subtract(&a, &b);
        assert!((d.volume() - 0.875).abs() < 1e-6, "volume {}", d.volume());
    }

    #[test]
    fn intersect_overlapping_cubes() {
        let a = unit_cube(0.0);
        let b = unit_cube(0.5);
        let i = bsp_intersect(&a, &b);
        assert!((i.volume() - 0.125).abs() < 1e-6, "volume {}", i.volume());
    }

    #[test]
    fn subtract_enclosing_solid_is_empty() {
        let a = unit_cube(0.0);
        let big = box_solid(Vec3::new(-1.0, -1.0, -1.0), Vec3::new(2.0, 2.0, 2.0));
        let d = bsp_subtract(&a, &big);
        assert!(d.volume().abs() < 1e-9, "volume {}", d.volume());
    }

    #[test]
    fn subtract_inner_cavity_keeps_shell_volume() {
        let a = box_solid(Vec3::ZERO, Vec3::new(4.0, 4.0, 4.0));
        let b = box_solid(Vec3::new(1.0, 1.0, 1.0), Vec3::new(2.0, 2.0, 2.0));
        let d = bsp_subtract(&a, &b);
        // 64 minus a fully-enclosed 1x1x1 void.
        assert!((d.volume() - 63.0).abs() < 1e-6, "volume {}", d.volume());
    }

    #[test]
    fn disjoint_operands_take_fast_paths() {
        let a = unit_cube(0.0);
        let b = unit_cube(5.0);
        assert_eq!(
            bsp_union(&a, &b).triangle_count(),
            a.triangle_count() + b.triangle_count()
        );
        assert_eq!(bsp_subtract(&a, &b), a);
        assert_eq!(bsp_intersect(&a, &b).triangle_count(), 0);
    }

    #[test]
    fn empty_operands_are_handled() {
        let a = unit_cube(0.0);
        let empty = Solid::new();
        assert_eq!(bsp_union(&a, &empty), a);
        assert_eq!(bsp_union(&empty, &a), a);
        assert_eq!(bsp_subtract(&a, &empty), a);
        assert_eq!(bsp_subtract(&empty, &a).triangle_count(), 0);
        assert_eq!(bsp_intersect(&a, &empty).triangle_count(), 0);
    }

    #[test]
    fn cut_half_space_halves_a_cube() {
        let a = unit_cube(0.0);
        let half = cut_half_space(&a, Vec3::new(0.5, 0.0, 0.0), Vec3::X);
        assert!(
            (half.volume() - 0.5).abs() < 1e-6,
            "volume {}",
            half.volume()
        );
        // A plane entirely clear of the solid leaves it untouched.
        let untouched = cut_half_space(&a, Vec3::new(5.0, 0.0, 0.0), Vec3::X);
        assert_eq!(untouched.triangle_count(), a.triangle_count());
    }

    #[test]
    fn boolean_output_has_no_nan() {
        let a = unit_cube(0.0);
        let b = unit_cube(0.3);
        for s in [
            bsp_union(&a, &b),
            bsp_subtract(&a, &b),
            bsp_intersect(&a, &b),
        ] {
            assert!(s.vertices.iter().all(|v| is_finite(*v)));
        }
    }
}
