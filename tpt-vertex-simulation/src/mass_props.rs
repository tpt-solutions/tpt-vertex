//! Mass properties (volume, center of mass, inertia tensor) of a closed solid.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! Computes exact mass properties of a watertight triangle-mesh [`Solid`] by
//! summing signed tetrahedra formed between the world origin and each face
//! (the standard divergence-theorem technique used for STL volume/inertia
//! calculators; consistent face winding makes the signed sum cancel correctly
//! for concave shapes). Per-tetrahedron moment integrals use the closed-form
//! formulas for a tetrahedron with one vertex at the origin (Tonon, 2004,
//! "Explicit Exact Formulas for the 3-D Tetrahedron Inertia Tensor in Terms
//! of its Vertex Coordinates").

use tpt_vertex_kernel::geometry::solid::Solid;
use tpt_vertex_kernel::math::{Mat3, Vec3};

/// Mass, center of mass, and inertia tensor (about the center of mass, in the
/// solid's own coordinate frame) of a watertight solid at a given density.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MassProperties {
    /// Mass (g, since kernel density is g/mm³ and lengths are mm).
    pub mass: f64,
    /// Volume (mm³).
    pub volume: f64,
    /// Center of mass, in the same frame as the solid's vertices.
    pub center_of_mass: Vec3,
    /// Inertia tensor about the center of mass (g·mm²), in the solid's frame.
    pub inertia_com: Mat3,
}

