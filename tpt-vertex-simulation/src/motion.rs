//! Assembly motion: time/parameter-driven playback over kernel mates.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! A [`MotionPlayer`] drives the DOF-bearing joint (`Revolute`/`Slider`/
//! `Cylindrical`) of an [`Assembly`] through a parameter range and re-solves
//! the mate graph each frame, producing the forward-kinematics pose of every
//! part. This is pure geometry (no dynamics): the drive parameter is scrubbed
//! linearly from `start` to `end`.

use tpt_vertex_kernel::assembly::Assembly;
use tpt_vertex_kernel::math::{Quaternion, Vec3};

/// Drives one joint of an assembly over a parameter range.
#[derive(Debug, Clone)]
pub struct MotionPlayer {
    assembly: Assembly,
    part: tpt_vertex_kernel::assembly::PartId,
    start: f64,
    end: f64,
    current: f64,
}

impl MotionPlayer {
    /// Create a player for the joint whose `mover` is `part`. The drive
    /// parameter is scrubbed from `start` to `end` (radians for revolute/
    /// cylindrical angle, mm for slider/cylindrical offset).
    pub fn new(
        assembly: Assembly,
        part: tpt_vertex_kernel::assembly::PartId,
        start: f64,
        end: f64,
    ) -> Self {
        MotionPlayer {
            assembly,
            part,
            start,
            end,
            current: start,
        }
    }

    /// Current drive parameter.
    pub fn current(&self) -> f64 {
        self.current
    }

    /// Range `(start, end)`.
    pub fn range(&self) -> (f64, f64) {
        (self.start, self.end)
    }

    /// Compute the assembly pose at a normalized time `t` in `[0, 1]`, solving
    /// the mate graph. `t = 0` maps to `start`, `t = 1` to `end`.
    pub fn frame_at(&mut self, t: f64) -> &Assembly {
        let t = t.clamp(0.0, 1.0);
        self.current = self.start + t * (self.end - self.start);
        self.assembly.set_drive(self.part, self.current);
        self.assembly.solve_mates(60, 1e-9);
        &self.assembly
    }

    /// Advance by a normalized step `dt` and return the updated assembly.
    pub fn step(&mut self, dt: f64) -> &Assembly {
        let t =
            ((self.current - self.start) / (self.end - self.start + 1e-12) + dt).clamp(0.0, 1.0);
        self.frame_at(t)
    }

    /// Immutable access to the underlying assembly.
    pub fn assembly(&self) -> &Assembly {
        &self.assembly
    }
}

/// Expected rotation quaternion for driving a revolute joint by `angle` about
/// `axis` (matches the kernel's `Quaternion::from_axis_angle`). Useful for
/// validating motion playback against closed-form kinematics.
pub fn expected_rotation(axis: Vec3, angle: f64) -> Quaternion {
    Quaternion::from_axis_angle(axis, angle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_vertex_kernel::assembly::{Assembly, Mate, Part};
    use tpt_vertex_kernel::feature_tree::Feature;
    use tpt_vertex_kernel::geometry::sketch::Sketch;
    use tpt_vertex_kernel::math::Vec2;

    fn block() -> tpt_vertex_kernel::feature_tree::FeatureTree {
        let mut s = Sketch::new();
        s.line(Vec2::ZERO, Vec2::new(1.0, 0.0));
        s.line(Vec2::new(1.0, 0.0), Vec2::new(1.0, 1.0));
        s.line(Vec2::new(1.0, 1.0), Vec2::ZERO);
        let mut t = tpt_vertex_kernel::feature_tree::FeatureTree::new();
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
    fn revolute_playback_matches_quaternion() {
        let mut asm = Assembly::new();
        let base = asm.add_part(Part::new("base", block()));
        let mover = asm.add_part(Part::new("mover", block()));
        let angle = std::f64::consts::FRAC_PI_2; // 90°
        asm.add_mate(Mate::Revolute {
            base,
            mover,
            anchor: Vec3::ZERO,
            axis: Vec3::Z,
            angle,
            limits: None,
        });
        let mut player = MotionPlayer::new(asm, mover, 0.0, angle);
        let solved = player.frame_at(1.0);
        let p = solved.part(mover).unwrap();
        // The mover's rotation quaternion should match a +90° rotation about Z.
        let expected = expected_rotation(Vec3::Z, angle);
        let q = p.transform.rotation;
        assert!(
            (q.w - expected.w).abs() < 1e-9,
            "w {} vs {}",
            q.w,
            expected.w
        );
        assert!((q.x - expected.x).abs() < 1e-9);
        assert!((q.y - expected.y).abs() < 1e-9);
        assert!((q.z - expected.z).abs() < 1e-9);
    }

    #[test]
    fn slider_playback_translates() {
        let mut asm = Assembly::new();
        let base = asm.add_part(Part::new("base", block()));
        let mover = asm.add_part(Part::new("mover", block()));
        let off = 5.0;
        asm.add_mate(Mate::Slider {
            base,
            mover,
            axis: Vec3::X,
            offset: off,
            limits: None,
        });
        let mut player = MotionPlayer::new(asm, mover, 0.0, off);
        let solved = player.frame_at(1.0);
        let p = solved.part(mover).unwrap();
        assert!(
            (p.transform.translation.x - off).abs() < 1e-6,
            "x {}",
            p.transform.translation.x
        );
    }
}
