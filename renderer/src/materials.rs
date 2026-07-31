//! Photorealistic rendering material presets.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! A [`RenderMaterial`] defines the visual appearance of a surface for the
//! PBR-style shader: albedo (base colour), metallic, roughness, and an
//! optional emissive factor.  The presets cover common engineering and
//! aesthetic materials — metals, plastics, composites, and finishes.

/// Linear-space RGBA colour (each channel 0.0–1.0).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Color { r, g, b, a }
    }

    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Color { r, g, b, a: 1.0 }
    }
}

/// A rendering material defining the visual surface properties.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderMaterial {
    /// Human-readable name (e.g. "Brushed Steel").
    pub name: String,
    /// Base (albedo) colour in linear space.
    pub albedo: Color,
    /// Metallic factor: 0 = dielectric, 1 = pure metal.
    pub metallic: f32,
    /// Surface roughness: 0 = mirror-smooth, 1 = fully diffuse.
    pub roughness: f32,
    /// Emissive intensity (0 = none, > 0 = glows).
    pub emissive: f32,
    /// Optional texture path (e.g. a brushed-metal normal map).
    pub texture: Option<String>,
}

impl RenderMaterial {
    pub fn new(name: impl Into<String>, albedo: Color, metallic: f32, roughness: f32) -> Self {
        RenderMaterial {
            name: name.into(),
            albedo,
            metallic,
            roughness,
            emissive: 0.0,
            texture: None,
        }
    }

