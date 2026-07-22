//! Boundary conditions for static FEA.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! A [`BoundaryCondition`] collects fixed (constrained) nodes and applied loads.
//! Each node has three translational DOFs (x, y, z), so a fixed node constrains
//! all three. Loads are expressed as nodal forces (point loads or accumulated
//! surface/body forces).

/// A force applied to a single node (units: N).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PointLoad {
    pub node: usize,
    pub fx: f64,
    pub fy: f64,
    pub fz: f64,
}

/// A boundary condition: which nodes are fixed, and what loads are applied.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BoundaryCondition {
    /// Nodes whose displacement is pinned to zero in all three DOFs.
    pub fixed_nodes: Vec<usize>,
    /// Applied nodal point loads.
    pub loads: Vec<PointLoad>,
}

impl BoundaryCondition {
    pub fn new() -> Self {
        BoundaryCondition::default()
    }

    /// Pin the given node in all three DOFs.
    pub fn fix_node(mut self, node: usize) -> Self {
        if !self.fixed_nodes.contains(&node) {
            self.fixed_nodes.push(node);
        }
        self
    }

    /// Pin every node in `nodes`.
    pub fn fix_all(mut self, nodes: &[usize]) -> Self {
        for &n in nodes {
            self = self.fix_node(n);
        }
        self
    }

    /// Add a point load at a node.
    pub fn with_load(mut self, load: PointLoad) -> Self {
        self.loads.push(load);
        self
    }

    /// True if node `n` is fixed.
    pub fn is_fixed(&self, n: usize) -> bool {
        self.fixed_nodes.contains(&n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_and_dedup() {
        let bc = BoundaryCondition::new()
            .fix_node(3)
            .fix_node(3)
            .fix_node(7)
            .with_load(PointLoad {
                node: 1,
                fx: 0.0,
                fy: -10.0,
                fz: 0.0,
            });
        assert_eq!(bc.fixed_nodes.len(), 2);
        assert!(bc.is_fixed(7));
        assert!(!bc.is_fixed(0));
        assert_eq!(bc.loads.len(), 1);
    }
}
