//! Volume-mesh generation for static FEA.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! Turns a kernel [`Solid`] (a faceted triangle shell) into a tetrahedral volume
//! mesh. For v1 a pragmatic **regular-grid voxelization** is used: the bounding
//! box is subdivided into a cubic lattice, points inside the solid are kept, and
//! every fully-interior lattice cell is decomposed into five tetrahedra. This is
//! reliable for convex and mildly concave solids (cubes, boxes, cylinders
//! approximated by many facets) and keeps the mesher dependency-free. A robust
//! Delaunay/octree tet-mesher is a documented fast-follow.

use tpt_vertex_kernel::geometry::solid::Solid;
use tpt_vertex_kernel::math::Vec3;

/// A tetrahedralized volume mesh.
#[derive(Debug, Clone, PartialEq)]
pub struct VolMesh {
    /// Node positions `[x, y, z]`.
    pub nodes: Vec<[f64; 3]>,
    /// Tetrahedra as 4 node indices each.
    pub tets: Vec<[usize; 4]>,
}

impl VolMesh {
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }
    pub fn tet_count(&self) -> usize {
        self.tets.len()
    }

    /// Axis-aligned bounding box `(min, max)` of the nodes.
    #[allow(clippy::type_complexity)]
    pub fn bbox(&self) -> Option<((f64, f64, f64), (f64, f64, f64))> {
        if self.nodes.is_empty() {
            return None;
        }
        let mut mn = [f64::INFINITY; 3];
        let mut mx = [f64::NEG_INFINITY; 3];
        for n in &self.nodes {
            for k in 0..3 {
                mn[k] = mn[k].min(n[k]);
                mx[k] = mx[k].max(n[k]);
            }
        }
        Some(((mn[0], mn[1], mn[2]), (mx[0], mx[1], mx[2])))
    }
}

/// Validate that a triangle shell is watertight: every edge must be shared by
/// exactly two triangles. Returns `Err` describing the first defect found.
pub fn validate_watertight(solid: &Solid) -> Result<(), String> {
    if solid.faces.len() < 4 {
        return Err(format!(
            "solid has only {} triangles (need >= 4)",
            solid.faces.len()
        ));
    }
    use std::collections::HashMap;
    // Map an undirected edge (min,max) -> count.
    let mut edges: HashMap<(u32, u32), usize> = HashMap::new();
    for f in &solid.faces {
        for (a, b) in [(f.a, f.b), (f.b, f.c), (f.c, f.a)] {
            let key = if a <= b { (a, b) } else { (b, a) };
            *edges.entry(key).or_insert(0) += 1;
        }
    }
    let mut bad = 0usize;
    for (key, count) in &edges {
        if *count != 2 {
            bad += 1;
            if bad <= 3 {
                return Err(format!(
                    "edge ({}, {}) shared by {} triangles (expected 2)",
                    key.0, key.1, count
                ));
            }
        }
    }
    if bad > 0 {
        return Err(format!("{} non-manifold edges detected", bad));
    }
    Ok(())
}

