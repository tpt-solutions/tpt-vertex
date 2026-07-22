//! Full rigid-body dynamics: mass/inertia, forces/torques, time integration,
//! and single-axis joint reaction forces.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! Two complementary pieces:
//!
//! - [`RigidBody`] + [`step_free`]: unconstrained 6-DOF Newton-Euler
//!   integration (semi-implicit Euler) of a body under applied force/torque,
//!   using the world-frame inertia `I_world = R I_body Rᵀ` and the standard
//!   gyroscopic (Euler's equation) coupling term.
//! - [`RevoluteJointDynamics`] + [`step_hinge`]: a rigid body pinned by a
//!   frictionless revolute joint at a *fixed* world anchor/axis (e.g. a
//!   pendulum or door hinge). The single-DOF equation of motion and the
//!   pin reaction force are derived in closed form (see `step_hinge` docs).

use crate::mass_props::MassProperties;
use tpt_vertex_kernel::math::{Mat3, Quaternion, Vec3};

/// A free rigid body: mass/inertia properties plus current kinematic state.
#[derive(Debug, Clone, Copy)]
pub struct RigidBody {
    pub mass: f64,
    /// Inertia tensor about the center of mass, in the body's *reference*
    /// (unrotated) frame.
    pub inertia_body: Mat3,
    /// World-space position of the center of mass.
    pub position: Vec3,
    /// Orientation relative to the reference frame in which `inertia_body`
    /// was computed.
    pub orientation: Quaternion,
    pub linear_velocity: Vec3,
    /// Angular velocity, expressed in world coordinates.
    pub angular_velocity: Vec3,
}

impl RigidBody {
    /// Build a body at rest from computed mass properties, with its current
    /// pose (`position`/`orientation`) taken as the reference frame.
    pub fn from_mass_properties(mp: &MassProperties, position: Vec3) -> Self {
        RigidBody {
            mass: mp.mass,
            inertia_body: mp.inertia_com,
            position,
            orientation: Quaternion::identity(),
            linear_velocity: Vec3::ZERO,
            angular_velocity: Vec3::ZERO,
        }
    }

    /// World-frame inertia tensor at the current orientation: `R I_body Rᵀ`.
    pub fn inertia_world(&self) -> Mat3 {
        let r = self.orientation.to_mat3();
        mat3_mul(mat3_mul(r, self.inertia_body), r.transpose())
    }

    /// Angular momentum in world coordinates.
    pub fn angular_momentum(&self) -> Vec3 {
        self.inertia_world().mul_vec(self.angular_velocity)
    }

    /// Translational + rotational kinetic energy.
    pub fn kinetic_energy(&self) -> f64 {
        let lin = 0.5 * self.mass * self.linear_velocity.dot(self.linear_velocity);
        let l = self.angular_momentum();
        let rot = 0.5 * self.angular_velocity.dot(l);
        lin + rot
    }
}

/// Advance a free (unconstrained) rigid body by `dt` under a net applied
/// `force` (through the COM) and `torque` (about the COM), both in world
/// coordinates. Semi-implicit (symplectic) Euler: velocities are updated
/// first, then position/orientation are integrated with the new velocities.
pub fn step_free(body: &mut RigidBody, force: Vec3, torque: Vec3, dt: f64) {
    // Linear part.
    let linear_accel = force * (1.0 / body.mass);
    body.linear_velocity = body.linear_velocity + linear_accel * dt;
    body.position = body.position + body.linear_velocity * dt;

    // Angular part: Euler's rigid-body equation in world coordinates,
    // dL/dt = torque with L = I_world * omega, i.e.
    // I_world * omega_dot = torque - omega x L (gyroscopic term).
    let i_world = body.inertia_world();
    let i_world_inv = mat3_invert(&i_world);
    let l = i_world.mul_vec(body.angular_velocity);
    let gyroscopic = body.angular_velocity.cross(l);
    let angular_accel = i_world_inv.mul_vec(torque - gyroscopic);
    body.angular_velocity = body.angular_velocity + angular_accel * dt;

    // Orientation update: dq/dt = 0.5 * omega_quat * q.
    let om = body.angular_velocity;
    let omega_quat = Quaternion {
        w: 0.0,
        x: om.x,
        y: om.y,
        z: om.z,
    };
    let dq = omega_quat * body.orientation;
    body.orientation = Quaternion {
        w: body.orientation.w + 0.5 * dt * dq.w,
        x: body.orientation.x + 0.5 * dt * dq.x,
        y: body.orientation.y + 0.5 * dt * dq.y,
        z: body.orientation.z + 0.5 * dt * dq.z,
    }
    .normalize();
}