/// Compute mass properties of `solid` at `density` (g/mm³).
///
/// Returns a zero-mass result if the solid encloses no volume.
pub fn compute(solid: &Solid, density: f64) -> MassProperties {
    let mut vol6_sum = 0.0f64;
    let mut m1 = Vec3::ZERO; // sum of ∫x dV, ∫y dV, ∫z dV (volume-space, undensified)
    let (mut ixx, mut iyy, mut izz) = (0.0f64, 0.0f64, 0.0f64);
    let (mut ixy, mut ixz, mut iyz) = (0.0f64, 0.0f64, 0.0f64);

    for f in &solid.faces {
        let p1 = solid.vertices[f.a as usize];
        let p2 = solid.vertices[f.b as usize];
        let p3 = solid.vertices[f.c as usize];
        let (x1, y1, z1) = (p1.x, p1.y, p1.z);
        let (x2, y2, z2) = (p2.x, p2.y, p2.z);
        let (x3, y3, z3) = (p3.x, p3.y, p3.z);

        // 6 * signed volume of the tetrahedron (origin, p1, p2, p3).
        let v6 = x1 * (y2 * z3 - y3 * z2) - y1 * (x2 * z3 - x3 * z2) + z1 * (x2 * y3 - x3 * y2);
        vol6_sum += v6;

        m1.x += v6 / 24.0 * (x1 + x2 + x3);
        m1.y += v6 / 24.0 * (y1 + y2 + y3);
        m1.z += v6 / 24.0 * (z1 + z2 + z3);

        ixx += v6 / 60.0
            * (y1 * y1 + y2 * y2 + y3 * y3 + y1 * y2 + y2 * y3 + y3 * y1
                + z1 * z1 + z2 * z2 + z3 * z3 + z1 * z2 + z2 * z3 + z3 * z1);
        iyy += v6 / 60.0
            * (x1 * x1 + x2 * x2 + x3 * x3 + x1 * x2 + x2 * x3 + x3 * x1
                + z1 * z1 + z2 * z2 + z3 * z3 + z1 * z2 + z2 * z3 + z3 * z1);
        izz += v6 / 60.0
            * (x1 * x1 + x2 * x2 + x3 * x3 + x1 * x2 + x2 * x3 + x3 * x1
                + y1 * y1 + y2 * y2 + y3 * y3 + y1 * y2 + y2 * y3 + y3 * y1);
        ixy += v6 / 120.0
            * (2.0 * x1 * y1 + 2.0 * x2 * y2 + 2.0 * x3 * y3
                + x1 * y2 + x2 * y1
                + x1 * y3 + x3 * y1
                + x2 * y3 + x3 * y2);
        ixz += v6 / 120.0
            * (2.0 * x1 * z1 + 2.0 * x2 * z2 + 2.0 * x3 * z3
                + x1 * z2 + x2 * z1
                + x1 * z3 + x3 * z1
                + x2 * z3 + x3 * z2);
        iyz += v6 / 120.0
            * (2.0 * y1 * z1 + 2.0 * y2 * z2 + 2.0 * y3 * z3
                + y1 * z2 + y2 * z1
                + y1 * z3 + y3 * z1
                + y2 * z3 + y3 * z2);
    }

    let volume = vol6_sum / 6.0;
    if volume.abs() < 1e-15 {
        return MassProperties {
            mass: 0.0,
            volume: 0.0,
            center_of_mass: Vec3::ZERO,
            inertia_com: Mat3::identity(),
        };
    }
    let com = m1 * (1.0 / volume);

    // Inertia tensor about the origin (volume-space units, tensor convention:
    // diagonal = ∫(y²+z²)dV etc., off-diagonal = -∫xy dV etc.).
    let i_origin = [
        [ixx, -ixy, -ixz],
        [-ixy, iyy, -iyz],
        [-ixz, -iyz, izz],
    ];

    // Parallel-axis theorem: I_origin = I_com + V*(|c|²·I3 - c⊗c)
    //                      => I_com = I_origin - V*(|c|²·I3 - c⊗c)
    let c = [com.x, com.y, com.z];
    let c_dot = c[0] * c[0] + c[1] * c[1] + c[2] * c[2];
    let mut i_com = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            let delta = if i == j { 1.0 } else { 0.0 };
            i_com[i][j] = i_origin[i][j] - volume * (delta * c_dot - c[i] * c[j]);
        }
    }

    let mass = density * volume;
    let inertia_com = Mat3::from_row_major([
        i_com[0][0] * density, i_com[0][1] * density, i_com[0][2] * density,
        i_com[1][0] * density, i_com[1][1] * density, i_com[1][2] * density,
        i_com[2][0] * density, i_com[2][1] * density, i_com[2][2] * density,
    ]);

    MassProperties {
        mass,
        volume,
        center_of_mass: com,
        inertia_com,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_vertex_kernel::geometry::solid::Face;

    fn cube(half: f64) -> Solid {
        let mut s = Solid::new();
        let h = half;
        let mut v = |x: f64, y: f64, z: f64| s.add_vertex(Vec3::new(x, y, z));
        let p = [
            v(-h, -h, -h), v(h, -h, -h), v(h, h, -h), v(-h, h, -h),
            v(-h, -h, h), v(h, -h, h), v(h, h, h), v(-h, h, h),
        ];
        // Wound so each face's normal (right-hand rule) points outward.
        let mut f = |a: u32, b: u32, c: u32| s.faces.push(Face::new(a, c, b));
        f(p[0], p[1], p[2]); f(p[0], p[2], p[3]);
        f(p[4], p[6], p[5]); f(p[4], p[7], p[6]);
        f(p[0], p[5], p[1]); f(p[0], p[4], p[5]);
        f(p[1], p[6], p[2]); f(p[1], p[5], p[6]);
        f(p[2], p[7], p[3]); f(p[2], p[6], p[7]);
        f(p[3], p[4], p[0]); f(p[3], p[7], p[4]);
        s
    }

    #[test]
    fn unit_cube_volume_and_com() {
        // Cube of side 1 centered at origin: half-extent 0.5.
        let mp = compute(&cube(0.5), 1.0);
        assert!((mp.volume - 1.0).abs() < 1e-9, "volume {}", mp.volume);
        assert!((mp.mass - 1.0).abs() < 1e-9);
        assert!(mp.center_of_mass.length() < 1e-9, "com {:?}", mp.center_of_mass);
    }

    #[test]
    fn unit_cube_inertia_matches_closed_form() {
        // I = m*s²/6 per axis about the center, zero off-diagonal, for a cube of side s.
        let s = 2.0;
        let mp = compute(&cube(s / 2.0), 1.0);
        let expected = mp.mass * s * s / 6.0;
        assert!((mp.inertia_com.cols[0].x - expected).abs() < 1e-6, "Ixx {}", mp.inertia_com.cols[0].x);
        assert!((mp.inertia_com.cols[1].y - expected).abs() < 1e-6, "Iyy {}", mp.inertia_com.cols[1].y);
        assert!((mp.inertia_com.cols[2].z - expected).abs() < 1e-6, "Izz {}", mp.inertia_com.cols[2].z);
        assert!(mp.inertia_com.cols[1].x.abs() < 1e-6, "Ixy {}", mp.inertia_com.cols[1].x);
        assert!(mp.inertia_com.cols[2].x.abs() < 1e-6, "Ixz {}", mp.inertia_com.cols[2].x);
        assert!(mp.inertia_com.cols[2].y.abs() < 1e-6, "Iyz {}", mp.inertia_com.cols[2].y);
    }

    #[test]
    fn offset_cube_com_matches_translation() {
        let mut s = cube(0.5);
        for v in &mut s.vertices {
            *v = *v + Vec3::new(5.0, -2.0, 1.0);
        }
        let mp = compute(&s, 2.0);
        assert!((mp.volume - 1.0).abs() < 1e-9);
        assert!((mp.mass - 2.0).abs() < 1e-9);
        let d = mp.center_of_mass - Vec3::new(5.0, -2.0, 1.0);
        assert!(d.length() < 1e-9, "com {:?}", mp.center_of_mass);
    }
}
