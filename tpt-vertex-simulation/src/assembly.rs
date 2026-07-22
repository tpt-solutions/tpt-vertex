//! Global sparse/dense stiffness assembly and boundary-condition application.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! Assembles the linear system `K u = F` over all free DOFs and applies
//! constraints by the penalty method (fixed DOFs are pinned to zero
//! displacement). The global matrix is stored dense for v1; swapping in a sparse
//! solver is a contained change here + in [`crate::solve`].

use crate::bc::BoundaryCondition;
use crate::element::tet_stiffness;
use crate::material::elastic_matrix;
use crate::mesh::VolMesh;

/// Row-major dense `n×n` matrix stored as a flat vector (length `n*n`).
pub type DenseMatrix = Vec<f64>;

/// The global linear system for static FEA.
#[derive(Debug, Clone)]
pub struct GlobalSystem {
    /// Number of nodes.
    pub n_nodes: usize,
    /// Total DOFs (3 per node).
    pub n_dofs: usize,
    /// Dense stiffness matrix, row-major (`n_dofs × n_dofs`).
    pub k: DenseMatrix,
    /// Global load vector (length `n_dofs`).
    pub f: Vec<f64>,
    /// Penalty scale used for fixed DOFs (diagnostics only).
    pub penalty: f64,
}

impl GlobalSystem {
    /// DOF index for node `n`, axis `a` (0=x, 1=y, 2=z).
    pub fn dof(n: usize, a: usize) -> usize {
        n * 3 + a
    }

    /// Solve the system, returning the displacement vector (length `n_dofs`).
    pub fn solve(&self) -> Vec<f64> {
        crate::solve::solve_dense(&self.k, &self.f, self.n_dofs)
    }
}

/// Assemble the global system for `vol` under material `(e, nu)` and BCs.
pub fn assemble(vol: &VolMesh, e: f64, nu: f64, bc: &BoundaryCondition) -> GlobalSystem {
    let n_nodes = vol.node_count();
    let n_dofs = n_nodes * 3;
    let d = elastic_matrix(e, nu);

    let mut k = vec![0.0; n_dofs * n_dofs];
    let mut f = vec![0.0; n_dofs];

    // Element assembly.
    for tet in &vol.tets {
        let nodes = [
            vol.nodes[tet[0]],
            vol.nodes[tet[1]],
            vol.nodes[tet[2]],
            vol.nodes[tet[3]],
        ];
        let ke = tet_stiffness(&nodes, &d);
        let gdof = [
            GlobalSystem::dof(tet[0], 0),
            GlobalSystem::dof(tet[0], 1),
            GlobalSystem::dof(tet[0], 2),
            GlobalSystem::dof(tet[1], 0),
            GlobalSystem::dof(tet[1], 1),
            GlobalSystem::dof(tet[1], 2),
            GlobalSystem::dof(tet[2], 0),
            GlobalSystem::dof(tet[2], 1),
            GlobalSystem::dof(tet[2], 2),
            GlobalSystem::dof(tet[3], 0),
            GlobalSystem::dof(tet[3], 1),
            GlobalSystem::dof(tet[3], 2),
        ];
        for i in 0..12 {
            for j in 0..12 {
                k[gdof[i] * n_dofs + gdof[j]] += ke[i][j];
            }
        }
    }

    // Loads.
    for load in &bc.loads {
        let gdof = GlobalSystem::dof(load.node, 0);
        f[gdof] += load.fx;
        f[gdof + 1] += load.fy;
        f[gdof + 2] += load.fz;
    }

    // Penalty constraint application for fixed nodes.
    let mut kmax: f64 = 0.0;
    for &v in &k {
        kmax = kmax.max(v.abs());
    }
    let penalty = kmax * 1e12 + 1.0;
    for &node in &bc.fixed_nodes {
        for a in 0..3 {
            let d0 = GlobalSystem::dof(node, a);
            for j in 0..n_dofs {
                k[d0 * n_dofs + j] = 0.0;
                k[j * n_dofs + d0] = 0.0;
            }
            k[d0 * n_dofs + d0] = penalty;
            f[d0] = 0.0;
        }
    }

    GlobalSystem {
        n_nodes,
        n_dofs,
        k,
        f,
        penalty,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::tetrahedralize;
    use tpt_vertex_kernel::geometry::solid::Solid;
    use tpt_vertex_kernel::math::Vec3;

    fn cube() -> Solid {
        let mut s = Solid::new();
        let mut v = |x, y, z| s.add_vertex(Vec3::new(x, y, z));
        let p = [
            v(-1.0, -1.0, -1.0),
            v(1.0, -1.0, -1.0),
            v(1.0, 1.0, -1.0),
            v(-1.0, 1.0, -1.0),
            v(-1.0, -1.0, 1.0),
            v(1.0, -1.0, 1.0),
            v(1.0, 1.0, 1.0),
            v(-1.0, 1.0, 1.0),
        ];
        let mut f = |a, b, c| {
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
    fn assemble_size_matches() {
        let m = tetrahedralize(&cube(), 1.0).unwrap();
        let bc = BoundaryCondition::new().fix_node(0);
        let sys = assemble(&m, 200_000.0, 0.3, &bc);
        assert_eq!(sys.n_dofs, m.node_count() * 3);
        assert_eq!(sys.k.len(), sys.n_dofs * sys.n_dofs);
    }
}
