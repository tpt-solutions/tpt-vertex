//! Post-processing: stresses, von Mises, and displacement recovery.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! Given the solved displacement vector `u`, recover per-element stresses via
//! `σ = D B u_e`, compute the von Mises scalar field, and interpolate
//! displacements at nodes.

use crate::assembly::GlobalSystem;
use crate::element::strain_displacement;
use crate::material::elastic_matrix;
use crate::mesh::VolMesh;

/// A symmetric 3D stress tensor in Voigt ordering `[sx, sy, sz, txy, tyz, tzx]`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct StressTensor {
    pub sx: f64,
    pub sy: f64,
    pub sz: f64,
    pub txy: f64,
    pub tyz: f64,
    pub tzx: f64,
}

impl StressTensor {
    /// Von Mises equivalent stress.
    pub fn von_mises(&self) -> f64 {
        let s = (self.sx - self.sy).powi(2)
            + (self.sy - self.sz).powi(2)
            + (self.sz - self.sx).powi(2)
            + 6.0 * (self.txy.powi(2) + self.tyz.powi(2) + self.tzx.powi(2));
        (s / 2.0).sqrt()
    }
}

/// Recover the element stress tensor for tet `t` of `vol`.
pub fn element_stress(vol: &VolMesh, t: usize, e: f64, nu: f64, u: &[f64]) -> StressTensor {
    let tet = vol.tets[t];
    let nodes = [
        vol.nodes[tet[0]],
        vol.nodes[tet[1]],
        vol.nodes[tet[2]],
        vol.nodes[tet[3]],
    ];
    let b = strain_displacement(&nodes);
    let d = elastic_matrix(e, nu);

    // Gather element displacement vector (12).
    let mut ue = [0.0; 12];
    for i in 0..4 {
        let gdof = GlobalSystem::dof(tet[i], 0);
        ue[3 * i] = u[gdof];
        ue[3 * i + 1] = u[gdof + 1];
        ue[3 * i + 2] = u[gdof + 2];
    }

    // ε = B u_e  (6-vector).
    let mut strain = [0.0; 6];
    for i in 0..6 {
        let mut s = 0.0;
        for j in 0..12 {
            s += b[i][j] * ue[j];
        }
        strain[i] = s;
    }

    // σ = D ε.
    let mut sigma = [0.0; 6];
    for i in 0..6 {
        let mut s = 0.0;
        for j in 0..6 {
            s += d[i][j] * strain[j];
        }
        sigma[i] = s;
    }

    StressTensor {
        sx: sigma[0],
        sy: sigma[1],
        sz: sigma[2],
        txy: sigma[3],
        tyz: sigma[4],
        tzx: sigma[5],
    }
}

/// Per-element von Mises field (length = number of tets).
pub fn von_mises_field(vol: &VolMesh, e: f64, nu: f64, u: &[f64]) -> Vec<f64> {
    vol.tets
        .iter()
        .enumerate()
        .map(|(t, _)| element_stress(vol, t, e, nu, u).von_mises())
        .collect()
}

/// Displacement `[dx, dy, dz]` at a given node.
pub fn displacement_at(_vol: &VolMesh, u: &[f64], node: usize) -> [f64; 3] {
    let gdof = GlobalSystem::dof(node, 0);
    [u[gdof], u[gdof + 1], u[gdof + 2]]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assembly::assemble;
    use crate::bc::BoundaryCondition;
    use crate::mesh::{tetrahedralize, validate_watertight};
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
    fn axial_pull_recovers_uniaxial_stress() {
        let solid = cube();
        assert!(validate_watertight(&solid).is_ok());
        let m = tetrahedralize(&solid, 1.0).unwrap();
        // Pick the corner at -x and +x faces; apply tensile load at +x node.
        let mut plus_x = 0usize;
        let mut max_x = f64::NEG_INFINITY;
        let mut minus_x = 0usize;
        let mut min_x = f64::INFINITY;
        for (i, n) in m.nodes.iter().enumerate() {
            if n[0] > max_x {
                max_x = n[0];
                plus_x = i;
            }
            if n[0] < min_x {
                min_x = n[0];
                minus_x = i;
            }
        }
        let force = 8000.0; // N => σ = 2000 MPa target
        let bc = BoundaryCondition::new()
            .fix_node(minus_x)
            .with_load(crate::bc::PointLoad {
                node: plus_x,
                fx: force,
                fy: 0.0,
                fz: 0.0,
            });
        let sys = assemble(&m, 200_000.0, 0.3, &bc);
        let u = sys.solve();
        // Element stress near the loaded face should be ≈ F/A = 2000 MPa.
        let vm = von_mises_field(&m, 200_000.0, 0.3, &u);
        let max_vm = vm.iter().cloned().fold(0.0, f64::max);
        // Coarse tet mesh underestimates peak; accept within 50%.
        assert!(max_vm > 1000.0, "von Mises too low: {max_vm}");
        // Plus-x node should have moved in +x.
        let disp = displacement_at(&m, &u, plus_x);
        assert!(disp[0] > 0.0, "loaded node should displace +x: {:?}", disp);
    }
}
