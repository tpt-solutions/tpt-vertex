//! Assembly & mating structure for multi-part models.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! An [`Assembly`] is a named collection of [`Part`]s, each carrying a solid
//! (produced by a feature tree) and a placement transform, plus optional
//! [`Mate`] constraints that relate the placement of two parts (coincident
//! faces, aligned axes, distance limits). Mates are solved to position parts
//! relative to one another.

use crate::feature_tree::{Evaluation, FeatureTree};
use crate::geometry::solid::Solid;
use crate::material::Material;
use crate::math::{Quaternion, Transform, Vec3};

/// A unique part identifier within an assembly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PartId(pub u64);

/// A part: a feature tree (its geometry), a placement transform, and an optional
/// engineering material (used by BOM mass and simulation).
#[derive(Debug, Clone)]
pub struct Part {
    pub name: String,
    pub tree: FeatureTree,
    pub transform: Transform,
    /// Optional material; `None` means unspecified (BOM uses a generic default).
    pub material: Option<Material>,
}

impl Part {
    pub fn new(name: impl Into<String>, tree: FeatureTree) -> Self {
        Part {
            name: name.into(),
            tree,
            transform: Transform::identity(),
            material: None,
        }
    }

    /// Builder-style setter for the part material.
    pub fn with_material(mut self, material: Material) -> Self {
        self.material = Some(material);
        self
    }

    /// Evaluate the part's solid in its local frame.
    pub fn evaluate(&self) -> Evaluation {
        self.tree
            .evaluate()
            .expect("part feature tree must evaluate")
    }

    /// Evaluate the part's solid transformed into assembly space.
    pub fn solid_in_assembly(&self) -> Solid {
        let mut s = self.evaluate().final_solid;
        for v in &mut s.vertices {
            *v = self.transform.transform_point(*v);
        }
        s
    }
}

/// A mating constraint between two parts.
///
/// The first three variants are positional; the last three are *DOF-bearing*
/// joints that carry a degree of freedom (angle or offset) which motion studies
/// drive over time (see `tpt-vertex-simulation`).
#[derive(Debug, Clone, PartialEq)]
pub enum Mate {
    /// Two parts share a coincident point/plane (their origins coincide).
    Coincident(PartId, PartId),
    /// One part's origin is placed at a fixed offset from another's origin.
    Offset(PartId, PartId, Vec3),
    /// Two parts are aligned along a common axis (parallel Z axes).
    AxisAligned(PartId, PartId),
    /// Revolute (hinge) joint: `mover` rotates about `axis` (through `anchor`)
    /// relative to `base` by `angle` radians. One rotational DOF.
    Revolute {
        base: PartId,
        mover: PartId,
        anchor: Vec3,
        axis: Vec3,
        angle: f64,
        /// Optional `(min, max)` angle limits in radians.
        limits: Option<(f64, f64)>,
    },
    /// Slider (prismatic) joint: `mover` translates along `axis` relative to
    /// `base` by `offset` (kernel units). One translational DOF.
    Slider {
        base: PartId,
        mover: PartId,
        axis: Vec3,
        offset: f64,
        /// Optional `(min, max)` offset limits.
        limits: Option<(f64, f64)>,
    },
    /// Cylindrical joint: `mover` both rotates about and slides along `axis`
    /// relative to `base`. Two DOF (`angle`, `offset`).
    Cylindrical {
        base: PartId,
        mover: PartId,
        anchor: Vec3,
        axis: Vec3,
        angle: f64,
        offset: f64,
    },
}

/// An assembly of parts and their mates.
#[derive(Debug, Clone, Default)]
pub struct Assembly {
    parts: Vec<(PartId, Part)>,
    mates: Vec<Mate>,
    next_id: u64,
}

impl Assembly {
    pub fn new() -> Self {
        Assembly::default()
    }

    /// Add a part, returning its id.
    pub fn add_part(&mut self, part: Part) -> PartId {
        let id = PartId(self.next_id);
        self.next_id += 1;
        self.parts.push((id, part));
        id
    }

    pub fn part(&self, id: PartId) -> Option<&Part> {
        self.parts
            .iter()
            .find(|(pid, _)| *pid == id)
            .map(|(_, p)| p)
    }

    pub fn part_mut(&mut self, id: PartId) -> Option<&mut Part> {
        self.parts
            .iter_mut()
            .find(|(pid, _)| *pid == id)
            .map(|(_, p)| p)
    }

