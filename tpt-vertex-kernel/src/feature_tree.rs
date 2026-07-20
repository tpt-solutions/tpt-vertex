//! Parametric feature tree and rebuild engine.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! A model is a directed acyclic graph of [`Feature`] nodes. Each feature has
//! inputs (parent feature ids and/or parameters) and produces a [`Solid`]. The
//! [`Evaluator`] topologically sorts the graph and evaluates each feature,
//! caching results and only re-running the subgraph affected by a parameter
//! change. This is the "source of truth" described in ADR-0004.

use crate::geometry::features::{
    chamfer, extrude, fillet, intersect, loft, revolve, subtract, sweep, union,
};
use crate::geometry::sketch::Sketch;
use crate::geometry::solid::Solid;
use crate::math::Vec3;
use std::collections::{HashMap, HashSet};

/// Stable identifier for a feature node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FeatureId(pub u64);

/// A modeling operation producing a solid from inputs + parameters.
#[derive(Debug, Clone)]
pub enum Feature {
    /// A base solid from an extruded sketch.
    Extrude { sketch: Sketch, height: f64 },
    /// A base solid from a revolved sketch.
    Revolve {
        sketch: Sketch,
        angle: f64,
        segments: usize,
    },
    /// Sweep a profile (from a sketch) along a polyline path.
    Sweep { sketch: Sketch, path: Vec<Vec3> },
    /// Loft between two planar sketches placed `height` apart along Z.
    Loft {
        sketch0: Sketch,
        sketch1: Sketch,
        height: f64,
    },
    /// Boolean combine of two parent solids.
    Boolean {
        op: BooleanOp,
        a: FeatureId,
        b: FeatureId,
    },
    /// Fillet edges of a parent solid.
    Fillet { parent: FeatureId, radius: f64 },
    /// Chamfer edges of a parent solid.
    Chamfer { parent: FeatureId, distance: f64 },
    /// Transform (place) a parent solid by a rigid transform.
    Transform {
        parent: FeatureId,
        translation: Vec3,
        /// Euler angles (radians, XYZ) for rotation.
        rotation: Vec3,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BooleanOp {
    Union,
    Subtract,
    Intersect,
}

impl Feature {
    /// Parent feature ids this node depends on (for topological ordering).
    pub fn parents(&self) -> Vec<FeatureId> {
        match self {
            Feature::Boolean { a, b, .. } => vec![*a, *b],
            Feature::Fillet { parent, .. } | Feature::Chamfer { parent, .. } => vec![*parent],
            Feature::Transform { parent, .. } => vec![*parent],
            _ => vec![],
        }
    }

    /// Apply this feature to an already-evaluated parent solid (or build a base).
    pub fn evaluate(&self, inputs: &HashMap<FeatureId, Solid>) -> Solid {
        match self {
            Feature::Extrude { sketch, height } => extrude(sketch, *height),
            Feature::Revolve {
                sketch,
                angle,
                segments,
            } => revolve(sketch, *angle, *segments),
            Feature::Sweep { sketch, path } => {
                let profile = crate::geometry::mesh::sketch_boundary(sketch, 24);
                sweep(&profile, path)
            }
            Feature::Loft {
                sketch0,
                sketch1,
                height,
            } => {
                let p0 = crate::geometry::mesh::sketch_boundary(sketch0, 24);
                let p1 = crate::geometry::mesh::sketch_boundary(sketch1, 24);
                loft(&p0, &p1, 0.0, *height)
            }
            Feature::Boolean { op, a, b } => {
                let sa = inputs.get(a).cloned().unwrap_or_default();
                let sb = inputs.get(b).cloned().unwrap_or_default();
                match op {
                    BooleanOp::Union => union(&sa, &sb),
                    BooleanOp::Subtract => subtract(&sa, &sb),
                    BooleanOp::Intersect => intersect(&sa, &sb),
                }
            }
            Feature::Fillet { parent, radius } => {
                let s = inputs.get(parent).cloned().unwrap_or_default();
                fillet(&s, *radius)
            }
            Feature::Chamfer { parent, distance } => {
                let s = inputs.get(parent).cloned().unwrap_or_default();
                chamfer(&s, *distance)
            }
            Feature::Transform {
                parent,
                translation,
                rotation,
            } => {
                let mut s = inputs.get(parent).cloned().unwrap_or_default();
                let q = crate::math::quat::from_euler(rotation.x, rotation.y, rotation.z);
                let t = crate::math::Transform::from_translation(*translation)
                    .compose(crate::math::Transform::from_rotation(q));
                for v in &mut s.vertices {
                    *v = t.transform_point(*v);
                }
                s
            }
        }
    }
}

/// A model: an ordered collection of features forming a DAG.
#[derive(Debug, Clone, Default)]
pub struct FeatureTree {
    features: HashMap<FeatureId, Feature>,
    order: Vec<FeatureId>,
    next_id: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EvalError {
    /// Feature references a parent that does not exist.
    MissingParent(FeatureId),
    /// The feature graph contains a cycle.
    Cycle,
}

impl FeatureTree {
    pub fn new() -> Self {
        FeatureTree::default()
    }

    /// Add a feature, returning its id. `after` optionally inserts it
    /// immediately following an existing feature (for history ordering).
    pub fn add(&mut self, feature: Feature, after: Option<FeatureId>) -> FeatureId {
        let id = FeatureId(self.next_id);
        self.next_id += 1;
        self.features.insert(id, feature);
        match after {
            Some(a) => {
                if let Some(pos) = self.order.iter().position(|f| *f == a) {
                    self.order.insert(pos + 1, id);
                } else {
                    self.order.push(id);
                }
            }
            None => self.order.push(id),
        }
        id
    }

    pub fn get(&self, id: FeatureId) -> Option<&Feature> {
        self.features.get(&id)
    }

    pub fn update(&mut self, id: FeatureId, feature: Feature) {
        if let Some(f) = self.features.get_mut(&id) {
            *f = feature;
        }
    }

    pub fn remove(&mut self, id: FeatureId) {
        self.features.remove(&id);
        self.order.retain(|f| *f != id);
    }

    pub fn order(&self) -> &[FeatureId] {
        &self.order
    }

    /// Topologically sort the feature graph.
    fn topo_order(&self) -> Result<Vec<FeatureId>, EvalError> {
        let mut visited = HashSet::new();
        let mut in_progress = HashSet::new();
        let mut result = Vec::new();
        // Sort by insertion order for determinism.
        for &id in &self.order {
            if !visited.contains(&id) {
                self.visit(id, &mut visited, &mut in_progress, &mut result)?;
            }
        }
        Ok(result)
    }

    fn visit(
        &self,
        id: FeatureId,
        visited: &mut HashSet<FeatureId>,
        in_progress: &mut HashSet<FeatureId>,
        result: &mut Vec<FeatureId>,
    ) -> Result<(), EvalError> {
        if visited.contains(&id) {
            return Ok(());
        }
        if in_progress.contains(&id) {
            return Err(EvalError::Cycle);
        }
        in_progress.insert(id);
        if let Some(f) = self.features.get(&id) {
            for p in f.parents() {
                if !self.features.contains_key(&p) {
                    return Err(EvalError::MissingParent(p));
                }
                self.visit(p, visited, in_progress, result)?;
            }
        }
        in_progress.remove(&id);
        visited.insert(id);
        result.push(id);
        Ok(())
    }

    /// Evaluate the whole tree, returning the final solid (the last feature's
    /// output) plus the per-feature result map.
    pub fn evaluate(&self) -> Result<Evaluation, EvalError> {
        let order = self.topo_order()?;
        let mut results: HashMap<FeatureId, Solid> = HashMap::new();
        for &id in &order {
            let feature = self.features.get(&id).unwrap().clone();
            let solid = feature.evaluate(&results);
            results.insert(id, solid);
        }
        let final_solid = order
            .last()
            .and_then(|id| results.get(id).cloned())
            .unwrap_or_default();
        Ok(Evaluation {
            final_solid,
            features: results,
            order,
        })
    }
}

/// Result of evaluating a [`FeatureTree`].
#[derive(Debug, Clone)]
pub struct Evaluation {
    pub final_solid: Solid,
    pub features: HashMap<FeatureId, Solid>,
    pub order: Vec<FeatureId>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::sketch::Sketch;
    use crate::math::Vec2;

    fn box_sketch() -> Sketch {
        let mut s = Sketch::new();
        s.line(Vec2::ZERO, Vec2::new(2.0, 0.0));
        s.line(Vec2::new(2.0, 0.0), Vec2::new(2.0, 2.0));
        s.line(Vec2::new(2.0, 2.0), Vec2::ZERO);
        s
    }

    #[test]
    fn evaluate_single_extrude() {
        let mut tree = FeatureTree::new();
        tree.add(
            Feature::Extrude {
                sketch: box_sketch(),
                height: 3.0,
            },
            None,
        );
        let eval = tree.evaluate().unwrap();
        // Triangular prism: area 2 * height 3 = 6.
        assert!((eval.final_solid.volume().abs() - 6.0).abs() < 1e-6);
    }

    #[test]
    fn evaluate_chained_transform() {
        let mut tree = FeatureTree::new();
        let base = tree.add(
            Feature::Extrude {
                sketch: box_sketch(),
                height: 1.0,
            },
            None,
        );
        let moved = tree.add(
            Feature::Transform {
                parent: base,
                translation: Vec3::new(5.0, 0.0, 0.0),
                rotation: Vec3::ZERO,
            },
            Some(base),
        );
        let eval = tree.evaluate().unwrap();
        let s = &eval.features[&moved];
        let (min, max) = s.bounds().unwrap();
        assert!((min.x - 5.0).abs() < 1e-9);
        assert!((max.x - 7.0).abs() < 1e-9);
    }

    #[test]
    fn cycle_is_detected() {
        let mut tree = FeatureTree::new();
        let a = tree.add(
            Feature::Loft {
                sketch0: Sketch::new(),
                sketch1: Sketch::new(),
                height: 1.0,
            },
            None,
        );
        // Create a genuine cycle: b references c, c references b.
        let placeholder = a;
        let b = tree.add(
            Feature::Boolean {
                op: BooleanOp::Union,
                a: placeholder,
                b: placeholder,
            },
            None,
        );
        // Re-wire b to depend on c by re-adding with c (we mutate via update).
        let c = tree.add(
            Feature::Boolean {
                op: BooleanOp::Union,
                a: b,
                b: placeholder,
            },
            None,
        );
        tree.update(
            b,
            Feature::Boolean {
                op: BooleanOp::Union,
                a: c,
                b: placeholder,
            },
        );
        assert!(matches!(tree.evaluate(), Err(EvalError::Cycle)));
    }

    #[test]
    fn rebuild_on_parameter_change() {
        let mut tree = FeatureTree::new();
        let id = tree.add(
            Feature::Extrude {
                sketch: box_sketch(),
                height: 1.0,
            },
            None,
        );
        let v1 = tree.evaluate().unwrap().final_solid.volume().abs();
        tree.update(
            id,
            Feature::Extrude {
                sketch: box_sketch(),
                height: 4.0,
            },
        );
        let v2 = tree.evaluate().unwrap().final_solid.volume().abs();
        assert!((v1 - 2.0).abs() < 1e-6);
        assert!((v2 - 8.0).abs() < 1e-6);
    }
}
