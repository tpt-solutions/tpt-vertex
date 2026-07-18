//! CRDT document model for the parametric feature tree.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! Implements the design in ADR-0006:
//!
//! - Feature existence is an **observed-remove set (OR-Set)**: an add tags the
//!   feature with a unique clock; a remove tombstones the tags it observed.
//!   Concurrent add/remove converge (a concurrent add survives a remove that did
//!   not observe it).
//! - Each feature's parameters are **LWW-Registers** keyed by parameter name and
//!   stamped with a [`HybridClock`]; the highest clock wins.
//! - History **ordering** is a fractional-index sequence CRDT: each feature holds
//!   a rational-ish position key (`Vec<u32>` compared lexicographically) so
//!   inserts between neighbours never collide.
//!
//! All merge operators are commutative, associative, and idempotent, so applying
//! the same set of [`Op`]s in any order on any replica yields the same document.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::clock::{HybridClock, ReplicaId};

/// Stable identifier of a feature within the collaborative document.
pub type FeatureKey = u64;

/// A parameter value. Kept as a small tagged union so the CRDT is decoupled from
/// the kernel's concrete `Feature` enum while remaining convertible to it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ParamValue {
    Number(f64),
    Int(i64),
    Text(String),
    Bool(bool),
}

/// An LWW register: a value plus the clock that last wrote it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Register {
    pub value: ParamValue,
    pub clock: HybridClock,
}

impl Register {
    /// Merge in a competing write; keep the one with the higher clock.
    fn merge(&mut self, other: &Register) {
        if other.clock > self.clock {
            self.value = other.value.clone();
            self.clock = other.clock;
        }
    }
}

/// The state of one feature: its kind, LWW parameters, and OR-Set add tags.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FeatureState {
    /// Discriminant name (e.g. "Extrude"), an LWW register once set.
    pub kind: Option<Register>,
    /// Ordering position key (fractional index), an LWW register.
    pub position: Option<Register>,
    /// LWW parameter registers by name.
    pub params: BTreeMap<String, Register>,
    /// OR-Set: active add tags. Non-empty ⇒ the feature exists.
    pub add_tags: HashSet<HybridClock>,
    /// OR-Set: tombstoned add tags (observed removes).
    pub removed_tags: HashSet<HybridClock>,
}

impl FeatureState {
    /// A feature exists iff it has an add tag not shadowed by a remove.
    pub fn exists(&self) -> bool {
        self.add_tags
            .difference(&self.removed_tags)
            .next()
            .is_some()
    }
}

/// A single CRDT operation. Ops are self-describing and idempotent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Op {
    /// Add (or re-add) a feature with a unique tag and a kind.
    AddFeature {
        key: FeatureKey,
        kind: String,
        tag: HybridClock,
        position: String,
    },
    /// Remove a feature by tombstoning the add tags it observed.
    RemoveFeature {
        key: FeatureKey,
        observed_tags: Vec<HybridClock>,
    },
    /// Set a parameter (LWW).
    SetParam {
        key: FeatureKey,
        name: String,
        value: ParamValue,
        clock: HybridClock,
    },
    /// Move a feature to a new ordering position (LWW).
    SetPosition {
        key: FeatureKey,
        position: String,
        clock: HybridClock,
    },
}

impl Op {
    /// The clock carried by this op (for causal bookkeeping).
    pub fn clock(&self) -> Option<HybridClock> {
        match self {
            Op::AddFeature { tag, .. } => Some(*tag),
            Op::SetParam { clock, .. } => Some(*clock),
            Op::SetPosition { clock, .. } => Some(*clock),
            Op::RemoveFeature { .. } => None,
        }
    }
}

/// The replicated collaborative document.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CrdtDoc {
    features: HashMap<FeatureKey, FeatureState>,
}

impl CrdtDoc {
    pub fn new() -> Self {
        CrdtDoc::default()
    }