    /// Build the built-in material preset library.
    pub fn presets() -> Vec<RenderMaterial> {
        vec![
            // Metals
            RenderMaterial::new("Polished Steel", Color::rgb(0.72, 0.73, 0.76), 0.95, 0.15),
            RenderMaterial::new("Brushed Steel", Color::rgb(0.68, 0.70, 0.73), 0.90, 0.35),
            RenderMaterial::new("Aluminum", Color::rgb(0.82, 0.83, 0.85), 0.90, 0.25),
            RenderMaterial::new("Anodized Red", Color::rgb(0.70, 0.15, 0.10), 0.80, 0.30),
            RenderMaterial::new("Anodized Black", Color::rgb(0.08, 0.08, 0.08), 0.70, 0.40),
            RenderMaterial::new("Brass", Color::rgb(0.78, 0.62, 0.32), 0.90, 0.20),
            RenderMaterial::new("Copper", Color::rgb(0.72, 0.45, 0.20), 0.90, 0.25),
            RenderMaterial::new("Gold", Color::rgb(0.83, 0.69, 0.22), 0.95, 0.10),
            RenderMaterial::new("Titanium", Color::rgb(0.62, 0.62, 0.66), 0.85, 0.30),
            RenderMaterial::new("Chrome", Color::rgb(0.85, 0.85, 0.87), 0.95, 0.05),
            RenderMaterial::new("Cast Iron", Color::rgb(0.40, 0.40, 0.42), 0.85, 0.55),
            RenderMaterial::new("Tungsten", Color::rgb(0.55, 0.55, 0.58), 0.90, 0.30),
            // Plastics
            RenderMaterial::new("Matte White", Color::rgb(0.90, 0.90, 0.90), 0.0, 0.80),
            RenderMaterial::new("Matte Black", Color::rgb(0.05, 0.05, 0.05), 0.0, 0.85),
            RenderMaterial::new("Glossy Red", Color::rgb(0.80, 0.08, 0.08), 0.0, 0.15),
            RenderMaterial::new("Glossy Blue", Color::rgb(0.10, 0.25, 0.80), 0.0, 0.15),
            RenderMaterial::new("Glossy Green", Color::rgb(0.10, 0.65, 0.20), 0.0, 0.15),
            RenderMaterial::new("PLA White", Color::rgb(0.88, 0.88, 0.86), 0.0, 0.65),
            RenderMaterial::new("PLA Black", Color::rgb(0.08, 0.08, 0.08), 0.0, 0.70),
            RenderMaterial::new("PETG Natural", Color::rgb(0.82, 0.82, 0.80), 0.0, 0.50),
            RenderMaterial::new("ABS White", Color::rgb(0.87, 0.87, 0.85), 0.0, 0.70),
            RenderMaterial::new("Nylon Natural", Color::rgb(0.80, 0.80, 0.75), 0.0, 0.55),
            RenderMaterial::new("TPU Black", Color::rgb(0.08, 0.08, 0.08), 0.0, 0.90),
            // Composites & Finishes
            RenderMaterial::new("Carbon Fiber", Color::rgb(0.15, 0.15, 0.17), 0.30, 0.40),
            RenderMaterial::new("Rubber", Color::rgb(0.10, 0.10, 0.10), 0.0, 0.95),
            RenderMaterial::new("Glass", Color::rgb(0.85, 0.88, 0.90), 0.10, 0.05),
            RenderMaterial::new("Ceramic", Color::rgb(0.85, 0.82, 0.78), 0.0, 0.60),
            RenderMaterial::new("Wood (Oak)", Color::rgb(0.65, 0.45, 0.25), 0.0, 0.75),
            RenderMaterial::new("Wood (Walnut)", Color::rgb(0.35, 0.22, 0.12), 0.0, 0.70),
            RenderMaterial::new("Leather", Color::rgb(0.40, 0.25, 0.15), 0.0, 0.85),
            RenderMaterial::new("Fabric (Cotton)", Color::rgb(0.85, 0.82, 0.78), 0.0, 0.95),
            // Engineering
            RenderMaterial::new("Concrete", Color::rgb(0.60, 0.58, 0.55), 0.0, 0.90),
            RenderMaterial::new("Marble", Color::rgb(0.88, 0.86, 0.84), 0.05, 0.30),
            RenderMaterial::new("Granite", Color::rgb(0.45, 0.43, 0.42), 0.10, 0.60),
            RenderMaterial::new("Bronze", Color::rgb(0.55, 0.42, 0.20), 0.88, 0.35),
            RenderMaterial::new("Zinc", Color::rgb(0.65, 0.67, 0.70), 0.85, 0.30),
            RenderMaterial::new("Nickel", Color::rgb(0.70, 0.68, 0.65), 0.90, 0.20),
            // Transparent / Translucent
            RenderMaterial::new("Clear Acrylic", Color::rgb(0.92, 0.92, 0.95), 0.0, 0.05),
            RenderMaterial::new("Amber", Color::rgb(0.80, 0.55, 0.10), 0.0, 0.15),
            RenderMaterial::new("Ruby", Color::rgb(0.60, 0.05, 0.10), 0.10, 0.10),
            RenderMaterial::new("Sapphire", Color::rgb(0.10, 0.15, 0.50), 0.10, 0.10),
            // Emissive
            RenderMaterial {
                name: "LED Blue".to_string(),
                albedo: Color::rgb(0.10, 0.20, 0.80),
                metallic: 0.0,
                roughness: 0.3,
                emissive: 0.8,
                texture: None,
            },
            RenderMaterial {
                name: "LED Green".to_string(),
                albedo: Color::rgb(0.10, 0.70, 0.15),
                metallic: 0.0,
                roughness: 0.3,
                emissive: 0.8,
                texture: None,
            },
        ]
    }

    /// Look up a preset by (case-insensitive) name, returning `None` if
    /// not found.
    pub fn by_name(name: &str) -> Option<RenderMaterial> {
        Self::presets()
            .into_iter()
            .find(|m| m.name.eq_ignore_ascii_case(name))
    }

    /// Convert to the 4-float base colour array expected by the GPU shader.
    pub fn to_shader_color(&self) -> [f32; 4] {
        [self.albedo.r, self.albedo.g, self.albedo.b, self.albedo.a]
    }
}

impl Default for RenderMaterial {
    fn default() -> Self {
        RenderMaterial::new("Default", Color::rgb(0.75, 0.77, 0.80), 0.5, 0.4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_include_common_materials() {
        let presets = RenderMaterial::presets();
        assert!(presets.len() >= 15);
        assert!(RenderMaterial::by_name("Gold").is_some());
        assert!(RenderMaterial::by_name("Carbon Fiber").is_some());
    }

    #[test]
    fn shader_color_is_rgba() {
        let m = RenderMaterial::default();
        let c = m.to_shader_color();
        assert_eq!(c.len(), 4);
        assert_eq!(c[3], 1.0);
    }

    #[test]
    fn case_insensitive_lookup() {
        assert!(RenderMaterial::by_name("brass").is_some());
        assert!(RenderMaterial::by_name("BRASS").is_some());
    }
}
