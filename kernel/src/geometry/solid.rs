//! Faceted B-rep solid representation.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! For v1 the kernel uses a **faceted** boundary representation: a solid is a
//! set of triangle faces over a shared vertex pool, plus a face/edge topology
//! summary used for boolean operations. This is exact enough for rendering,
//! STL export, and a first-pass boolean engine, and is the representation
//! described in ADR-0004 (hybrid B-rep with CSG feature ops).

use crate::math::Vec3;

/// A triangular face referencing three vertex indices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Face {
    pub a: u32,
    pub b: u32,
    pub c: u32,
}

impl Face {
    pub fn new(a: u32, b: u32, c: u32) -> Self {
        Face { a, b, c }
    }

    /// Vertex indices as a slice.
    pub fn indices(&self) -> [u32; 3] {
        [self.a, self.b, self.c]
    }
}

/// A solid: a watertight (ideally) shell of triangles over a vertex pool.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Solid {
    pub vertices: Vec<Vec3>,
    pub faces: Vec<Face>,
}

impl Solid {
    pub fn new() -> Self {
        Solid::default()
    }

    /// Number of triangles.
    pub fn triangle_count(&self) -> usize {
        self.faces.len()
    }

    /// Number of vertices.
    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }

    /// Add a vertex, returning its index.
    pub fn add_vertex(&mut self, v: Vec3) -> u32 {
        self.vertices.push(v);
        (self.vertices.len() - 1) as u32
    }

    /// Add a triangle by vertex positions, deduplicating coincident vertices.
    pub fn add_triangle(&mut self, a: Vec3, b: Vec3, c: Vec3) {
        let ia = self.push_dedup(a);
        let ib = self.push_dedup(b);
        let ic = self.push_dedup(c);
        self.faces.push(Face::new(ia, ib, ic));
    }

    fn push_dedup(&mut self, v: Vec3) -> u32 {
        for (i, existing) in self.vertices.iter().enumerate() {
            if existing.distance(v) < 1e-7 {
                return i as u32;
            }
        }
        self.add_vertex(v)
    }

    /// Total surface area.
    pub fn surface_area(&self) -> f64 {
        self.faces
            .iter()
            .map(|f| {
                let a = self.vertices[f.a as usize];
                let b = self.vertices[f.b as usize];
                let c = self.vertices[f.c as usize];
                (b - a).cross(c - a).length() * 0.5
            })
            .sum()
    }

    /// Axis-aligned bounding box `(min, max)`; `None` if empty.
    pub fn bounds(&self) -> Option<(Vec3, Vec3)> {
        let mut iter = self.vertices.iter();
        let first = *iter.next()?;
        let (mut min, mut max) = (first, first);
        for v in iter {
            min.x = min.x.min(v.x);
            min.y = min.y.min(v.y);
            min.z = min.z.min(v.z);
            max.x = max.x.max(v.x);
            max.y = max.y.max(v.y);
            max.z = max.z.max(v.z);
        }
        Some((min, max))
    }

    /// Approximate volume via the divergence theorem (signed; assumes a
    /// consistently oriented closed mesh).
    pub fn volume(&self) -> f64 {
        self.faces
            .iter()
            .map(|f| {
                let a = self.vertices[f.a as usize];
                let b = self.vertices[f.b as usize];
                let c = self.vertices[f.c as usize];
                a.dot(b.cross(c)) / 6.0
            })
            .sum()
    }

    /// Append another solid's geometry into this one (used by union pre-merge).
    pub fn extend(&mut self, other: &Solid) {
        let offset = self.vertices.len() as u32;
        self.vertices.extend_from_slice(&other.vertices);
        for f in &other.faces {
            self.faces
                .push(Face::new(f.a + offset, f.b + offset, f.c + offset));
        }
    }

    /// Flip the winding of every face, reversing all normals in place. Used to
    /// correct consistently inward-oriented generated meshes (e.g. revolve).
    pub fn reverse_winding(&mut self) {
        for f in &mut self.faces {
            std::mem::swap(&mut f.b, &mut f.c);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triangle_area_and_volume() {
        let mut s = Solid::new();
        // Unit tetrahedron with all faces oriented outward. Volume = 1/6.
        s.add_triangle(Vec3::ZERO, Vec3::Y, Vec3::X);
        s.add_triangle(Vec3::ZERO, Vec3::X, Vec3::Z);
        s.add_triangle(Vec3::ZERO, Vec3::Z, Vec3::Y);
        s.add_triangle(Vec3::X, Vec3::Y, Vec3::Z);
        // Unit tetrahedron: 3 right-triangle faces (area 1/2 each) + 1 equilateral
        // face (area sqrt(3)/4 ... wait side sqrt(2) -> area sqrt(3)/2).
        let expected_area = 1.5 + 3.0f64.sqrt() / 2.0;
        assert!((s.surface_area() - expected_area).abs() < 1e-9);
        assert!((s.volume() - 1.0 / 6.0).abs() < 1e-9);
    }

    #[test]
    fn dedup_vertices() {
        let mut s = Solid::new();
        s.add_triangle(Vec3::ZERO, Vec3::X, Vec3::Y);
        s.add_triangle(Vec3::ZERO, Vec3::X, Vec3::Y);
        assert_eq!(s.vertex_count(), 3);
        assert_eq!(s.triangle_count(), 2);
    }
}