    pub fn add_mate(&mut self, mate: Mate) {
        self.mates.push(mate);
    }

    pub fn parts(&self) -> &[(PartId, Part)] {
        &self.parts
    }

    pub fn mates(&self) -> &[Mate] {
        &self.mates
    }

    /// Set the drive parameter of the DOF-bearing mate attached to `part` (the
    /// `Revolute`/`Slider`/`Cylindrical` joint whose `mover` is `part`). For a
    /// `Revolute`/`Cylindrical` joint this is the rotation `angle` (radians);
    /// for a `Slider`/`Cylindrical` it is the translation `offset`. Used by
    /// motion studies to scrub a joint through its range. Returns true if a
    /// matching mate was found and updated.
    pub fn set_drive(&mut self, part: PartId, value: f64) -> bool {
        let mut updated = false;
        for m in &mut self.mates {
            let is_target = match m {
                Mate::Revolute { mover, .. } => *mover == part,
                Mate::Slider { mover, .. } => *mover == part,
                Mate::Cylindrical { mover, .. } => *mover == part,
                _ => false,
            };
            if is_target {
                match m {
                    Mate::Revolute { angle, .. } => *angle = value,
                    Mate::Slider { offset, .. } => *offset = value,
                    Mate::Cylindrical { angle, offset, .. } => {
                        *angle = value;
                        *offset = value;
                    }
                    _ => {}
                }
                updated = true;
            }
        }
        updated
    }

    /// Solve mates by adjusting each part's placement transform to satisfy its
    /// constraints (Gauss-Seidel relaxation, like the sketch solver). Returns
    /// the maximum residual after solving.
    pub fn solve_mates(&mut self, max_iters: usize, tol: f64) -> f64 {
        let mut residual = f64::MAX;
        for _ in 0..max_iters {
            residual = 0.0;
            let mates = self.mates.clone();
            for mate in &mates {
                residual = residual.max(self.apply_mate(mate));
            }
            if residual <= tol {
                return residual;
            }
        }
        residual
    }

    fn apply_mate(&mut self, mate: &Mate) -> f64 {
        match mate {
            Mate::Coincident(a, b) => {
                let pa = self.part(*a).map(|p| p.transform.translation);
                let pb = self.part(*b).map(|p| p.transform.translation);
                if let (Some(pa), Some(pb)) = (pa, pb) {
                    let mid = (pa + pb) * 0.5;
                    if let Some(p) = self.part_mut(*a) {
                        p.transform.translation = mid;
                    }
                    if let Some(p) = self.part_mut(*b) {
                        p.transform.translation = mid;
                    }
                    pa.distance(pb)
                } else {
                    0.0
                }
            }
            Mate::Offset(a, b, offset) => {
                let pa = self.part(*a).map(|p| p.transform.translation);
                let pb = self.part(*b).map(|p| p.transform.translation);
                if let (Some(pa), Some(pb)) = (pa, pb) {
                    let target = pb + *offset;
                    let d = target - pa;
                    if let Some(p) = self.part_mut(*a) {
                        p.transform.translation = target;
                    }
                    d.length()
                } else {
                    0.0
                }
            }
            Mate::AxisAligned(a, b) => {
                // Align part `b`'s local Z axis onto part `a`'s local Z axis by
                // applying the smallest rotation between them. This replaces the
                // former no-op stub with real rotational solving.
                let ra = self.part(*a).map(|p| p.transform.rotation);
                let rb = self.part(*b).map(|p| p.transform.rotation);
                if let (Some(ra), Some(rb)) = (ra, rb) {
                    let za = ra.rotate_vec(Vec3::Z);
                    let zb = rb.rotate_vec(Vec3::Z);
                    let correction = crate::math::quat::rotation_between(zb, za);
                    let residual = crate::math::quat::angle_between(za, zb);
                    if let Some(p) = self.part_mut(*b) {
                        p.transform.rotation = (correction * rb).normalize();
                    }
                    residual
                } else {
                    0.0
                }
            }
            Mate::Revolute {
                base,
                mover,
                anchor,
                axis,
                angle,
                limits,
            } => {
                let ang = clamp_limits(*angle, *limits);
                self.apply_joint_rotation(*base, *mover, *anchor, *axis, ang);
                0.0
            }
            Mate::Slider {
                base,
                mover,
                axis,
                offset,
                limits,
            } => {
                let off = clamp_limits(*offset, *limits);
                self.apply_joint_translation(*base, *mover, *axis, off);
                0.0
            }
            Mate::Cylindrical {
                base,
                mover,
                anchor,
                axis,
                angle,
                offset,
            } => {
                self.apply_joint_rotation(*base, *mover, *anchor, *axis, *angle);
                self.apply_joint_translation(*base, *mover, *axis, *offset);
                0.0
            }
        }
    }

