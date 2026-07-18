//! Camera system: orbit, pan, zoom, perspective/orthographic toggle.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0

use glam::{Mat4, Vec3};

/// Projection mode for the camera.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Projection {
    Perspective,
    Orthographic,
}

/// An orbit camera orbiting a target point.
#[derive(Debug, Clone)]
pub struct Camera {
    /// World-space point the camera looks at.
    pub target: Vec3,
    /// Distance from target.
    pub distance: f32,
    /// Yaw (radians) around the world up axis.
    pub yaw: f32,
    /// Pitch (radians) above/below the horizon, clamped to (-π/2, π/2).
    pub pitch: f32,
    pub projection: Projection,
    pub fov_y: f32,
    pub near: f32,
    pub far: f32,
    /// Viewport aspect ratio (width / height).
    pub aspect: f32,
    /// Half-height of the view frustum in world units (orthographic).
    pub ortho_half_height: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Camera {
            target: Vec3::ZERO,
            distance: 10.0,
            yaw: std::f32::consts::FRAC_PI_4,
            pitch: std::f32::consts::FRAC_PI_6,
            projection: Projection::Perspective,
            fov_y: 60.0_f32.to_radians(),
            near: 0.01,
            far: 1000.0,
            aspect: 1.0,
            ortho_half_height: 5.0,
        }
    }
}

impl Camera {
    /// Eye position derived from target + spherical coordinates.
    pub fn eye(&self) -> Vec3 {
        let cp = self.pitch.cos();
        let dir = Vec3::new(
            cp * self.yaw.sin(),
            self.pitch.sin(),
            cp * self.yaw.cos(),
        );
        self.target + dir * self.distance
    }

    pub fn forward(&self) -> Vec3 {
        (self.target - self.eye()).normalize()
    }

    pub fn right(&self) -> Vec3 {
        self.forward().cross(Vec3::Y).normalize()
    }

    pub fn up(&self) -> Vec3 {
        self.right().cross(self.forward()).normalize()
    }

    /// View matrix.
    pub fn view_matrix(&self) -> Mat4 {
        Mat4::look_at_rh(self.eye(), self.target, Vec3::Y)
    }

    /// Projection matrix for the current mode.
    pub fn projection_matrix(&self) -> Mat4 {
        match self.projection {
            Projection::Perspective => Mat4::perspective_rh(
                self.fov_y,
                self.aspect.max(1e-4),
                self.near,
                self.far,
            ),
            Projection::Orthographic => {
                let h = self.ortho_half_height;
                let w = h * self.aspect;
                Mat4::orthographic_rh(-w, w, -h, h, self.near, self.far)
            }
        }
    }

    /// Combined view-projection matrix.
    pub fn view_proj(&self) -> Mat4 {
        self.projection_matrix() * self.view_matrix()
    }

    /// Orbit horizontally (dyaw) and vertically (dpitch).
    pub fn orbit(&mut self, dyaw: f32, dpitch: f32) {
        self.yaw += dyaw;
        self.pitch = (self.pitch + dpitch).clamp(
            -std::f32::consts::FRAC_PI_2 + 1e-3,
            std::f32::consts::FRAC_PI_2 - 1e-3,
        );
    }

    /// Dolly zoom: scale distance (perspective) or ortho frustum (orthographic).
    pub fn zoom(&mut self, factor: f32) {
        let factor = factor.clamp(0.1, 10.0);
        self.distance = (self.distance * factor).clamp(0.05, 1e6);
        if self.projection == Projection::Orthographic {
            self.ortho_half_height = (self.ortho_half_height * factor).clamp(1e-3, 1e6);
        }
    }

    /// Pan the target in the camera's screen plane by (dx, dy) world units.
    pub fn pan(&mut self, dx: f32, dy: f32) {
        let right = self.right();
        let up = self.up();
        self.target += right * dx + up * dy;
    }

    pub fn set_aspect(&mut self, aspect: f32) {
        self.aspect = aspect;
    }

    pub fn toggle_projection(&mut self) {
        self.projection = match self.projection {
            Projection::Perspective => Projection::Orthographic,
            Projection::Orthographic => Projection::Perspective,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eye_is_distance_from_target() {
        let cam = Camera::default();
        assert!((cam.eye().distance(cam.target) - cam.distance).abs() < 1e-4);
    }

    #[test]
    fn zoom_scales_distance() {
        let mut cam = Camera::default();
        let d0 = cam.distance;
        cam.zoom(0.5);
        assert!((cam.distance - d0 * 0.5).abs() < 1e-4);
    }

    #[test]
    fn pitch_clamped() {
        let mut cam = Camera::default();
        cam.orbit(0.0, 10.0);
        assert!(cam.pitch <= std::f32::consts::FRAC_PI_2);
    }
}
