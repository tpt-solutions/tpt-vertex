//! Engineering material properties attached to parts.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! A [`Material`] carries the physical properties used across the platform:
//! density for mass/BOM, and elastic constants (Young's modulus, Poisson's
//! ratio) plus yield strength for simulation. Kernel units are millimetres, so
//! densities are stored in g/mm³ and stresses/moduli in MPa (N/mm²), which keeps
//! FEA results in consistent units (force in N, stress in MPa).

/// Physical properties of an engineering material.
#[derive(Debug, Clone, PartialEq)]
pub struct Material {
    /// Human-readable name (e.g. "Steel", "PLA").
    pub name: String,
    /// Density in g/mm³ (kernel units are millimetres).
    pub density: f64,
    /// Young's modulus (elastic modulus) in MPa (N/mm²).
    pub youngs_modulus: f64,
    /// Poisson's ratio (dimensionless, typically 0.2–0.45).
    pub poisson_ratio: f64,
    /// Yield strength in MPa; failure/safety-factor reference for simulation.
    pub yield_strength: f64,
}

impl Material {
    /// Construct a material from explicit properties.
    pub fn new(
        name: impl Into<String>,
        density: f64,
        youngs_modulus: f64,
        poisson_ratio: f64,
        yield_strength: f64,
    ) -> Self {
        Material {
            name: name.into(),
            density,
            youngs_modulus,
            poisson_ratio,
            yield_strength,
        }
    }

    /// Look up a material from the built-in table by (case-insensitive) name,
    /// falling back to a generic engineering plastic if unknown.
    pub fn from_name(name: &str) -> Material {
        for m in Material::table() {
            if m.name.eq_ignore_ascii_case(name) {
                return m;
            }
        }
        Material {
            name: name.to_string(),
            ..Material::generic_plastic()
        }
    }

    /// The shared built-in material table. Densities in g/mm³, moduli/strengths
    /// in MPa. This is the single source of truth folded in from the former
    /// `manufacturing::bom` density table.
    pub fn table() -> Vec<Material> {
        vec![
            Material::new("Steel", 7.85e-3, 200_000.0, 0.30, 250.0),
            Material::new("Aluminum", 2.70e-3, 69_000.0, 0.33, 95.0),
            Material::new("Brass", 8.50e-3, 100_000.0, 0.34, 200.0),
            Material::new("Titanium", 4.51e-3, 116_000.0, 0.32, 880.0),
            Material::new("PLA", 1.24e-3, 3_500.0, 0.36, 50.0),
            Material::new("ABS", 1.04e-3, 2_300.0, 0.35, 40.0),
            Material::new("PETG", 1.27e-3, 2_100.0, 0.40, 50.0),
            Material::new("Nylon", 1.14e-3, 2_000.0, 0.39, 45.0),
        ]
    }

    /// A generic engineering plastic used as the default fallback.
    pub fn generic_plastic() -> Material {
        Material::new("Generic Plastic", 1.2e-3, 2_000.0, 0.35, 40.0)
    }

    /// Density in g/mm³ for the named material (BOM convenience).
    pub fn density_of(name: &str) -> f64 {
        Material::from_name(name).density
    }
}

impl Default for Material {
    fn default() -> Self {
        Material::from_name("Steel")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_material_lookup() {
        let steel = Material::from_name("steel");
        assert_eq!(steel.name, "Steel");
        assert!((steel.density - 7.85e-3).abs() < 1e-12);
        assert!((steel.youngs_modulus - 200_000.0).abs() < 1e-6);
    }

    #[test]
    fn unknown_material_falls_back() {
        let m = Material::from_name("Unobtanium");
        assert_eq!(m.name, "Unobtanium");
        assert!((m.density - Material::generic_plastic().density).abs() < 1e-12);
    }

    #[test]
    fn density_helper_matches_table() {
        assert!((Material::density_of("Aluminum") - 2.70e-3).abs() < 1e-12);
    }
}