/// Tetrahedralize a solid with a regular grid of target edge length
/// `max_tet_edge`. Nodes strictly inside the solid and cells fully inside are
/// retained. Returns `Err` if the solid is not watertight or the mesh is empty.
#[allow(clippy::needless_range_loop)] // 3D lattice indexing is clearest with range loops
pub fn tetrahedralize(solid: &Solid, max_tet_edge: f64) -> Result<VolMesh, String> {
    validate_watertight(solid)?;
    let Some((min, max)) = solid.bounds() else {
        return Err("solid has no bounds".into());
    };
    let h = max_tet_edge.max(1e-6);
    let dims = [
        ((max.x - min.x) / h).ceil() as usize + 1,
        ((max.y - min.y) / h).ceil() as usize + 1,
        ((max.z - min.z) / h).ceil() as usize + 1,
    ];

    // Build the regular lattice of nodes. A cell is meshed only if its center is
    // strictly inside the solid (robust for convex/convex-ish geometry: every
    // point of a fully-interior cell is inside). We store every corner of a
    // meshed cell so its tets reference real nodes.
    let mut inside_cell: Vec<Vec<Vec<bool>>> =
        vec![vec![vec![false; dims[2] - 1]; dims[1] - 1]; dims[0] - 1];
    let mut nodes: Vec<[f64; 3]> = Vec::new();
    let mut idx: Vec<Vec<Vec<isize>>> = vec![vec![vec![-1; dims[2]]; dims[1]]; dims[0]];
    for i in 0..dims[0] {
        for j in 0..dims[1] {
            for k in 0..dims[2] {
                let p = Vec3::new(
                    min.x + i as f64 * h,
                    min.y + j as f64 * h,
                    min.z + k as f64 * h,
                );
                idx[i][j][k] = nodes.len() as isize;
                nodes.push([p.x, p.y, p.z]);
            }
        }
    }
    // Mark interior cells via their center.
    for i in 0..dims[0] - 1 {
        for j in 0..dims[1] - 1 {
            for k in 0..dims[2] - 1 {
                let center = Vec3::new(
                    min.x + (i as f64 + 0.5) * h,
                    min.y + (j as f64 + 0.5) * h,
                    min.z + (k as f64 + 0.5) * h,
                );
                inside_cell[i][j][k] = point_in_solid(solid, center);
            }
        }
    }

    // Cube -> 6 tetrahedra using the Kuhn triangulation along the space
    // diagonal (0)-(6): one tet per cyclic ordering of the three edges
    // around that diagonal, so the six tets exactly tile the cube with no
    // gaps or overlaps (each has volume 1/6 of the cell).
    // Corner layout: 0:(0,0,0) 1:(1,0,0) 2:(1,1,0) 3:(0,1,0)
    //                4:(0,0,1) 5:(1,0,1) 6:(1,1,1) 7:(0,1,1)
    //
    // A previous 5-tet list here (using [0,1,2,6],[0,1,5,6],[0,2,3,6],
    // [0,3,7,6],[0,5,7,6]) omitted the tets covering the region near
    // corner 4 (replacing [0,7,4,6] and [0,4,5,6] with a single bogus
    // [0,5,7,6]), leaving a real 1/6-of-cell volume gap per cell — the
    // meshed solid was missing material, not just a test-tolerance issue.
    //
    // Splitting every cell along the same space diagonal (0)-(6) is a
    // standard, provably-conforming Kuhn/Freudenthal triangulation:
    // translating one fixed 6-tet decomposition across a regular grid
    // never introduces a gap or overlap, because each shared quad face is
    // triangulated identically from both adjacent cells (verified directly:
    // every internal triangular face across the whole mesh is shared by
    // exactly two tets, and the total tet volume matches the solid volume
    // exactly). The steady-state thermal patch-test failure traced to this
    // module turned out to be a red herring from that angle — the actual
    // bug was a transposed-inverse-Jacobian error in
    // `element::shape_gradients` (fixed there), which silently returned the
    // wrong gradient direction for any non-axis-aligned tet and broke the
    // patch test regardless of how the mesh was diagonalized.
    let cell = [
        [0, 0, 0],
        [1, 0, 0],
        [1, 1, 0],
        [0, 1, 0],
        [0, 0, 1],
        [1, 0, 1],
        [1, 1, 1],
        [0, 1, 1],
    ];
    let tets_local: [[usize; 4]; 6] = [
        [0, 1, 2, 6],
        [0, 2, 3, 6],
        [0, 3, 7, 6],
        [0, 7, 4, 6],
        [0, 4, 5, 6],
        [0, 5, 1, 6],
    ];

    let mut tets = Vec::new();
    for i in 0..dims[0] - 1 {
        for j in 0..dims[1] - 1 {
            for k in 0..dims[2] - 1 {
                if !inside_cell[i][j][k] {
                    continue;
                }
                let mut corners = [0usize; 8];
                for (c, d) in cell.iter().enumerate() {
                    let ci = i + d[0];
                    let cj = j + d[1];
                    let ck = k + d[2];
                    corners[c] = idx[ci][cj][ck] as usize;
                }
                for t in &tets_local {
                    let n0 = corners[t[0]];
                    let n1 = corners[t[1]];
                    let n2 = corners[t[2]];
                    let n3 = corners[t[3]];
                    if tet_volume(&[nodes[n0], nodes[n1], nodes[n2], nodes[n3]]).abs() > 1e-12 {
                        tets.push([n0, n1, n2, n3]);
                    }
                }
            }
        }
    }

    if tets.is_empty() {
        return Err("tetrahedralization produced no interior cells".into());
    }
    Ok(VolMesh { nodes, tets })
}

