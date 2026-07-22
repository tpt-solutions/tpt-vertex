//! Steady-state thermal conduction and thermal stress.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! Solves steady-state heat conduction `K_t T = Q` over the same tet mesh
//! used for static FEA (one temperature DOF per node instead of three
//! displacement DOFs), then — given a thermal-expansion coefficient — builds
//! the equivalent nodal thermal load and combines it with the mechanical
//! stiffness system to recover thermally-induced stress.

use crate::assembly::GlobalSystem;
use crate::bc::BoundaryCondition;
use crate::element::{shape_gradients, strain_displacement, tet_volume};
use crate::material::{apply_d, elastic_matrix};
use crate::mesh::VolMesh;
use crate::post::StressTensor;

/// Steady-state thermal boundary conditions: prescribed nodal temperatures
/// and/or nodal heat sources (W).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ThermalBoundaryCondition {
    pub fixed_temps: Vec<(usize, f64)>,
    pub heat_sources: Vec<(usize, f64)>,
}

impl ThermalBoundaryCondition {
    pub fn new() -> Self {
        ThermalBoundaryCondition::default()
    }

    pub fn fix_temp(mut self, node: usize, temp: f64) -> Self {
        self.fixed_temps.push((node, temp));
        self
    }

    pub fn with_heat_source(mut self, node: usize, q: f64) -> Self {
        self.heat_sources.push((node, q));
        self
    }
}

/// The global steady-state conduction system: one DOF (temperature) per node.
#[derive(Debug, Clone)]
pub struct ThermalSystem {
    pub n_nodes: usize,
    pub k: Vec<f64>,
    pub f: Vec<f64>,
}

impl ThermalSystem {
    pub fn solve(&self) -> Vec<f64> {
        crate::solve::solve_dense(&self.k, &self.f, self.n_nodes)
    }
}

/// Element conductivity matrix `K_e = k · V · ∇Nᵀ∇N` (4×4).
pub fn element_conductivity(nodes: &[[f64; 3]; 4], conductivity: f64) -> [[f64; 4]; 4] {
    let g = shape_gradients(nodes);
    let vol = tet_volume(nodes).abs();
    let mut ke = [[0.0; 4]; 4];
    for i in 0..4 {
        for j in 0..4 {
            let dot = g[i][0] * g[j][0] + g[i][1] * g[j][1] + g[i][2] * g[j][2];
            ke[i][j] = conductivity * vol * dot;
        }
    }
    ke
}

/// Assemble the global steady-state conduction system and apply boundary
/// conditions via exact Dirichlet elimination (fixed temperatures need not
/// be zero, unlike the mechanical penalty-method fixation).
pub fn assemble_thermal(
    vol: &VolMesh,
    conductivity: f64,
    bc: &ThermalBoundaryCondition,
) -> ThermalSystem {
    let n = vol.node_count();
    let mut k = vec![0.0; n * n];
    let mut f = vec![0.0; n];

    for tet in &vol.tets {
        let nodes = [
            vol.nodes[tet[0]],
            vol.nodes[tet[1]],
            vol.nodes[tet[2]],
            vol.nodes[tet[3]],
        ];
        let ke = element_conductivity(&nodes, conductivity);
        for i in 0..4 {
            for j in 0..4 {
                k[tet[i] * n + tet[j]] += ke[i][j];
            }
        }
    }
    for &(node, q) in &bc.heat_sources {
        f[node] += q;
    }

    // Exact Dirichlet elimination: subtract each fixed node's known
    // contribution from every other equation's RHS before zeroing rows/cols
    // (must use the pre-zeroed matrix for every subtraction pass).
    for &(node, t) in &bc.fixed_temps {
        for j in 0..n {
            if j != node {
                f[j] -= k[j * n + node] * t;
            }
        }
    }
    for &(node, t) in &bc.fixed_temps {
        for j in 0..n {
            k[node * n + j] = 0.0;
            k[j * n + node] = 0.0;
        }
        k[node * n + node] = 1.0;
        f[node] = t;
    }

    ThermalSystem { n_nodes: n, k, f }
}

/// Run steady-state thermal conduction on `vol`, returning nodal temperatures.
pub fn solve_steady_state(
    vol: &VolMesh,
    conductivity: f64,
    bc: &ThermalBoundaryCondition,
) -> Vec<f64> {
    assemble_thermal(vol, conductivity, bc).solve()
}