    /// Apply an op (idempotent, commutative). Returns `true` if it changed state.
    pub fn apply(&mut self, op: &Op) -> bool {
        match op {
            Op::AddFeature {
                key,
                kind,
                tag,
                position,
            } => {
                let f = self.features.entry(*key).or_default();
                let inserted = f.add_tags.insert(*tag);
                let kr = Register {
                    value: ParamValue::Text(kind.clone()),
                    clock: *tag,
                };
                match &mut f.kind {
                    Some(existing) => existing.merge(&kr),
                    None => f.kind = Some(kr),
                }
                let pr = Register {
                    value: ParamValue::Text(position.clone()),
                    clock: *tag,
                };
                match &mut f.position {
                    Some(existing) => existing.merge(&pr),
                    None => f.position = Some(pr),
                }
                inserted
            }
            Op::RemoveFeature { key, observed_tags } => {
                let f = self.features.entry(*key).or_default();
                let mut changed = false;
                for t in observed_tags {
                    changed |= f.removed_tags.insert(*t);
                }
                changed
            }
            Op::SetParam {
                key,
                name,
                value,
                clock,
            } => {
                let f = self.features.entry(*key).or_default();
                let reg = Register {
                    value: value.clone(),
                    clock: *clock,
                };
                match f.params.get_mut(name) {
                    Some(existing) => {
                        let before = existing.clone();
                        existing.merge(&reg);
                        *existing != before
                    }
                    None => {
                        f.params.insert(name.clone(), reg);
                        true
                    }
                }
            }
            Op::SetPosition {
                key,
                position,
                clock,
            } => {
                let f = self.features.entry(*key).or_default();
                let reg = Register {
                    value: ParamValue::Text(position.clone()),
                    clock: *clock,
                };
                match &mut f.position {
                    Some(existing) => {
                        let before = existing.clone();
                        existing.merge(&reg);
                        *existing != before
                    }
                    None => {
                        f.position = Some(reg);
                        true
                    }
                }
            }
        }
    }

    /// Merge another document into this one (state-based CRDT merge).
    pub fn merge(&mut self, other: &CrdtDoc) {
        for (key, of) in &other.features {
            let f = self.features.entry(*key).or_default();
            for t in &of.add_tags {
                f.add_tags.insert(*t);
            }
            for t in &of.removed_tags {
                f.removed_tags.insert(*t);
            }
            if let Some(ok) = &of.kind {
                match &mut f.kind {
                    Some(existing) => existing.merge(ok),
                    None => f.kind = Some(ok.clone()),
                }
            }
            if let Some(op) = &of.position {
                match &mut f.position {
                    Some(existing) => existing.merge(op),
                    None => f.position = Some(op.clone()),
                }
            }
            for (name, oreg) in &of.params {
                match f.params.get_mut(name) {
                    Some(existing) => existing.merge(oreg),
                    None => {
                        f.params.insert(name.clone(), oreg.clone());
                    }
                }
            }
        }
    }

    /// Live feature keys in ordering (position) order.
    pub fn ordered_keys(&self) -> Vec<FeatureKey> {
        let mut live: Vec<(String, FeatureKey)> = self
            .features
            .iter()
            .filter(|(_, f)| f.exists())
            .map(|(k, f)| {
                let pos = match f.position.as_ref().map(|r| &r.value) {
                    Some(ParamValue::Text(s)) => s.clone(),
                    _ => String::new(),
                };
                (pos, *k)
            })
            .collect();
        live.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        live.into_iter().map(|(_, k)| k).collect()
    }

    pub fn feature(&self, key: FeatureKey) -> Option<&FeatureState> {
        self.features.get(&key).filter(|f| f.exists())
    }

    /// Number of live features.
    pub fn len(&self) -> usize {
        self.features.values().filter(|f| f.exists()).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Read a parameter value if present.
    pub fn param(&self, key: FeatureKey, name: &str) -> Option<&ParamValue> {
        self.feature(key)
            .and_then(|f| f.params.get(name))
            .map(|r| &r.value)
    }

    /// Collect all add-tags currently observed for a feature (for building a
    /// causal `RemoveFeature`).
    pub fn observed_tags(&self, key: FeatureKey) -> Vec<HybridClock> {
        self.features
            .get(&key)
            .map(|f| f.add_tags.iter().copied().collect())
            .unwrap_or_default()
    }
}

/// A convenience builder that owns a replica's clock and emits ops for the local
/// user, applying them to a local [`CrdtDoc`].
#[derive(Debug)]
pub struct LocalReplica {
    pub id: ReplicaId,
    clock: HybridClock,
    pub doc: CrdtDoc,
    next_key: FeatureKey,
}

impl LocalReplica {
    pub fn new(id: ReplicaId) -> Self {
        LocalReplica {
            id,
            clock: HybridClock::new(id),
            doc: CrdtDoc::new(),
            next_key: (id.0 << 32) + 1,
        }
    }

    fn tick(&mut self) -> HybridClock {
        self.clock.tick(0)
    }

    /// Observe a remote op's clock to keep causality, then it can be applied.
    pub fn receive(&mut self, op: &Op) -> bool {
        if let Some(c) = op.clock() {
            self.clock.observe(&c);
        }
        self.doc.apply(op)
    }