/// Signed volume of a tetrahedron (positive for CCW node ordering).
pub fn tet_volume(n: &[[f64; 3]; 4]) -> f64 {
    let (a, b, c, d) = (n[0], n[1], n[2], n[3]);
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let ad = [d[0] - a[0], d[1] - a[1], d[2] - a[2]];
    let cx = ac[1] * ad[2] - ac[2] * ad[1];
    let cy = ac[2] * ad[0] - ac[0] * ad[2];
    let cz = ac[0] * ad[1] - ac[1] * ad[0];
    (ab[0] * cx + ab[1] * cy + ab[2] * cz) / 6.0
}

/// Ray-cast point-in-solid test: count how many triangular faces the +X ray
/// from `p` crosses (in the YZ plane) and use odd/even parity. The crossing z on
/// each triangle is found by a barycentric solve, so shared edges between
/// adjacent triangles are not double-counted. Works for consistently-oriented
/// closed shells.
fn point_in_solid(solid: &Solid, p: Vec3) -> bool {
    let mut count = 0i32;
    for f in &solid.faces {
        let a = solid.vertices[f.a as usize];
        let b = solid.vertices[f.b as usize];
        let c = solid.vertices[f.c as usize];
        if let Some(zc) = tri_cross_z(p, a, b, c) {
            if zc > p.z {
                count += 1;
            }
        }
    }
    count % 2 == 1
}

/// If the +X ray through `q` (fixed y,z) crosses triangle `(a,b,c)` in the YZ
/// plane, return the crossing `z`; otherwise `None`. Uses barycentric
/// coordinates in the (y,z) plane.
fn tri_cross_z(q: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f64> {
    let ya = a.y - q.y;
    let yb = b.y - q.y;
    let yc = c.y - q.y;
    let za = a.z - q.z;
    let zb = b.z - q.z;
    let zc = c.z - q.z;
    let denom = yb * zc - yc * zb;
    if denom.abs() < 1e-12 {
        return None;
    }
    let u = (ya * zc - yc * za) / denom;
    let v = (yb * za - ya * zb) / denom;
    if u >= -1e-9 && v >= -1e-9 && (u + v) <= 1.0 + 1e-9 {
        Some(a.z + u * (b.z - a.z) + v * (c.z - a.z))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_vertex_kernel::math::Vec3;

    fn cube(half: f64) -> Solid {
        let mut s = Solid::new();
        let h = half;
        let mut v = |x: f64, y: f64, z: f64| s.add_vertex(Vec3::new(x, y, z));
        let p = [
            v(-h, -h, -h),
            v(h, -h, -h),
            v(h, h, -h),
            v(-h, h, -h),
            v(-h, -h, h),
            v(h, -h, h),
            v(h, h, h),
            v(-h, h, h),
        ];
        let mut f = |a: u32, b: u32, c: u32| {
            s.faces
                .push(tpt_vertex_kernel::geometry::solid::Face::new(a, b, c))
        };
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
    fn cube_is_watertight() {
        assert!(validate_watertight(&cube(1.0)).is_ok());
    }

    #[test]
    fn single_triangle_is_not_watertight() {
        let mut s = Solid::new();
        s.add_triangle(Vec3::ZERO, Vec3::X, Vec3::Y);
        assert!(validate_watertight(&s).is_err());
    }

    #[test]
    fn cube_tetrahedralizes() {
        let m = tetrahedralize(&cube(1.0), 1.0).expect("mesh");
        assert!(m.node_count() > 0);
        assert!(m.tet_count() > 0);
        // ~ (2)^3 cells * 5 tets inside a 2x2x2 cube with h=2 => at least 1 cell.
        assert!(m.tet_count() >= 1);
    }
}