/// Equivalent nodal thermal load vector `F_th = Σ_e V·Bᵀ·D·ε₀`, where
/// `ε₀ = α·(T_avg − T_ref)·[1,1,1,0,0,0]` is the free-thermal-expansion
/// eigenstrain of each element (from its average nodal temperature).
pub fn thermal_load_vector(
    vol: &VolMesh,
    e: f64,
    nu: f64,
    temperatures: &[f64],
    alpha: f64,
    t_ref: f64,
) -> Vec<f64> {
    let n_dofs = vol.node_count() * 3;
    let mut f = vec![0.0; n_dofs];
    let d = elastic_matrix(e, nu);

    for tet in &vol.tets {
        let nodes = [
            vol.nodes[tet[0]],
            vol.nodes[tet[1]],
            vol.nodes[tet[2]],
            vol.nodes[tet[3]],
        ];
        let t_avg = (temperatures[tet[0]]
            + temperatures[tet[1]]
            + temperatures[tet[2]]
            + temperatures[tet[3]])
            / 4.0;
        let eps0 = thermal_eigenstrain(alpha, t_avg, t_ref);
        let d_eps0 = apply_d(&d, eps0);
        let b = strain_displacement(&nodes);
        let vol_e = tet_volume(&nodes).abs();

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
            let mut s = 0.0;
            for k_ in 0..6 {
                s += b[k_][i] * d_eps0[k_];
            }
            f[gdof[i]] += vol_e * s;
        }
    }
    f
}

fn thermal_eigenstrain(alpha: f64, t_avg: f64, t_ref: f64) -> [f64; 6] {
    let e0 = alpha * (t_avg - t_ref);
    [e0, e0, e0, 0.0, 0.0, 0.0]
}

/// Recover the total stress in tet `t` given the solved displacement field
/// `u` and the nodal `temperatures`: `σ = D·(B·u − ε₀)`.
#[allow(clippy::too_many_arguments)] // mirrors the FEA material parameter set used throughout this crate
pub fn element_thermal_stress(
    vol: &VolMesh,
    t: usize,
    e: f64,
    nu: f64,
    u: &[f64],
    temperatures: &[f64],
    alpha: f64,
    t_ref: f64,
) -> StressTensor {
    let tet = vol.tets[t];
    let nodes = [
        vol.nodes[tet[0]],
        vol.nodes[tet[1]],
        vol.nodes[tet[2]],
        vol.nodes[tet[3]],
    ];
    let b = strain_displacement(&nodes);
    let d = elastic_matrix(e, nu);

    let mut ue = [0.0; 12];
    for i in 0..4 {
        let gdof = GlobalSystem::dof(tet[i], 0);
        ue[3 * i] = u[gdof];
        ue[3 * i + 1] = u[gdof + 1];
        ue[3 * i + 2] = u[gdof + 2];
    }
    let mut strain = [0.0; 6];
    for i in 0..6 {
        let mut s = 0.0;
        for j in 0..12 {
            s += b[i][j] * ue[j];
        }
        strain[i] = s;
    }
    let t_avg =
        (temperatures[tet[0]] + temperatures[tet[1]] + temperatures[tet[2]] + temperatures[tet[3]])
            / 4.0;
    let eps0 = thermal_eigenstrain(alpha, t_avg, t_ref);
    let mech_strain: Vec<f64> = strain
        .iter()
        .zip(eps0.iter())
        .map(|(s, e0)| s - e0)
        .collect();
    let sigma = apply_d(
        &d,
        [
            mech_strain[0],
            mech_strain[1],
            mech_strain[2],
            mech_strain[3],
            mech_strain[4],
            mech_strain[5],
        ],
    );

    StressTensor {
        sx: sigma[0],
        sy: sigma[1],
        sz: sigma[2],
        txy: sigma[3],
        tyz: sigma[4],
        tzx: sigma[5],
    }
}

/// Settings for a combined steady-state-thermal + thermal-stress run.
#[derive(Debug, Clone)]
pub struct ThermalStressSettings {
    pub conductivity: f64,
    pub thermal_bc: ThermalBoundaryCondition,
    /// Thermal expansion coefficient (1/°C).
    pub alpha: f64,
    /// Reference (stress-free) temperature.
    pub t_ref: f64,
    /// Mechanical fixed nodes (+ any additional mechanical point loads).
    pub mech: BoundaryCondition,
    pub youngs_modulus: f64,
    pub poisson_ratio: f64,
    pub max_tet_edge: f64,
}

/// Result of a combined thermal-stress analysis.
#[derive(Debug, Clone)]
pub struct ThermalStressResult {
    pub mesh: VolMesh,
    pub temperatures: Vec<f64>,
    pub displacements: Vec<f64>,
    pub von_mises: Vec<f64>,
}

