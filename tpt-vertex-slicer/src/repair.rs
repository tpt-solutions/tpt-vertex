//! Mesh repair / manifold-checking pre-pass, run before slicing to make
//! marginal meshes (duplicate vertices, degenerate triangles, or a handful of
//! non-manifold edges) slice more robustly.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! This is a basic pass, not a general-purpose mesh-healing library: it welds
//! near-duplicate vertices, drops degenerate (zero-area or repeated-vertex)
//! triangles, and reports non-manifold edges (edges shared by other than
//! exactly two triangles) for diagnostics. It does not attempt to close holes
//! or resolve self-intersections.

use std::collections::HashMap;
use tpt_vertex_kernel::geometry::solid::{Face, Solid};
use tpt_vertex_kernel::math::Vec3;

/// Diagnostics produced by a repair pass.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RepairReport {
    pub vertices_welded: usize,
    pub degenerate_faces_removed: usize,
    /// Edges shared by other than exactly two triangles, after welding and
    /// degenerate-face removal. A watertight, 2-manifold mesh has zero.
    pub non_manifold_edges: usize,
}

/// Weld vertices within `epsilon` millimetres of each other and drop
/// degenerate triangles, returning the cleaned solid and a diagnostic report.
pub fn repair_mesh(solid: &Solid, epsilon: f64) -> (Solid, RepairReport) {
    let mut report = RepairReport::default();
    let cell = epsilon.max(1e-9);

    // 1. Weld near-duplicate vertices via a spatial hash on an `epsilon`-sized
    // grid (a vertex snaps to the first vertex seen in its cell).
    let key = |v: Vec3| -> (i64, i64, i64) {
        (
            (v.x / cell).round() as i64,
            (v.y / cell).round() as i64,
            (v.z / cell).round() as i64,
        )
    };
    let mut grid: HashMap<(i64, i64, i64), u32> = HashMap::new();
    let mut remap: Vec<u32> = Vec::with_capacity(solid.vertices.len());
    let mut new_vertices: Vec<Vec3> = Vec::new();
    for &v in &solid.vertices {
        let k = key(v);
        if let Some(&idx) = grid.get(&k) {
            remap.push(idx);
            report.vertices_welded += 1;
        } else {
            let idx = new_vertices.len() as u32;
            new_vertices.push(v);
            grid.insert(k, idx);
            remap.push(idx);
        }
    }

    // 2. Remap faces, dropping degenerate (repeated-vertex or near-zero-area)
    // triangles.
    let mut new_faces = Vec::with_capacity(solid.faces.len());
    for f in &solid.faces {
        let (a, b, c) = (remap[f.a as usize], remap[f.b as usize], remap[f.c as usize]);
        if a == b || b == c || a == c {
            report.degenerate_faces_removed += 1;
            continue;
        }
        let (va, vb, vc) = (
            new_vertices[a as usize],
            new_vertices[b as usize],
            new_vertices[c as usize],
        );
        let area2 = (vb - va).cross(vc - va).length();
        if area2 < 1e-12 {
            report.degenerate_faces_removed += 1;
            continue;
        }
        new_faces.push(Face::new(a, b, c));
    }

    // 3. Diagnose non-manifold edges (undirected edges shared by != 2 tris).
    let mut edge_counts: HashMap<(u32, u32), usize> = HashMap::new();
    for f in &new_faces {
        for (p, q) in [(f.a, f.b), (f.b, f.c), (f.c, f.a)] {
            let k = if p < q { (p, q) } else { (q, p) };
            *edge_counts.entry(k).or_insert(0) += 1;
        }
    }
    report.non_manifold_edges = edge_counts.values().filter(|&&c| c != 2).count();

    let mut out = Solid::new();
    out.vertices = new_vertices;
    out.faces = new_faces;
    (out, report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube_with_duplicate_vertices() -> Solid {
        let mut s = Solid::new();
        let mut v = |x: f64, y: f64, z: f64| s.add_vertex(Vec3::new(x, y, z));
        let p = [
            v(0.0, 0.0, 0.0), v(1.0, 0.0, 0.0), v(1.0, 1.0, 0.0), v(0.0, 1.0, 0.0),
            v(0.0, 0.0, 1.0), v(1.0, 0.0, 1.0), v(1.0, 1.0, 1.0), v(0.0, 1.0, 1.0),
        ];
        // Duplicate a vertex (within welding tolerance of an existing one),
        // added before the `f` closure below so both closures don't need to
        // borrow `s` mutably at the same time.
        let dup = v(1e-9, 1e-9, 1e-9); // effectively coincident with p[0]

        let mut f = |a: u32, b: u32, c: u32| s.faces.push(Face::new(a, b, c));
        f(p[0], p[1], p[2]); f(p[0], p[2], p[3]);
        f(p[4], p[6], p[5]); f(p[4], p[7], p[6]);
        f(p[0], p[5], p[1]); f(p[0], p[4], p[5]);
        f(p[1], p[6], p[2]); f(p[1], p[5], p[6]);
        f(p[2], p[7], p[3]); f(p[2], p[6], p[7]);
        f(p[3], p[4], p[0]); f(p[3], p[7], p[4]);
        // A degenerate zero-area triangle using the duplicate vertex.
        f(dup, p[1], p[1]);
        s
    }

    #[test]
    fn welds_duplicates_and_drops_degenerate_faces() {
        let s = cube_with_duplicate_vertices();
        let (repaired, report) = repair_mesh(&s, 1e-6);
        assert_eq!(report.vertices_welded, 1);
        assert_eq!(report.degenerate_faces_removed, 1);
        assert_eq!(repaired.faces.len(), 12);
    }

    #[test]
    fn watertight_cube_is_fully_manifold() {
        let s = cube_with_duplicate_vertices();
        let (_repaired, report) = repair_mesh(&s, 1e-6);
        assert_eq!(report.non_manifold_edges, 0);
    }
}