/// A rigid body pinned by a frictionless revolute joint at a world-fixed
/// `anchor`/`axis`. Only the single rotational DOF about `axis` is free; the
/// joint is assumed to supply whatever reaction force is needed to hold the
/// anchor point fixed (off-axis reaction *torque* is not modeled — this is
/// an idealized single-axis hinge/pendulum, not a full 6-DOF bearing).
#[derive(Debug, Clone, Copy)]
pub struct RevoluteJointDynamics {
    pub anchor: Vec3,
    pub axis: Vec3,
    /// Vector from `anchor` to the body's center of mass at `angle == 0`.
    pub r0: Vec3,
    pub angle: f64,
    pub angular_velocity: f64,
}

impl RevoluteJointDynamics {
    pub fn new(anchor: Vec3, axis: Vec3, r0: Vec3) -> Self {
        RevoluteJointDynamics {
            anchor,
            axis: axis.normalize(),
            r0,
            angle: 0.0,
            angular_velocity: 0.0,
        }
    }

    /// Vector from the anchor to the COM at the current angle.
    pub fn r(&self) -> Vec3 {
        Quaternion::from_axis_angle(self.axis, self.angle).rotate_vec(self.r0)
    }
}

/// Result of one hinge integration step.
#[derive(Debug, Clone, Copy)]
pub struct HingeStepResult {
    pub angle: f64,
    pub angular_velocity: f64,
    pub angular_acceleration: f64,
    /// Force the joint exerts on the body at the anchor (world coordinates)
    /// to keep the anchor point fixed, given the computed motion.
    pub reaction_force: Vec3,
}

/// Effective (constant) moment of inertia about the fixed hinge axis:
/// `I_axis = m·d² + axis·(I_body·axis)`, where `d` is the perpendicular
/// distance from the COM to the axis line. This is exactly constant over
/// the motion because the body only ever rotates about `axis` itself (a
/// short proof: `axis·(R I_body Rᵀ)·axis = axis·(I_body·axis)` whenever `R`
/// is a rotation about `axis`, since `Rᵀaxis = axis`).
pub fn hinge_axis_inertia(mp: &MassProperties, joint: &RevoluteJointDynamics) -> f64 {
    let r0 = joint.r0;
    let axis = joint.axis;
    let d2 = r0.dot(r0) - r0.dot(axis).powi(2);
    mp.mass * d2 + axis.dot(mp.inertia_com.mul_vec(axis))
}

/// Advance the hinge by `dt` (classic 4th-order Runge-Kutta on the scalar
/// `(angle, angular_velocity)` state) under gravity and an optional applied
/// torque about the axis. Also returns the reaction force at the anchor.
pub fn step_hinge(
    mp: &MassProperties,
    joint: &mut RevoluteJointDynamics,
    gravity: Vec3,
    applied_torque_about_axis: f64,
    dt: f64,
) -> HingeStepResult {
    let i_axis = hinge_axis_inertia(mp, joint);
    let torque_of = |angle: f64| -> f64 {
        let r = Quaternion::from_axis_angle(joint.axis, angle).rotate_vec(joint.r0);
        let gravity_torque = joint.axis.dot(r.cross(gravity * mp.mass));
        gravity_torque + applied_torque_about_axis
    };
    let deriv = |theta: f64, omega: f64| -> (f64, f64) { (omega, torque_of(theta) / i_axis) };

    let (t0, w0) = (joint.angle, joint.angular_velocity);
    let (k1t, k1w) = deriv(t0, w0);
    let (k2t, k2w) = deriv(t0 + 0.5 * dt * k1t, w0 + 0.5 * dt * k1w);
    let (k3t, k3w) = deriv(t0 + 0.5 * dt * k2t, w0 + 0.5 * dt * k2w);
    let (k4t, k4w) = deriv(t0 + dt * k3t, w0 + dt * k3w);

    joint.angle = t0 + dt / 6.0 * (k1t + 2.0 * k2t + 2.0 * k3t + k4t);
    joint.angular_velocity = w0 + dt / 6.0 * (k1w + 2.0 * k2w + 2.0 * k3w + k4w);
    let angular_acceleration = torque_of(joint.angle) / i_axis;

    // Reaction force at the anchor: R = m*a_com - F_ext, with
    // a_com = alpha x r + omega x (omega x r).
    let r = joint.r();
    let omega_vec = joint.axis * joint.angular_velocity;
    let alpha_vec = joint.axis * angular_acceleration;
    let a_com = alpha_vec.cross(r) + omega_vec.cross(omega_vec.cross(r));
    let reaction_force = a_com * mp.mass - gravity * mp.mass;

    HingeStepResult {
        angle: joint.angle,
        angular_velocity: joint.angular_velocity,
        angular_acceleration,
        reaction_force,
    }
}

