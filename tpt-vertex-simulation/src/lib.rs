//! Simulation for TPT Vertex: static FEA and assembly motion.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! Provides linear-elastic static-stress analysis (small deformation, isotropic
//! material) over a tetrahedralized volume mesh derived from a kernel [`Solid`],
//! plus forward-kinematics motion playback over kernel `Assembly` mates.
//!
//! Pipeline: [`mesh::validate_watertight`] → [`mesh::tetrahedralize`] →
//! [`material::elastic_matrix`] → [`assembly::assemble`] → [`solve`] →
//! [`post`] (stresses / von Mises / displacements). See the Phase-11 ADR for the
//! scope decision and the `faer` sparse-solver fast-follow.

pub mod adaptivity;
pub mod assembly;
pub mod bc;
pub mod buckling;
pub mod contact;
pub mod contact_fea;
pub mod dynamics;
pub mod element;
pub mod fatigue;
pub mod mass_props;
pub mod material;
pub mod mesh;
pub mod modal;
pub mod motion;
pub mod nonlinear;
pub mod plasticity;
pub mod post;
pub mod quadratic_tet;
pub mod solve;
pub mod thermal;
pub mod topo_opt;
pub mod wasm_api;

use tpt_vertex_kernel::geometry::solid::Solid;
use tpt_vertex_kernel::material::Material;

pub use bc::BoundaryCondition;
pub use mesh::VolMesh;
pub use motion::MotionPlayer;

/// Configuration for a static analysis run.
#[derive(Debug, Clone)]
pub struct AnalysisSettings {
    /// Material properties (E, ν, density, strength).
    pub material: Material,
    /// Boundary conditions (fixed nodes + loads).
    pub bc: BoundaryCondition,
    /// Target tetrahedral edge length in mm (smaller = finer mesh).
    pub max_tet_edge: f64,
}

impl AnalysisSettings {
    pub fn new(material: Material, bc: BoundaryCondition, max_tet_edge: f64) -> Self {
        AnalysisSettings {
            material,
            bc,
            max_tet_edge,
        }
    }
}

/// Outcome of a static analysis.
#[derive(Debug, Clone)]
pub struct AnalysisResult {
    /// The volume mesh the solution was computed on.
    pub mesh: VolMesh,
    /// Full DOF displacement vector (length = mesh nodes × 3).
    pub displacements: Vec<f64>,
    /// Per-element von Mises stress (length = number of tets), in MPa.
    pub von_mises: Vec<f64>,
    /// Maximum nodal displacement magnitude (mm).
    pub max_displacement: f64,
    /// Maximum von Mises stress across the model (MPa).
    pub max_von_mises: f64,
}

/// Run the full static linear-elastic analysis on a kernel solid.
///
/// Returns `Err` if the solid is not watertight or cannot be meshed.
pub fn run_static_analysis(
    solid: &Solid,
    settings: &AnalysisSettings,
) -> Result<AnalysisResult, String> {
    let mesh = mesh::tetrahedralize(solid, settings.max_tet_edge)?;
    let e = settings.material.youngs_modulus;
    let nu = settings.material.poisson_ratio;

    let system = assembly::assemble(&mesh, e, nu, &settings.bc);
    let displacements = system.solve();
    let von_mises = post::von_mises_field(&mesh, e, nu, &displacements);

    let max_displacement = (0..mesh.node_count())
        .map(|n| {
            let d = post::displacement_at(&mesh, &displacements, n);
            (d[0].powi(2) + d[1].powi(2) + d[2].powi(2)).sqrt()
        })
        .fold(0.0, f64::max);
    let max_von_mises = von_mises.iter().cloned().fold(0.0, f64::max);

    Ok(AnalysisResult {
        mesh,
        displacements,
        von_mises,
        max_displacement,
        max_von_mises,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_vertex_kernel::math::Vec3;

    fn beam() -> Solid {
        // 10 (x) × 1 (y) × 1 (z) bar; cantilever fixed at x=0.
        let mut s = Solid::new();
        let mut v = |x, y, z| s.add_vertex(Vec3::new(x, y, z));
        let p = [
            v(0.0, 0.0, 0.0),
            v(10.0, 0.0, 0.0),
            v(10.0, 1.0, 0.0),
            v(0.0, 1.0, 0.0),
            v(0.0, 0.0, 1.0),
            v(10.0, 0.0, 1.0),
            v(10.0, 1.0, 1.0),
            v(0.0, 1.0, 1.0),
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
    fn cantilever_deflects_downward() {
        let solid = beam();
        let m = tetrahedralize_check(&solid);

        // Find nodes at x≈0 (fixed) and x≈10 (tip load).
        let mut tip = 0usize;
        let mut tip_x = f64::NEG_INFINITY;
        let mut fixed = Vec::new();
        for (i, n) in m.nodes.iter().enumerate() {
            if n[0] < 1e-6 {
                fixed.push(i);
            }
            if n[0] > tip_x {
                tip_x = n[0];
                tip = i;
            }
        }
        // Apply a downward (–y) point load at the tip node.
        let force = 50.0; // N
        let bc = BoundaryCondition::new()
            .fix_all(&fixed)
            .with_load(bc::PointLoad {
                node: tip,
                fx: 0.0,
                fy: -force,
                fz: 0.0,
            });

        let mat = Material::from_name("Steel"); // E=200000 MPa
        let settings = AnalysisSettings::new(mat, bc, 1.0);
        let res = run_static_analysis(&solid, &settings).expect("analysis");

        // Euler-Bernoulli tip deflection: δ = F L³ / (3 E I),
        // I = b h³/12 = 1·1³/12 = 0.0833, L=10 => δ ≈ 1.0 mm (downward).
        // A coarse linear-tet mesh with high span aspect ratio is shear-locked
        // and under-predicts; we only assert the correct *sign* and a non-trivial
        // downward motion (the structural pipeline is what is under test).
        let tip_disp = post::displacement_at(&res.mesh, &res.displacements, tip);
        assert!(
            tip_disp[1] < -1e-4,
            "tip should deflect downward, got {:?}",
            tip_disp
        );
        assert!(res.max_displacement > 0.0);
    }

    // Local helper: a validated, tetrahedralized copy for node selection.
    fn tetrahedralize_check(solid: &Solid) -> VolMesh {
        mesh::tetrahedralize(solid, 1.0).expect("mesh")
    }
}
