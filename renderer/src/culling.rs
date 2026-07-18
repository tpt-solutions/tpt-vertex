//! Frustum culling, bounding volumes, and level-of-detail selection.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! Large assemblies are optimized by (1) skipping draw items whose bounds fall
//! entirely outside the camera frustum, and (2) choosing a coarser mesh
//! level-of-detail (LOD) for distant items. These helpers are GPU-free and
//! testable; the renderer calls [`Frustum::from_view_proj`] each frame and tests
//! each draw item's [`Aabb`]/[`BoundingSphere`] against it, then picks an LOD via
//! [`select_lod`]. Instancing is enabled by grouping visible items that share a
//! mesh + LOD (see [`InstanceBatch`]).

use std::collections::HashMap;

use glam::{Mat4, Vec3, Vec4, Vec4Swizzles};

/// An axis-aligned bounding box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    pub fn new(min: Vec3, max: Vec3) -> Self {
        Aabb { min, max }
    }

    /// Build from a point cloud; `None` if empty.
    pub fn from_points(points: impl IntoIterator<Item = Vec3>) -> Option<Aabb> {
        let mut iter = points.into_iter();
        let first = iter.next()?;
        let (mut min, mut max) = (first, first);
        for p in iter {
            min = min.min(p);
            max = max.max(p);
        }
        Some(Aabb { min, max })
    }

    pub fn center(&self) -> Vec3 {
        (self.min + self.max) * 0.5
    }

    pub fn corners(&self) -> [Vec3; 8] {
        let (a, b) = (self.min, self.max);
        [
            Vec3::new(a.x, a.y, a.z),
            Vec3::new(b.x, a.y, a.z),
            Vec3::new(a.x, b.y, a.z),
            Vec3::new(b.x, b.y, a.z),
            Vec3::new(a.x, a.y, b.z),
            Vec3::new(b.x, a.y, b.z),
            Vec3::new(a.x, b.y, b.z),
            Vec3::new(b.x, b.y, b.z),
        ]
    }

    /// Transform by a matrix and return the AABB of the transformed corners.
    pub fn transformed(&self, m: &Mat4) -> Aabb {
        let pts = self.corners().map(|c| m.transform_point3(c));
        Aabb::from_points(pts).unwrap()
    }

    pub fn bounding_sphere(&self) -> BoundingSphere {
        let center = self.center();
        BoundingSphere {
            center,
            radius: (self.max - center).length(),
        }
    }
}

/// A bounding sphere.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoundingSphere {
    pub center: Vec3,
    pub radius: f32,
}

/// A view frustum as six planes (each `Vec4` = `(nx, ny, nz, d)`, normalized,
/// pointing inward: a point is inside iff `dot(plane.xyz, p) + plane.w >= 0`).
#[derive(Debug, Clone, Copy)]
pub struct Frustum {
    pub planes: [Vec4; 6],
}

impl Frustum {
    /// Extract frustum planes from a view-projection matrix (Gribb-Hartmann).
    pub fn from_view_proj(vp: Mat4) -> Frustum {
        // Rows of the matrix.
        let r0 = Vec4::new(vp.x_axis.x, vp.y_axis.x, vp.z_axis.x, vp.w_axis.x);
        let r1 = Vec4::new(vp.x_axis.y, vp.y_axis.y, vp.z_axis.y, vp.w_axis.y);
        let r2 = Vec4::new(vp.x_axis.z, vp.y_axis.z, vp.z_axis.z, vp.w_axis.z);
        let r3 = Vec4::new(vp.x_axis.w, vp.y_axis.w, vp.z_axis.w, vp.w_axis.w);

        let planes = [
            r3 + r0, // left
            r3 - r0, // right
            r3 + r1, // bottom
            r3 - r1, // top
            r3 + r2, // near
            r3 - r2, // far
        ]
        .map(normalize_plane);

        Frustum { planes }
    }

    /// True if the sphere is at least partially inside the frustum.
    pub fn intersects_sphere(&self, s: &BoundingSphere) -> bool {
        self.planes
            .iter()
            .all(|p| p.xyz().dot(s.center) + p.w >= -s.radius)
    }

    /// True if the AABB is at least partially inside the frustum.
    pub fn intersects_aabb(&self, b: &Aabb) -> bool {
        // For each plane, find the AABB corner most in the plane's positive
        // direction; if that is outside, the whole box is outside.
        for p in &self.planes {
            let n = p.xyz();
            let positive = Vec3::new(
                if n.x >= 0.0 { b.max.x } else { b.min.x },
                if n.y >= 0.0 { b.max.y } else { b.min.y },
                if n.z >= 0.0 { b.max.z } else { b.min.z },
            );
            if n.dot(positive) + p.w < 0.0 {
                return false;
            }
        }
        true
    }
}

fn normalize_plane(p: Vec4) -> Vec4 {
    let len = p.xyz().length();
    if len > 1e-12 {
        p / len
    } else {
        p
    }
}