    /// Locally add a feature, returning `(key, op)`.
    pub fn add_feature(&mut self, kind: &str, position: &str) -> (FeatureKey, Op) {
        let key = self.next_key;
        self.next_key += 1;
        let tag = self.tick();
        let op = Op::AddFeature {
            key,
            kind: kind.to_string(),
            tag,
            position: position.to_string(),
        };
        self.doc.apply(&op);
        (key, op)
    }

    /// Locally set a parameter, returning the op to broadcast.
    pub fn set_param(&mut self, key: FeatureKey, name: &str, value: ParamValue) -> Op {
        let clock = self.tick();
        let op = Op::SetParam {
            key,
            name: name.to_string(),
            value,
            clock,
        };
        self.doc.apply(&op);
        op
    }

    /// Locally remove a feature, returning the op to broadcast.
    pub fn remove_feature(&mut self, key: FeatureKey) -> Op {
        let observed_tags = self.doc.observed_tags(key);
        let op = Op::RemoveFeature { key, observed_tags };
        self.doc.apply(&op);
        op
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_read_feature() {
        let mut r = LocalReplica::new(ReplicaId(1));
        let (k, _) = r.add_feature("Extrude", "a");
        assert_eq!(r.doc.len(), 1);
        assert!(r.doc.feature(k).is_some());
    }

    #[test]
    fn concurrent_param_edits_converge_lww() {
        let mut a = LocalReplica::new(ReplicaId(1));
        let (k, add) = a.add_feature("Extrude", "a");
        let mut b = LocalReplica::new(ReplicaId(2));
        b.receive(&add);

        // Concurrent edits to the same param on both replicas.
        let oa = a.set_param(k, "height", ParamValue::Number(3.0));
        let ob = b.set_param(k, "height", ParamValue::Number(9.0));

        // Exchange ops.
        b.receive(&oa);
        a.receive(&ob);

        // Both converge to the same value.
        assert_eq!(a.doc.param(k, "height"), b.doc.param(k, "height"));
    }

    #[test]
    fn concurrent_add_survives_remove_it_did_not_observe() {
        let mut a = LocalReplica::new(ReplicaId(1));
        let (k, add) = a.add_feature("Extrude", "a");
        let mut b = LocalReplica::new(ReplicaId(2));
        b.receive(&add);

        // a removes; concurrently b re-adds a param edit is fine, but test a
        // fresh concurrent add on the same key path via a second add tag.
        let rem = a.remove_feature(k);
        // b adds a new tag to the same key (simulating concurrent re-add).
        let tag = b.clock.tick(0);
        let readd = Op::AddFeature {
            key: k,
            kind: "Extrude".into(),
            tag,
            position: "a".into(),
        };
        b.doc.apply(&readd);

        // Exchange.
        b.receive(&rem);
        a.receive(&readd);

        // The concurrent re-add's tag was not observed by the remove, so the
        // feature survives on both.
        assert!(a.doc.feature(k).is_some(), "survives on a");
        assert!(b.doc.feature(k).is_some(), "survives on b");
    }

    #[test]
    fn apply_is_idempotent_and_order_independent() {
        let mut a = LocalReplica::new(ReplicaId(1));
        let (k, add) = a.add_feature("Extrude", "m");
        let p1 = a.set_param(k, "height", ParamValue::Number(2.0));
        let p2 = a.set_param(k, "width", ParamValue::Number(5.0));

        let mut d1 = CrdtDoc::new();
        for op in [&add, &p1, &p2] {
            d1.apply(op);
            d1.apply(op); // idempotent
        }
        let mut d2 = CrdtDoc::new();
        for op in [&p2, &p1, &add] {
            d2.apply(op);
        }
        assert_eq!(d1.param(k, "height"), d2.param(k, "height"));
        assert_eq!(d1.param(k, "width"), d2.param(k, "width"));
        assert_eq!(d1.len(), d2.len());
    }

    #[test]
    fn ordered_keys_follow_position() {
        let mut r = LocalReplica::new(ReplicaId(1));
        let (k1, _) = r.add_feature("A", "b");
        let (k2, _) = r.add_feature("B", "a");
        assert_eq!(r.doc.ordered_keys(), vec![k2, k1]);
    }

    #[test]
    fn state_merge_matches_op_apply() {
        let mut a = LocalReplica::new(ReplicaId(1));
        let (k, _) = a.add_feature("Extrude", "a");
        a.set_param(k, "height", ParamValue::Number(7.0));

        let mut b = CrdtDoc::new();
        b.merge(&a.doc);
        assert_eq!(b.param(k, "height"), Some(&ParamValue::Number(7.0)));
    }
}
