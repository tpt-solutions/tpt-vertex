// SPDX-License-Identifier: MIT OR Apache-2.0

//! Fatigue / lifetime analysis: S-N curves, Goodman mean-stress correction,
//! and Miner's cumulative-damage rule.
//!
//! Given a cyclic stress history (amplitude and mean per cycle, or a full
//! time-series reduced to rainflow-counted ranges), this module estimates
//! the number of cycles to failure via an S-N (Wöhler) curve and accumulates
//! damage with Palmgren-Miner linear damage rule: `D = Σ nᵢ / Nᵢ`.
//! Failure is predicted when `D ≥ 1.0`.

use tpt_vertex_kernel::material::Material;

/// S-N curve parameters: `N = C / (Δσ)^m` where `N` is cycles to failure at
/// stress range `Δσ`, `C` is the fatigue strength coefficient, and `m` is the
/// fatigue strength exponent (Basquin's law). Below the endurance limit the
/// curve is flat (infinite life).
#[derive(Debug, Clone, Copy)]
pub struct SnCurve {
    /// Basquin exponent (slope of log-log S-N curve). Typical: 3–5 for metals.
    pub m: f64,
    /// Fatigue strength coefficient (intercept at N=1 cycle). MPa.
    pub c: f64,
    /// Endurance limit (stress range below which life is infinite). MPa.
    /// Set to `0.0` for materials with no endurance limit (e.g. some polymers).
    pub endurance_limit: f64,
}

impl SnCurve {
    /// Cycles to failure at stress range `delta_sigma` (MPa).
    /// Returns `f64::INFINITY` if below the endurance limit.
    pub fn life(&self, delta_sigma: f64) -> f64 {
        if delta_sigma <= 0.0 {
            return f64::INFINITY;
        }
        if self.endurance_limit > 0.0 && delta_sigma <= self.endurance_limit {
            return f64::INFINITY;
        }
        self.c / delta_sigma.powf(self.m)
    }

    /// Stress range (MPa) for a given life `n` cycles.
    pub fn stress_range(&self, n: f64) -> f64 {
        if !n.is_finite() || n <= 0.0 {
            return self.endurance_limit;
        }
        (self.c / n).powf(1.0 / self.m)
    }
}

/// Goodman mean-stress correction: converts a (stress_amplitude, mean_stress)
/// pair to an equivalent fully-reversed stress amplitude (`σ_ar`).
///
/// `σ_ar = σ_a / (1 - σ_m / σ_u)`, where `σ_u` is the ultimate tensile
/// strength. Clamped to return at least `σ_a` (tensile mean stress is
/// damaging; compressive is beneficial but we don't reduce below zero).
pub fn goodman_correct(amplitude: f64, mean: f64, uts: f64) -> f64 {
    if uts <= 0.0 {
        return amplitude;
    }
    let denom = 1.0 - mean / uts;
    if denom.abs() < 1e-12 {
        return f64::INFINITY;
    }
    (amplitude / denom).max(amplitude)
}

/// Gerber parabolic mean-stress correction (less conservative than Goodman
/// for ductile metals): `σ_ar = σ_a / (1 - (σ_m / σ_u)²)`.
pub fn gerber_correct(amplitude: f64, mean: f64, uts: f64) -> f64 {
    if uts <= 0.0 {
        return amplitude;
    }
    let ratio = mean / uts;
    let denom = 1.0 - ratio * ratio;
    if denom < 1e-12 {
        return f64::INFINITY;
    }
    (amplitude / denom).max(amplitude)
}

/// A single load cycle identified from a stress history.
#[derive(Debug, Clone, Copy)]
pub struct LoadCycle {
    /// Stress range `Δσ` (peak-to-peak, always positive). MPa.
    pub range: f64,
    /// Mean stress of the cycle. MPa.
    pub mean: f64,
    /// Number of repetitions of this cycle.
    pub count: f64,
}

