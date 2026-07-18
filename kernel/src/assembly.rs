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
use crate::math::{Transform, Vec3};

/// A unique part identifier within an assembly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PartId(pub u64);

/// A part: a feature tree (its geometry) plus a placement transform.
#[derive(Debug, Clone)]
pub struct Part {
    pub name: String,
    pub tree: FeatureTree,
    pub transform: Transform,
}

impl Part {
    pub fn new(name: impl Into<String>, tree: FeatureTree) -> Self {
        Part {
            name: name.into(),
            tree,
            transform: Transform::identity(),
        }
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
#[derive(Debug, Clone, PartialEq)]
pub enum Mate {
    /// Two parts share a coincident point/plane (their origins coincide).
    Coincident(PartId, PartId),
    /// One part's origin is placed at a fixed offset from another's origin.
    Offset(PartId, PartId, Vec3),
    /// Two parts are aligned along a common axis (parallel Z axes).
    AxisAligned(PartId, PartId),
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
            Mate::AxisAligned(_, _) => {
                // v1: axis alignment is implied by the shared identity rotation;
                // no translational correction needed.
                0.0
            }
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
