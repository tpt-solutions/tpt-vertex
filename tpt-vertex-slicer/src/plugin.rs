//! Adapter exposing slicing as an [`ExporterPlugin`], so a
//! [`PluginRegistry`](tpt_vertex_manufacturing::plugin::PluginRegistry) can
//! offer "export to G-code" alongside STL/OBJ/glTF/STEP, without the
//! manufacturing crate needing to know slicing exists.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0

use tpt_vertex_kernel::geometry::solid::Solid;
use tpt_vertex_manufacturing::plugin::{ExporterPlugin, PluginInfo, PluginError};

use crate::profile::{MaterialCalibration, PrinterProfile, SliceSettings};
use crate::slice::slice_solid_to_gcode;

/// Exports a [`Solid`] as G-code via the slicer, for use in a
/// [`PluginRegistry`](tpt_vertex_manufacturing::plugin::PluginRegistry).
#[derive(Default)]
pub struct GcodeExporterPlugin {
    pub printer: PrinterProfile,
    pub settings: SliceSettings,
    pub material: MaterialCalibration,
}

impl ExporterPlugin for GcodeExporterPlugin {
    fn info(&self) -> PluginInfo {
        PluginInfo {
            id: "gcode",
            name: "G-code (FDM slice)",
            extension: "gcode",
        }
    }

    fn export(&self, solid: &Solid, _name: &str) -> Result<Vec<u8>, PluginError> {
        let result = slice_solid_to_gcode(
            solid,
            &self.printer,
            &self.settings,
            &self.material,
            None,
            None,
        );
        if result.layers.is_empty() {
            return Err(PluginError::Failed(
                "solid produced no slice layers (empty or degenerate mesh)".to_string(),
            ));
        }
        Ok(result.gcode.text.into_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_vertex_kernel::geometry::solid::Face;
    use tpt_vertex_kernel::math::Vec3;
    use tpt_vertex_manufacturing::plugin::PluginRegistry;

    fn cube() -> Solid {
        let mut s = Solid::new();
        let mut v = |x: f64, y: f64, z: f64| s.add_vertex(Vec3::new(x, y, z));
        let p = [
            v(-5.0, -5.0, 0.0), v(5.0, -5.0, 0.0), v(5.0, 5.0, 0.0), v(-5.0, 5.0, 0.0),
            v(-5.0, -5.0, 10.0), v(5.0, -5.0, 10.0), v(5.0, 5.0, 10.0), v(-5.0, 5.0, 10.0),
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

    #[test]
    fn exports_gcode_bytes() {
        let plugin = GcodeExporterPlugin::default();
        let bytes = plugin.export(&cube(), "cube").unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.contains("G1 X"));
    }

    #[test]
    fn registers_and_dispatches_via_plugin_registry() {
        let mut registry = PluginRegistry::with_builtins();
        registry.register_exporter(Box::new(GcodeExporterPlugin::default()));
        let bytes = registry.export("gcode", &cube(), "cube").unwrap();
        assert!(!bytes.is_empty());
    }
}