/// Miner's cumulative damage for a sequence of load cycles.
///
/// Returns `(damage, cycles_to_failure)` where `damage` is the accumulated
/// Miner sum `D = Σ nᵢ/Nᵢ` and `cycles_to_failure` is the total block life
/// (1/D if D > 0, else infinite).
pub fn miner_damage(cycles: &[LoadCycle], sn: &SnCurve, uts: f64) -> (f64, f64) {
    let mut damage = 0.0;
    for c in cycles {
        let sigma_ar = goodman_correct(c.range / 2.0, c.mean, uts);
        let delta_sigma = 2.0 * sigma_ar; // convert amplitude back to range for S-N
        let n = sn.life(delta_sigma);
        if n.is_finite() && n > 0.0 {
            damage += c.count / n;
        }
    }
    let life = if damage > 0.0 { 1.0 / damage } else { f64::INFINITY };
    (damage, life)
}

/// Build an S-N curve from a kernel [`Material`] using simplified empirical
/// correlations. This is a v1 approximation; a material library with measured
/// fatigue data is a documented fast-follow.
pub fn sn_from_material(mat: &Material) -> SnCurve {
    // Approximate endurance limit as fraction of yield strength.
    // Metals: ~0.5 × S_ut for steel (no endurance limit below ~10⁷),
    // ~0.3–0.4 × S_ut for aluminum. Polymers: no true endurance limit.
    let uts = mat.yield_strength; // v1: use yield as proxy for UTS
    let (m, endurance_fraction) = if mat.youngs_modulus > 50_000.0 {
        // Metal-like
        (3.5, 0.4)
    } else {
        // Polymer-like: steeper slope, no endurance limit
        (5.0, 0.0)
    };
    let endurance = uts * endurance_fraction * 2.0; // ×2 because S-N uses range
    let c = uts.powf(m) * 1.0; // normalized: at σ_range = UTS, N ≈ 1
    SnCurve { m, c, endurance_limit: endurance }
}

/// Result of a fatigue assessment on a mesh.
#[derive(Debug, Clone)]
pub struct FatigueResult {
    /// Per-element damage index (0.0 = no damage, ≥1.0 = predicted failure).
    pub damage: Vec<f64>,
    /// Overall Miner's sum (maximum across elements).
    pub max_damage: f64,
    /// Predicted block life (1/D_max).
    pub block_life: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sn_curve_infinite_life_below_endurance() {
        let sn = SnCurve { m: 3.0, c: 1e12, endurance_limit: 500.0 };
        assert!(sn.life(400.0).is_infinite());
        assert!(sn.life(600.0).is_finite());
    }

    #[test]
    fn sn_curve_decreasing_life_with_increasing_stress() {
        let sn = SnCurve { m: 3.0, c: 1e12, endurance_limit: 0.0 };
        let n1 = sn.life(500.0);
        let n2 = sn.life(1000.0);
        assert!(n1 > n2);
    }

    #[test]
    fn goodman_tensile_mean_reduces_life() {
        let uts = 500.0;
        let ar_zero = goodman_correct(200.0, 0.0, uts);
        let ar_tensile = goodman_correct(200.0, 100.0, uts);
        assert!(ar_tensile > ar_zero);
    }

    #[test]
    fn goodman_compressive_mean_does_not_reduce() {
        let uts = 500.0;
        let ar_zero = goodman_correct(200.0, 0.0, uts);
        let ar_comp = goodman_correct(200.0, -100.0, uts);
        assert!((ar_comp - ar_zero).abs() < 1e-9);
    }

    #[test]
    fn miner_zero_damage_infinite_life() {
        let sn = SnCurve { m: 3.0, c: 1e12, endurance_limit: 500.0 };
        // All cycles below endurance limit.
        let cycles = vec![LoadCycle { range: 400.0, mean: 0.0, count: 1000.0 }];
        let (d, life) = miner_damage(&cycles, &sn, 500.0);
        assert!(d == 0.0);
        assert!(life.is_infinite());
    }

    #[test]
    fn miner_accumulates_damage() {
        let sn = SnCurve { m: 3.0, c: 1e12, endurance_limit: 0.0 };
        let cycles = vec![
            LoadCycle { range: 1000.0, mean: 0.0, count: 100.0 },
            LoadCycle { range: 1000.0, mean: 0.0, count: 100.0 },
        ];
        let (d, _) = miner_damage(&cycles, &sn, 500.0);
        // Each cycle: N = 1e12 / 1e9 = 1000. Two sets of 100: D = 100/1000 + 100/1000 = 0.2.
        assert!((d - 0.2).abs() < 1e-6);
    }
}
