//! Object picking via ray casting against scene geometry.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0

use glam::{Mat4, Vec3, Vec4};
use vertex_kernel::geometry::solid::{Face, Solid};

/// A pick ray in world space.
#[derive(Debug, Clone, Copy)]
pub struct Ray {
    pub origin: Vec3,
    pub direction: Vec3,
}

/// Result of a pick test.
#[derive(Debug, Clone, Copy)]
pub struct PickHit {
    pub node_id: u64,
    pub distance: f32,
    pub point: Vec3,
}

/// Build a world-space ray from camera screen coordinates (NDC -1..1).
pub fn screen_ray(
    camera_view_proj_inverse: Mat4,
    ndc_x: f32,
    ndc_y: f32,
    eye: Vec3,
) -> Ray {
    let near = unproject(camera_view_proj_inverse, ndc_x, ndc_y, 0.0);
    let far = unproject(camera_view_proj_inverse, ndc_x, ndc_y, 1.0);
    let direction = (far - near).normalize();
    Ray {
        origin: eye,
        direction,
    }
}

fn unproject(inv_vp: Mat4, x: f32, y: f32, z: f32) -> Vec3 {
    let p = inv_vp * Vec4::new(x, y, z, 1.0);
    Vec3::new(p.x, p.y, p.z)
}

/// Cast a ray against a [`Solid`] whose vertices are transformed by `world`.
/// Returns the nearest hit distance along the ray, or None.
pub fn ray_vs_solid(ray: Ray, solid: &Solid, world: Mat4) -> Option<(f32, Vec3)> {
    let mut best: Option<(f32, Vec3)> = None;
    for f in &solid.faces {
        if let Some((t, point)) = ray_vs_face(ray, solid, f, world) {
            if t >= 0.0 && best.map(|(bt, _)| t < bt).unwrap_or(true) {
                best = Some((t, point));
            }
        }
    }
    best
}

fn ray_vs_face(ray: Ray, solid: &Solid, f: &Face, world: Mat4) -> Option<(f32, Vec3)> {
    let a = transform_point(world, solid.vertices[f.a as usize]);
    let b = transform_point(world, solid.vertices[f.b as usize]);
    let c = transform_point(world, solid.vertices[f.c as usize]);
    ray_triangle(ray, a, b, c)
}

fn transform_point(world: Mat4, v: vertex_kernel::math::Vec3) -> Vec3 {
    let p = world * Vec4::new(v.x as f32, v.y as f32, v.z as f32, 1.0);
    Vec3::new(p.x, p.y, p.z)
}

/// Möller–Trumbore ray/triangle intersection. Returns `(t, hit_point)` where
/// `t` is the distance along the ray (0 = origin), or None on miss.
pub fn ray_triangle(ray: Ray, a: Vec3, b: Vec3, c: Vec3) -> Option<(f32, Vec3)> {
    let epsilon = 1e-7;
    let edge1 = b - a;
    let edge2 = c - a;
    let h = ray.direction.cross(edge2);
    let det = edge1.dot(h);
    if det > -epsilon && det < epsilon {
        return None; // Ray parallel to triangle.
    }
    let inv_det = 1.0 / det;
    let s = ray.origin - a;
    let u = inv_det * s.dot(h);
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(edge1);
    let v = inv_det * ray.direction.dot(q);
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = inv_det * edge2.dot(q);
    if t > epsilon {
        Some((t, ray.origin + ray.direction * t))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ray_hits_centered_triangle() {
        let ray = Ray {
            origin: Vec3::new(0.0, 0.0, 5.0),
            direction: Vec3::new(0.0, 0.0, -1.0),
        };
        // Triangle in the z=0 plane spanning x,y in [-1,1].
        let a = Vec3::new(-1.0, -1.0, 0.0);
        let b = Vec3::new(1.0, -1.0, 0.0);
        let c = Vec3::new(0.0, 1.0, 0.0);
        let (t, p) = ray_triangle(ray, a, b, c).unwrap();
        assert!((t - 5.0).abs() < 1e-5);
        assert!((p.z).abs() < 1e-5);
    }

    #[test]
    fn ray_misses_triangle() {
        let ray = Ray {
            origin: Vec3::new(5.0, 5.0, 5.0),
            direction: Vec3::new(0.0, 0.0, -1.0),
        };
        let a = Vec3::new(-1.0, -1.0, 0.0);
        let b = Vec3::new(1.0, -1.0, 0.0);
        let c = Vec3::new(0.0, 1.0, 0.0);
        assert!(ray_triangle(ray, a, b, c).is_none());
    }
}