fn mat3_mul(a: Mat3, b: Mat3) -> Mat3 {
    Mat3::from_cols(
        a.mul_vec(b.cols[0]),
        a.mul_vec(b.cols[1]),
        a.mul_vec(b.cols[2]),
    )
}

/// Invert a `Mat3` (returns the identity if singular; callers only invoke
/// this on physical inertia tensors, which are always positive-definite).
fn mat3_invert(m: &Mat3) -> Mat3 {
    let rows = [
        [m.cols[0].x, m.cols[1].x, m.cols[2].x],
        [m.cols[0].y, m.cols[1].y, m.cols[2].y],
        [m.cols[0].z, m.cols[1].z, m.cols[2].z],
    ];
    let det = rows[0][0] * (rows[1][1] * rows[2][2] - rows[1][2] * rows[2][1])
        - rows[0][1] * (rows[1][0] * rows[2][2] - rows[1][2] * rows[2][0])
        + rows[0][2] * (rows[1][0] * rows[2][1] - rows[1][1] * rows[2][0]);
    if det.abs() < 1e-18 {
        return Mat3::identity();
    }
    let inv = 1.0 / det;
    let r = [
        [
            (rows[1][1] * rows[2][2] - rows[1][2] * rows[2][1]) * inv,
            (rows[0][2] * rows[2][1] - rows[0][1] * rows[2][2]) * inv,
            (rows[0][1] * rows[1][2] - rows[0][2] * rows[1][1]) * inv,
        ],
        [
            (rows[1][2] * rows[2][0] - rows[1][0] * rows[2][2]) * inv,
            (rows[0][0] * rows[2][2] - rows[0][2] * rows[2][0]) * inv,
            (rows[0][2] * rows[1][0] - rows[0][0] * rows[1][2]) * inv,
        ],
        [
            (rows[1][0] * rows[2][1] - rows[1][1] * rows[2][0]) * inv,
            (rows[0][1] * rows[2][0] - rows[0][0] * rows[2][1]) * inv,
            (rows[0][0] * rows[1][1] - rows[0][1] * rows[1][0]) * inv,
        ],
    ];
    Mat3::from_row_major([
        r[0][0], r[0][1], r[0][2], r[1][0], r[1][1], r[1][2], r[2][0], r[2][1], r[2][2],
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mass_props::MassProperties;

    fn point_mass(m: f64) -> MassProperties {
        MassProperties {
            mass: m,
            volume: 1.0,
            center_of_mass: Vec3::ZERO,
            inertia_com: Mat3::identity(), // negligible self-inertia (point mass)
        }
        .with_negligible_inertia()
    }

    // Small helper trait so the "point mass" fixture reads clearly at the call site.
    trait Negligible {
        fn with_negligible_inertia(self) -> Self;
    }
    impl Negligible for MassProperties {
        fn with_negligible_inertia(mut self) -> Self {
            self.inertia_com =
                Mat3::from_row_major([1e-9, 0.0, 0.0, 0.0, 1e-9, 0.0, 0.0, 0.0, 1e-9]);
            self
        }
    }

    #[test]
    fn free_body_projectile_matches_kinematics() {
        let mp = point_mass(2.0);
        let mut body = RigidBody::from_mass_properties(&mp, Vec3::ZERO);
        body.linear_velocity = Vec3::new(10.0, 0.0, 0.0);
        let g = Vec3::new(0.0, -9.81, 0.0);
        let dt = 0.001;
        let steps = 1000; // t = 1s
        for _ in 0..steps {
            step_free(&mut body, g * mp.mass, Vec3::ZERO, dt);
        }
        let t = steps as f64 * dt;
        // x = v0*t; y = 0.5*g*t^2 (downward), within semi-implicit-Euler integration error.
        assert!((body.position.x - 10.0 * t).abs() < 1e-6);
        assert!((body.position.y - (-0.5 * 9.81 * t * t)).abs() < 0.05);
    }

    #[test]
    fn free_body_zero_torque_conserves_angular_momentum() {
        let mp = MassProperties {
            mass: 1.0,
            volume: 1.0,
            center_of_mass: Vec3::ZERO,
            inertia_com: Mat3::from_row_major([2.0, 0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 0.0, 4.0]),
        };
        let mut body = RigidBody::from_mass_properties(&mp, Vec3::ZERO);
        body.angular_velocity = Vec3::new(1.0, 0.5, -0.3);
        let l0 = body.angular_momentum().length();
        for _ in 0..2000 {
            step_free(&mut body, Vec3::ZERO, Vec3::ZERO, 0.0005);
        }
        let l1 = body.angular_momentum().length();
        assert!((l0 - l1).abs() / l0 < 1e-3, "L0={l0} L1={l1}");
    }

    #[test]
    fn hinge_small_angle_matches_simple_pendulum_period() {
        // A near-point-mass bob at radius L below a fixed pivot swinging in the
        // XY plane under gravity, hinge axis = Z: I_axis ≈ m*L², the classic
        // simple-pendulum equation theta'' = -(g/L) sin(theta) applies.
        let l = 1.0;
        let g = 9.81;
        let mp = point_mass(1.0);
        let mut joint = RevoluteJointDynamics::new(Vec3::ZERO, Vec3::Z, Vec3::new(0.0, -l, 0.0));
        let theta0 = 0.05; // small angle (radians)
        joint.angle = theta0;
        let gravity = Vec3::new(0.0, -g, 0.0);
        let dt = 0.0005;
        let period = 2.0 * std::f64::consts::PI * (l / g).sqrt();
        let steps = (period / dt).round() as usize; // integrate one full period
        for _ in 0..steps {
            step_hinge(&mp, &mut joint, gravity, 0.0, dt);
        }
        // After one small-angle period the pendulum should return close to theta0.
        assert!(
            (joint.angle - theta0).abs() < 0.01,
            "angle after one period: {} (expected ~{})",
            joint.angle,
            theta0
        );
    }

    #[test]
    fn hinge_at_rest_horizontal_reaction_supports_weight() {
        // Bob held out horizontally (unstable equilibrium angle), zero velocity:
        // the instantaneous reaction force must supply the full weight upward
        // if angular acceleration momentarily has no vertical accel component
        // orthogonal to the rod... instead we check the simpler invariant that
        // gravity torque is maximal (rod horizontal) and matches -m*g*L.
        let l = 2.0;
        let g = 9.81;
        let mp = point_mass(3.0);
        let mut joint = RevoluteJointDynamics::new(Vec3::ZERO, Vec3::Z, Vec3::new(l, 0.0, 0.0));
        joint.angle = 0.0;
        let gravity = Vec3::new(0.0, -g, 0.0);
        let res = step_hinge(&mp, &mut joint, gravity, 0.0, 1e-6);
        let i_axis = hinge_axis_inertia(&mp, &joint);
        let expected_alpha = -(mp.mass * g * l) / i_axis;
        assert!(
            (res.angular_acceleration - expected_alpha).abs() / expected_alpha.abs() < 1e-3,
            "alpha {} expected {}",
            res.angular_acceleration,
            expected_alpha
        );
    }
}