    /// Position `mover` by rotating it `angle` radians about `axis` through
    /// `anchor`, expressed in `base`'s frame. Sets the mover's transform to the
    /// composed joint pose (used by both mate solving and motion playback).
    fn apply_joint_rotation(
        &mut self,
        base: PartId,
        mover: PartId,
        anchor: Vec3,
        axis: Vec3,
        angle: f64,
    ) {
        let base_tf = self
            .part(base)
            .map(|p| p.transform)
            .unwrap_or_else(Transform::identity);
        let world_axis = base_tf.transform_dir(axis).normalize();
        let world_anchor = base_tf.transform_point(anchor);
        let q = Quaternion::from_axis_angle(world_axis, angle);
        // Rotation about an arbitrary point: p' = q*(p - anchor) + anchor.
        if let Some(p) = self.part_mut(mover) {
            let new_rot = (q * p.transform.rotation).normalize();
            let rel = p.transform.translation - world_anchor;
            let new_tr = q.rotate_vec(rel) + world_anchor;
            p.transform.rotation = new_rot;
            p.transform.translation = new_tr;
        }
    }

    /// Position `mover` by translating it `offset` along `axis`, expressed in
    /// `base`'s frame.
    fn apply_joint_translation(&mut self, base: PartId, mover: PartId, axis: Vec3, offset: f64) {
        let base_tf = self
            .part(base)
            .map(|p| p.transform)
            .unwrap_or_else(Transform::identity);
        let world_axis = base_tf.transform_dir(axis).normalize();
        if let Some(p) = self.part_mut(mover) {
            p.transform.translation = p.transform.translation + world_axis * offset;
        }
    }

    /// Total triangle count across all parts in assembly space.
    pub fn total_triangles(&self) -> usize {
        self.parts
            .iter()
            .map(|(_, p)| p.solid_in_assembly().triangle_count())
            .sum()
    }
}

/// Clamp a joint DOF value (angle or offset) to optional `(min, max)` limits.
/// With no limits the value passes through unchanged.
fn clamp_limits(value: f64, limits: Option<(f64, f64)>) -> f64 {
    match limits {
        Some((lo, hi)) if hi >= lo => value.clamp(lo, hi),
        _ => value,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feature_tree::Feature;
    use crate::geometry::sketch::Sketch;
    use crate::math::Vec2;

    fn block() -> FeatureTree {
        let mut s = Sketch::new();
        s.line(Vec2::ZERO, Vec2::new(1.0, 0.0));
        s.line(Vec2::new(1.0, 0.0), Vec2::new(1.0, 1.0));
        s.line(Vec2::new(1.0, 1.0), Vec2::ZERO);
        let mut t = FeatureTree::new();
        t.add(
            Feature::Extrude {
                sketch: s,
                height: 1.0,
            },
            None,
        );
        t
    }

    #[test]
    fn coincident_mate_centers_parts() {
        let mut asm = Assembly::new();
        let a = asm.add_part(Part::new("A", block()));
        let b = asm.add_part(Part::new("B", block()));
        asm.part_mut(a).unwrap().transform.translation = Vec3::new(2.0, 0.0, 0.0);
        asm.add_mate(Mate::Coincident(a, b));
        let res = asm.solve_mates(50, 1e-9);
        assert!(res < 1e-6);
        let pa = asm.part(a).unwrap().transform.translation;
        let pb = asm.part(b).unwrap().transform.translation;
        assert!(pa.distance(pb) < 1e-6);
    }

    #[test]
    fn offset_mate_positions_b() {
        let mut asm = Assembly::new();
        let a = asm.add_part(Part::new("A", block()));
        let b = asm.add_part(Part::new("B", block()));
        asm.add_mate(Mate::Offset(a, b, Vec3::new(3.0, 0.0, 0.0)));
        asm.solve_mates(50, 1e-9);
        let pa = asm.part(a).unwrap().transform.translation;
        let pb = asm.part(b).unwrap().transform.translation;
        // Offset(a, b, off) means a is placed at b + off.
        assert!((pa.x - pb.x - 3.0).abs() < 1e-6);
    }
}
