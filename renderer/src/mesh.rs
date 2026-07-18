//! GPU mesh representation derived from kernel geometry.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0

use bytemuck::{Pod, Zeroable};
use tpt_vertex_kernel::geometry::solid::Solid;

/// Per-vertex data uploaded to the GPU (position + normal, interleaved).
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
}

/// A render-ready mesh: interleaved vertices, triangle indices, and a
/// line-index buffer for wireframe rendering.
#[derive(Debug, Clone)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub line_indices: Vec<u32>,
}

impl Mesh {
    pub fn vertex_count(&self) -> u32 {
        self.vertices.len() as u32
    }

    pub fn index_count(&self) -> u32 {
        self.indices.len() as u32
    }

    /// Build a kernel [`Solid`] (positions only) for CPU-side ray picking.
    /// Normals are irrelevant for intersection tests.
    pub fn solid(&self) -> Solid {
        let mut s = Solid::new();
        for v in &self.vertices {
            s.vertices.push(tpt_vertex_kernel::math::Vec3::new(
                v.position[0] as f64,
                v.position[1] as f64,
                v.position[2] as f64,
            ));
        }
        for tri in self.indices.chunks_exact(3) {
            s.faces.push(tpt_vertex_kernel::geometry::solid::Face::new(
                tri[0], tri[1], tri[2],
            ));
        }
        s
    }
}

/// Build a [`Mesh`] from a kernel [`Solid`], computing smooth per-vertex
/// normals by averaging the normals of adjacent triangles.
pub fn mesh_from_solid(solid: &Solid) -> Mesh {
    let mut vertices: Vec<Vertex> = Vec::with_capacity(solid.vertex_count());
    let mut normals: Vec<glam::Vec3> = Vec::with_capacity(solid.vertex_count());
    for v in &solid.vertices {
        vertices.push(Vertex {
            position: [v.x as f32, v.y as f32, v.z as f32],
            normal: [0.0, 0.0, 0.0],
        });
        normals.push(glam::Vec3::ZERO);
    }

    let mut indices = Vec::with_capacity(solid.triangle_count() * 3);
    for f in &solid.faces {
        let a = solid.vertices[f.a as usize];
        let b = solid.vertices[f.b as usize];
        let c = solid.vertices[f.c as usize];
        let n = face_normal(a, b, c);
        normals[f.a as usize] += n;
        normals[f.b as usize] += n;
        normals[f.c as usize] += n;
        indices.push(f.a);
        indices.push(f.b);
        indices.push(f.c);
    }

    for (v, n) in vertices.iter_mut().zip(normals.iter()) {
        let n = n.normalize();
        v.normal = [n.x, n.y, n.z];
    }

    // Decompose each triangle into its three edges for wireframe rendering.
    let mut line_indices = Vec::with_capacity(indices.len() * 2);
    for w in indices.chunks_exact(3) {
        line_indices.push(w[0]);
        line_indices.push(w[1]);
        line_indices.push(w[1]);
        line_indices.push(w[2]);
        line_indices.push(w[2]);
        line_indices.push(w[0]);
    }

    Mesh {
        vertices,
        indices,
        line_indices,
    }
}

fn face_normal(
    a: tpt_vertex_kernel::math::Vec3,
    b: tpt_vertex_kernel::math::Vec3,
    c: tpt_vertex_kernel::math::Vec3,
) -> glam::Vec3 {
    let ab = glam::Vec3::new((b.x - a.x) as f32, (b.y - a.y) as f32, (b.z - a.z) as f32);
    let ac = glam::Vec3::new((c.x - a.x) as f32, (c.y - a.y) as f32, (c.z - a.z) as f32);
    ab.cross(ac)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_vertex_kernel::geometry::features::extrude;
    use tpt_vertex_kernel::geometry::sketch::Sketch;
    use tpt_vertex_kernel::math::Vec2;

    #[test]
    fn mesh_from_box_has_normals() {
        let mut s = Sketch::new();
        s.line(Vec2::ZERO, Vec2::new(2.0, 0.0));
        s.line(Vec2::new(2.0, 0.0), Vec2::new(2.0, 2.0));
        s.line(Vec2::new(2.0, 2.0), Vec2::ZERO);
        let solid = extrude(&s, 3.0);
        let mesh = mesh_from_solid(&solid);
        assert!(mesh.index_count() > 0);
        assert!(mesh.vertices.iter().all(|v| {
            let len = (v.normal[0].powi(2) + v.normal[1].powi(2) + v.normal[2].powi(2)).sqrt();
            (len - 1.0).abs() < 1e-3
        }));
    }

    #[test]
    fn mesh_solid_round_trips_for_picking() {
        let mut s = Sketch::new();
        s.line(Vec2::ZERO, Vec2::new(2.0, 0.0));
        s.line(Vec2::new(2.0, 0.0), Vec2::new(2.0, 2.0));
        s.line(Vec2::new(2.0, 2.0), Vec2::ZERO);
        let solid = extrude(&s, 3.0);
        let mesh = mesh_from_solid(&solid);
        let round = mesh.solid();
        // Triangle count must be preserved for ray picking.
        assert_eq!(round.triangle_count(), mesh.index_count() as usize / 3);
        assert_eq!(round.vertex_count(), mesh.vertices.len());
    }
}
