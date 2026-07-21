//! Adaptive layer height: pick a Z step per band of the model height based on
//! local surface slope, so shallow/curved surfaces get finer resolution and
//! near-vertical walls (where extra resolution buys nothing) get coarser,
//! faster layers.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! For a facet whose normal makes angle `theta` with the vertical (build)
//! axis — `theta = 0` for a horizontal facet, `theta = 90°` for a vertical
//! wall — printing it in layers of height `h` leaves a vertical "cusp" (the
//! worst-case deviation between the printed staircase and the ideal surface)
//! of approximately `h * cos(theta)`: zero for a vertical wall (each layer
//! lands exactly on the one below, no stair-stepping) and up to the full
//! layer height for a horizontal facet. Solving for `h` given a target cusp
//! bound yields `h = target_cusp / cos(theta)`, clamped to the configured
//! min/max layer height.

use tpt_vertex_kernel::geometry::solid::Solid;

/// Tunables for adaptive layer-height slicing.
#[derive(Debug, Clone, PartialEq)]
pub struct AdaptiveLayerSettings {
    /// Smallest layer height the slicer will use, in millimetres.
    pub min_layer_height: f64,
    /// Largest layer height the slicer will use, in millimetres (used on
    /// near-vertical walls where slope imposes no finer bound).
    pub max_layer_height: f64,
    /// Maximum allowed vertical cusp/scallop error, in millimetres, on a
    /// surface angled away from vertical.
    pub target_cusp: f64,
    /// Sampling resolution along Z used to build the local slope profile.
    pub sample_height: f64,
}

impl Default for AdaptiveLayerSettings {
    fn default() -> Self {
        AdaptiveLayerSettings {
            min_layer_height: 0.1,
            max_layer_height: 0.3,
            target_cusp: 0.02,
            sample_height: 0.05,
        }
    }
}

/// Maximum layer height that keeps the cusp/scallop error under
/// `settings.target_cusp` for a facet whose normal makes angle `theta`
/// (radians) with the vertical build axis.
fn max_height_for_angle(theta: f64, settings: &AdaptiveLayerSettings) -> f64 {
    let c = theta.cos();
    let h = if c < 1e-6 {
        settings.max_layer_height
    } else {
        settings.target_cusp / c
    };
    h.clamp(settings.min_layer_height, settings.max_layer_height)
}

/// Build a per-Z-sample profile of the maximum layer height allowed by local
/// surface slope, covering `[z_min, z_max]` at `settings.sample_height`
/// resolution. Bands with no covering facet (shouldn't occur for a closed
/// solid) default to `max_layer_height`.
fn slope_profile(solid: &Solid, z_min: f64, z_max: f64, settings: &AdaptiveLayerSettings) -> Vec<f64> {
    let step = settings.sample_height.max(1e-3);
    let n = (((z_max - z_min) / step).ceil() as usize) + 1;
    let mut profile = vec![settings.max_layer_height; n.max(1)];

    for f in &solid.faces {
        let a = solid.vertices[f.a as usize];
        let b = solid.vertices[f.b as usize];
        let c = solid.vertices[f.c as usize];
        let normal = (b - a).cross(c - a);
        let len = normal.length();
        if len < 1e-12 {
            continue;
        }
        let nz = (normal.z / len).clamp(-1.0, 1.0).abs();
        let theta = nz.acos(); // angle from the vertical (Z) axis
        let allowed = max_height_for_angle(theta, settings);

        let fz_min = a.z.min(b.z).min(c.z);
        let fz_max = a.z.max(b.z).max(c.z);
        if fz_max < z_min || fz_min > z_max {
            continue;
        }
        let i0 = (((fz_min - z_min) / step).floor().max(0.0)) as usize;
        let i1 = (((fz_max - z_min) / step).ceil() as usize).min(profile.len() - 1);
        for slot in profile.iter_mut().take(i1 + 1).skip(i0) {
            if allowed < *slot {
                *slot = allowed;
            }
        }
    }
    profile
}