/// LOD levels, coarsest last.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Lod {
    High = 0,
    Medium = 1,
    Low = 2,
}

/// Select an LOD from the camera-to-object distance and the object's radius.
/// The screen-space size proxy is `radius / distance`; larger => finer LOD.
pub fn select_lod(distance: f32, radius: f32) -> Lod {
    if distance <= 1e-4 {
        return Lod::High;
    }
    let screen = radius / distance;
    if screen > 0.15 {
        Lod::High
    } else if screen > 0.04 {
        Lod::Medium
    } else {
        Lod::Low
    }
}

/// A draw item candidate for culling/LOD/instancing.
#[derive(Debug, Clone, Copy)]
pub struct Renderable {
    pub mesh_index: u32,
    /// World-space bounds.
    pub bounds: Aabb,
}

/// A group of visible items sharing a mesh + LOD, eligible for instanced draw.
#[derive(Debug, Clone, PartialEq)]
pub struct InstanceBatch {
    pub mesh_index: u32,
    pub lod: Lod,
    pub count: usize,
}

/// Cull `items` against the frustum, pick an LOD per visible item from the eye
/// position, and group them into instance batches. Returns `(visible_count,
/// batches)`. The batches are deterministic (sorted by mesh then LOD).
pub fn cull_and_batch(
    items: &[Renderable],
    frustum: &Frustum,
    eye: Vec3,
) -> (usize, Vec<InstanceBatch>) {
    let mut counts: HashMap<(u32, Lod), usize> = HashMap::new();
    let mut visible = 0;
    for it in items {
        if !frustum.intersects_aabb(&it.bounds) {
            continue;
        }
        visible += 1;
        let sphere = it.bounds.bounding_sphere();
        let dist = (sphere.center - eye).length();
        let lod = select_lod(dist, sphere.radius);
        *counts.entry((it.mesh_index, lod)).or_insert(0) += 1;
    }
    let mut batches: Vec<InstanceBatch> = counts
        .into_iter()
        .map(|((mesh_index, lod), count)| InstanceBatch {
            mesh_index,
            lod,
            count,
        })
        .collect();
    batches.sort_by(|a, b| a.mesh_index.cmp(&b.mesh_index).then(a.lod.cmp(&b.lod)));
    (visible, batches)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::Camera;

    #[test]
    fn aabb_from_points_and_sphere() {
        let b = Aabb::from_points([Vec3::new(-1.0, -1.0, -1.0), Vec3::new(1.0, 1.0, 1.0)]).unwrap();
        assert_eq!(b.center(), Vec3::ZERO);
        let s = b.bounding_sphere();
        assert!((s.radius - 3.0_f32.sqrt()).abs() < 1e-5);
    }

    #[test]
    fn object_at_origin_is_visible() {
        let cam = Camera::default();
        let f = Frustum::from_view_proj(cam.view_proj());
        let b = Aabb::new(Vec3::splat(-0.5), Vec3::splat(0.5));
        assert!(f.intersects_aabb(&b));
        assert!(f.intersects_sphere(&b.bounding_sphere()));
    }

    #[test]
    fn far_offscreen_object_is_culled() {
        let cam = Camera::default();
        let f = Frustum::from_view_proj(cam.view_proj());
        // Far behind the camera and off to the side.
        let b = Aabb::new(Vec3::new(9000.0, 0.0, 0.0), Vec3::new(9001.0, 1.0, 1.0));
        assert!(!f.intersects_aabb(&b));
    }

    #[test]
    fn lod_coarsens_with_distance() {
        assert_eq!(select_lod(1.0, 1.0), Lod::High);
        assert_eq!(select_lod(20.0, 1.0), Lod::Medium);
        assert_eq!(select_lod(100.0, 1.0), Lod::Low);
    }

    #[test]
    fn cull_and_batch_groups_shared_meshes() {
        let cam = Camera::default();
        let f = Frustum::from_view_proj(cam.view_proj());
        let eye = cam.eye();
        // Two instances of mesh 0 near origin (visible), one far/culled.
        let items = vec![
            Renderable {
                mesh_index: 0,
                bounds: Aabb::new(Vec3::splat(-0.5), Vec3::splat(0.5)),
            },
            Renderable {
                mesh_index: 0,
                bounds: Aabb::new(Vec3::new(0.6, -0.5, -0.5), Vec3::new(1.6, 0.5, 0.5)),
            },
            Renderable {
                mesh_index: 1,
                bounds: Aabb::new(Vec3::new(9000.0, 0.0, 0.0), Vec3::new(9001.0, 1.0, 1.0)),
            },
        ];
        let (visible, batches) = cull_and_batch(&items, &f, eye);
        assert_eq!(visible, 2);
        // Both visible items share mesh 0 and (being close) the same LOD.
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].mesh_index, 0);
        assert_eq!(batches[0].count, 2);
    }
}