/// Run steady-state thermal conduction, then the resulting thermal-stress
/// analysis, on `solid`.
pub fn run_thermal_stress_analysis(
    solid: &tpt_vertex_kernel::geometry::solid::Solid,
    settings: &ThermalStressSettings,
) -> Result<ThermalStressResult, String> {
    let mesh = crate::mesh::tetrahedralize(solid, settings.max_tet_edge)?;
    let temperatures = solve_steady_state(&mesh, settings.conductivity, &settings.thermal_bc);

    let mut sys = crate::assembly::assemble(
        &mesh,
        settings.youngs_modulus,
        settings.poisson_ratio,
        &settings.mech,
    );
    let f_th = thermal_load_vector(
        &mesh,
        settings.youngs_modulus,
        settings.poisson_ratio,
        &temperatures,
        settings.alpha,
        settings.t_ref,
    );
    for (fi, fth_i) in sys.f.iter_mut().zip(f_th.iter()) {
        *fi += fth_i;
    }
    // Re-clamp fixed DOFs to zero: `assemble` already zeroed their rows/cols
    // and RHS to encode u=0 there; adding f_th must not perturb that.
    for &node in &settings.mech.fixed_nodes {
        for a in 0..3 {
            sys.f[GlobalSystem::dof(node, a)] = 0.0;
        }
    }
    let displacements = sys.solve();

    let von_mises: Vec<f64> = (0..mesh.tet_count())
        .map(|t| {
            element_thermal_stress(
                &mesh,
                t,
                settings.youngs_modulus,
                settings.poisson_ratio,
                &displacements,
                &temperatures,
                settings.alpha,
                settings.t_ref,
            )
            .von_mises()
        })
        .collect();

    Ok(ThermalStressResult {
        mesh,
        temperatures,
        displacements,
        von_mises,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_vertex_kernel::geometry::solid::{Face, Solid};
    use tpt_vertex_kernel::math::Vec3;

    pub(super) fn cube() -> Solid {
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
        let mut f = |a, b, c| s.faces.push(Face::new(a, b, c));
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
    fn steady_state_1d_conduction_is_linear() {
        // Fix T=100 at the -x face nodes, T=0 at the +x face nodes; with no
        // internal sources the steady-state field is linear in x, so every
        // node's temperature should match its analytical x-based value.
        let m = crate::mesh::tetrahedralize(&cube(), 1.0).unwrap();
        let mut bc = ThermalBoundaryCondition::new();
        let (mut min_x, mut max_x) = (f64::INFINITY, f64::NEG_INFINITY);
        for n in &m.nodes {
            min_x = min_x.min(n[0]);
            max_x = max_x.max(n[0]);
        }
        for (i, n) in m.nodes.iter().enumerate() {
            if (n[0] - min_x).abs() < 1e-9 {
                bc = bc.fix_temp(i, 100.0);
            } else if (n[0] - max_x).abs() < 1e-9 {
                bc = bc.fix_temp(i, 0.0);
            }
        }
        let t = solve_steady_state(&m, 50.0, &bc);
        for (i, n) in m.nodes.iter().enumerate() {
            let expected = 100.0 * (max_x - n[0]) / (max_x - min_x);
            assert!(
                (t[i] - expected).abs() < 1e-6,
                "node {i} at x={} got {} expected {}",
                n[0],
                t[i],
                expected
            );
        }
    }

    #[test]
    fn fully_clamped_bar_under_heating_is_compressive() {
        // A cube clamped (all DOFs) at both x-end faces, uniformly heated:
        // prevented expansion should produce compressive (negative) sigma_xx
        // in the interior, with magnitude comparable to the 1D closed form
        // sigma = -E*alpha*dT (coarse tet mesh, so only sign/order-of-magnitude
        // is checked, consistent with this crate's other coarse-mesh tests).
        let solid = cube();
        let m = crate::mesh::tetrahedralize(&solid, 1.0).unwrap();
        let (mut min_x, mut max_x) = (f64::INFINITY, f64::NEG_INFINITY);
        for n in &m.nodes {
            min_x = min_x.min(n[0]);
            max_x = max_x.max(n[0]);
        }
        let mut fixed = Vec::new();
        for (i, n) in m.nodes.iter().enumerate() {
            if (n[0] - min_x).abs() < 1e-9 || (n[0] - max_x).abs() < 1e-9 {
                fixed.push(i);
            }
        }
        let mech = BoundaryCondition::new().fix_all(&fixed);
        let settings = ThermalStressSettings {
            conductivity: 50.0,
            thermal_bc: ThermalBoundaryCondition::new(), // no thermal BCs: uniform ΔT via t_ref offset alone
            alpha: 1.2e-5,
            t_ref: 0.0,
            mech,
            youngs_modulus: 200_000.0,
            poisson_ratio: 0.3,
            max_tet_edge: 1.0,
        };
        // Uniform temperature field (no conduction gradient): every node at 100°C.
        let uniform_temp = vec![100.0; m.node_count()];
        let f_th = thermal_load_vector(
            &m,
            settings.youngs_modulus,
            settings.poisson_ratio,
            &uniform_temp,
            settings.alpha,
            settings.t_ref,
        );
        let mut sys = crate::assembly::assemble(
            &m,
            settings.youngs_modulus,
            settings.poisson_ratio,
            &settings.mech,
        );
        for (fi, fth_i) in sys.f.iter_mut().zip(f_th.iter()) {
            *fi += fth_i;
        }
        for &node in &settings.mech.fixed_nodes {
            for a in 0..3 {
                sys.f[GlobalSystem::dof(node, a)] = 0.0;
            }
        }
        let u = sys.solve();
        let mut min_sx = f64::INFINITY;
        for t in 0..m.tet_count() {
            let s = element_thermal_stress(
                &m,
                t,
                settings.youngs_modulus,
                settings.poisson_ratio,
                &u,
                &uniform_temp,
                settings.alpha,
                settings.t_ref,
            );
            min_sx = min_sx.min(s.sx);
        }
        assert!(
            min_sx < -10.0,
            "expected compressive sigma_xx, got min {min_sx}"
        );
    }
}