/// Compute a sequence of layer top-of-layer Z heights from `z_min` to
/// `z_max`, starting with `first_layer_height`, then stepping by whatever
/// height the local slope profile allows (clamped to
/// `[min_layer_height, max_layer_height]`) at each subsequent layer.
pub fn adaptive_layer_zs(
    solid: &Solid,
    z_min: f64,
    z_max: f64,
    first_layer_height: f64,
    settings: &AdaptiveLayerSettings,
) -> Vec<f64> {
    let mut zs = Vec::new();
    if z_max <= z_min {
        return zs;
    }
    let profile = slope_profile(solid, z_min, z_max, settings);
    let step = settings.sample_height.max(1e-3);
    let lookup = |z: f64| -> f64 {
        let idx = (((z - z_min) / step).round() as isize)
            .clamp(0, profile.len() as isize - 1) as usize;
        profile[idx].clamp(settings.min_layer_height, settings.max_layer_height)
    };

    let mut z = z_min + first_layer_height;
    while z <= z_max + 1e-9 {
        zs.push(z);
        z += lookup(z);
    }
    // Make sure the model's top is actually covered even if the last
    // computed step overshot past z_max by more than rounding error.
    if let Some(&last) = zs.last() {
        if last < z_max - 1e-6 {
            zs.push(z_max);
        }
    }
    zs
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_vertex_kernel::geometry::solid::{Face, Solid as KernSolid};
    use tpt_vertex_kernel::math::Vec3;

    fn cube(half: f64, height: f64) -> KernSolid {
        let mut s = KernSolid::new();
        let (x0, y0, z0) = (-half, -half, 0.0);
        let (x1, y1, z1) = (half, half, height);
        let mut v = |x: f64, y: f64, z: f64| s.add_vertex(Vec3::new(x, y, z));
        let p = [
            v(x0, y0, z0), v(x1, y0, z0), v(x1, y1, z0), v(x0, y1, z0),
            v(x0, y0, z1), v(x1, y0, z1), v(x1, y1, z1), v(x0, y1, z1),
        ];
        let mut f = |a: u32, b: u32, c: u32| s.faces.push(Face::new(a, b, c));
        f(p[0], p[1], p[2]); f(p[0], p[2], p[3]);
        f(p[4], p[6], p[5]); f(p[4], p[7], p[6]);
        f(p[0], p[5], p[1]); f(p[0], p[4], p[5]);
        f(p[1], p[6], p[2]); f(p[1], p[5], p[6]);
        f(p[2], p[7], p[3]); f(p[2], p[6], p[7]);
        f(p[3], p[4], p[0]); f(p[3], p[7], p[4]);
        s
    }

    /// A cone: only sloped side facets, no vertical walls. Every layer should
    /// be forced down toward the minimum height by the slope bound.
    fn cone(base_r: f64, height: f64, segments: usize) -> KernSolid {
        let mut s = KernSolid::new();
        let apex = s.add_vertex(Vec3::new(0.0, 0.0, height));
        let base_center = s.add_vertex(Vec3::new(0.0, 0.0, 0.0));
        let mut ring = Vec::with_capacity(segments);
        for i in 0..segments {
            let a = (i as f64) / (segments as f64) * std::f64::consts::TAU;
            ring.push(s.add_vertex(Vec3::new(base_r * a.cos(), base_r * a.sin(), 0.0)));
        }
        for i in 0..segments {
            let j = (i + 1) % segments;
            s.faces.push(Face::new(ring[i], ring[j], apex));
            s.faces.push(Face::new(base_center, ring[j], ring[i]));
        }
        s
    }

    #[test]
    fn vertical_wall_uses_max_layer_height() {
        let s = cube(2.0, 4.0);
        let settings = AdaptiveLayerSettings::default();
        let zs = adaptive_layer_zs(&s, 0.0, 4.0, settings.max_layer_height, &settings);
        // A pure vertical-wall box should step at (close to) max_layer_height
        // throughout, i.e. roughly height / max_layer_height layers.
        let expected = (4.0 / settings.max_layer_height).ceil() as usize;
        assert!(
            zs.len() <= expected + 1,
            "expected roughly {} layers for an all-vertical box, got {}",
            expected,
            zs.len()
        );
    }

    #[test]
    fn sloped_cone_uses_finer_layers_than_max() {
        let s = cone(5.0, 4.0, 24);
        let settings = AdaptiveLayerSettings::default();
        let zs = adaptive_layer_zs(&s, 0.0, 4.0, settings.min_layer_height, &settings);
        // A cone's side is sloped everywhere; it should need meaningfully
        // more layers than a coarse fixed max-height slice would.
        let coarse = (4.0 / settings.max_layer_height).ceil() as usize;
        assert!(
            zs.len() > coarse,
            "expected adaptive slicing of a cone to use more, finer layers ({} vs coarse {})",
            zs.len(),
            coarse
        );
    }

    #[test]
    fn zs_are_monotonic_and_bounded() {
        let s = cone(5.0, 4.0, 16);
        let settings = AdaptiveLayerSettings::default();
        let zs = adaptive_layer_zs(&s, 0.0, 4.0, 0.2, &settings);
        assert!(!zs.is_empty());
        for w in zs.windows(2) {
            assert!(w[1] > w[0]);
        }
        assert!(*zs.last().unwrap() <= 4.0 + 1e-6);
    }
}
