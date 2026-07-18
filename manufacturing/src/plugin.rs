//! Plugin / extension API for custom tools and file-format support.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! This module defines the public extension surface for TPT Vertex's
//! manufacturing/interop layer. Third-party crates implement one of the plugin
//! traits and register instances with a [`PluginRegistry`]. The registry is what
//! the application (and desktop/CLI) drives to enumerate available formats and
//! run import/export by name — so new formats can be added without changing core
//! code.
//!
//! Three extension points are provided:
//! - [`ExporterPlugin`]: serialize a kernel [`Solid`] to bytes in some format.
//! - [`ImporterPlugin`]: parse bytes back into a [`Solid`].
//! - [`ToolPlugin`]: an arbitrary geometry-processing tool (a `Solid -> Solid`
//!   transform), e.g. mesh decimation, custom fillets, validation passes.
//!
//! The built-in STL/OBJ/glTF/STEP exporters and the STEP importer are exposed as
//! plugins via [`PluginRegistry::with_builtins`], demonstrating the intended
//! shape for external plugins.

use tpt_vertex_kernel::geometry::solid::Solid;

use crate::export::{export_obj, write_stl_ascii, write_stl_binary, StlError};
use crate::step::{export_step, import_step};

/// Metadata describing a format/tool a plugin provides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginInfo {
    /// Stable id used to select the plugin programmatically (e.g. "stl-binary").
    pub id: &'static str,
    /// Human-readable name (e.g. "STL (binary)").
    pub name: &'static str,
    /// Canonical file extension without the dot (e.g. "stl").
    pub extension: &'static str,
}

/// Error type surfaced across the plugin boundary.
#[derive(Debug)]
pub enum PluginError {
    /// The requested plugin id was not registered.
    NotFound(String),
    /// The plugin failed while processing.
    Failed(String),
}

impl std::fmt::Display for PluginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PluginError::NotFound(id) => write!(f, "no plugin registered with id '{id}'"),
            PluginError::Failed(msg) => write!(f, "plugin failed: {msg}"),
        }
    }
}

impl std::error::Error for PluginError {}

impl From<StlError> for PluginError {
    fn from(e: StlError) -> Self {
        PluginError::Failed(e.to_string())
    }
}

/// A plugin that exports a [`Solid`] to a byte buffer.
pub trait ExporterPlugin: Send + Sync {
    fn info(&self) -> PluginInfo;
    /// Export `solid` (named `name`) to bytes.
    fn export(&self, solid: &Solid, name: &str) -> Result<Vec<u8>, PluginError>;
}

/// A plugin that imports bytes into a [`Solid`].
pub trait ImporterPlugin: Send + Sync {
    fn info(&self) -> PluginInfo;
    fn import(&self, bytes: &[u8]) -> Result<Solid, PluginError>;
}

/// A plugin that performs an arbitrary geometry transform.
pub trait ToolPlugin: Send + Sync {
    fn info(&self) -> PluginInfo;
    fn run(&self, input: &Solid) -> Result<Solid, PluginError>;
}

/// Registry of installed plugins. The host application owns one of these.
#[derive(Default)]
pub struct PluginRegistry {
    exporters: Vec<Box<dyn ExporterPlugin>>,
    importers: Vec<Box<dyn ImporterPlugin>>,
    tools: Vec<Box<dyn ToolPlugin>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        PluginRegistry::default()
    }

    /// A registry pre-populated with the built-in format plugins.
    pub fn with_builtins() -> Self {
        let mut r = PluginRegistry::new();
        r.register_exporter(Box::new(StlBinaryExporter));
        r.register_exporter(Box::new(StlAsciiExporter));
        r.register_exporter(Box::new(ObjExporter));
        r.register_exporter(Box::new(StepExporter));
        r.register_importer(Box::new(StepImporter));
        r
    }

    pub fn register_exporter(&mut self, p: Box<dyn ExporterPlugin>) {
        self.exporters.push(p);
    }

    pub fn register_importer(&mut self, p: Box<dyn ImporterPlugin>) {
        self.importers.push(p);
    }

    pub fn register_tool(&mut self, p: Box<dyn ToolPlugin>) {
        self.tools.push(p);
    }

    /// Enumerate available exporters.
    pub fn exporters(&self) -> impl Iterator<Item = PluginInfo> + '_ {
        self.exporters.iter().map(|p| p.info())
    }

    /// Enumerate available importers.
    pub fn importers(&self) -> impl Iterator<Item = PluginInfo> + '_ {
        self.importers.iter().map(|p| p.info())
    }

    /// Enumerate available tools.
    pub fn tools(&self) -> impl Iterator<Item = PluginInfo> + '_ {
        self.tools.iter().map(|p| p.info())
    }

    /// Export using the plugin with `id`.
    pub fn export(&self, id: &str, solid: &Solid, name: &str) -> Result<Vec<u8>, PluginError> {
        let p = self
            .exporters
            .iter()
            .find(|p| p.info().id == id)
            .ok_or_else(|| PluginError::NotFound(id.to_string()))?;
        p.export(solid, name)
    }

    /// Import using the plugin with `id`.
    pub fn import(&self, id: &str, bytes: &[u8]) -> Result<Solid, PluginError> {
        let p = self
            .importers
            .iter()
            .find(|p| p.info().id == id)
            .ok_or_else(|| PluginError::NotFound(id.to_string()))?;
        p.import(bytes)
    }

    /// Run the tool with `id`.
    pub fn run_tool(&self, id: &str, input: &Solid) -> Result<Solid, PluginError> {
        let p = self
            .tools
            .iter()
            .find(|p| p.info().id == id)
            .ok_or_else(|| PluginError::NotFound(id.to_string()))?;
        p.run(input)
    }
}

// --- Built-in plugin adapters -------------------------------------------------

struct StlBinaryExporter;
impl ExporterPlugin for StlBinaryExporter {
    fn info(&self) -> PluginInfo {
        PluginInfo {
            id: "stl-binary",
            name: "STL (binary)",
            extension: "stl",
        }
    }
    fn export(&self, solid: &Solid, _name: &str) -> Result<Vec<u8>, PluginError> {
        let mut buf = Vec::new();
        write_stl_binary(&mut buf, solid)?;
        Ok(buf)
    }
}

struct StlAsciiExporter;
impl ExporterPlugin for StlAsciiExporter {
    fn info(&self) -> PluginInfo {
        PluginInfo {
            id: "stl-ascii",
            name: "STL (ASCII)",
            extension: "stl",
        }
    }
    fn export(&self, solid: &Solid, _name: &str) -> Result<Vec<u8>, PluginError> {
        let mut buf = Vec::new();
        write_stl_ascii(&mut buf, solid)?;
        Ok(buf)
    }
}

struct ObjExporter;
impl ExporterPlugin for ObjExporter {
    fn info(&self) -> PluginInfo {
        PluginInfo {
            id: "obj",
            name: "Wavefront OBJ",
            extension: "obj",
        }
    }
    fn export(&self, solid: &Solid, _name: &str) -> Result<Vec<u8>, PluginError> {
        let mut buf = Vec::new();
        export_obj(&mut buf, solid)?;
        Ok(buf)
    }
}

struct StepExporter;
impl ExporterPlugin for StepExporter {
    fn info(&self) -> PluginInfo {
        PluginInfo {
            id: "step",
            name: "STEP AP203/214",
            extension: "step",
        }
    }
    fn export(&self, solid: &Solid, name: &str) -> Result<Vec<u8>, PluginError> {
        let mut buf = Vec::new();
        export_step(&mut buf, solid, name)?;
        Ok(buf)
    }
}

struct StepImporter;
impl ImporterPlugin for StepImporter {
    fn info(&self) -> PluginInfo {
        PluginInfo {
            id: "step",
            name: "STEP AP203/214",
            extension: "step",
        }
    }
    fn import(&self, bytes: &[u8]) -> Result<Solid, PluginError> {
        Ok(import_step(bytes)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_vertex_kernel::feature_tree::{Feature, FeatureTree};
    use tpt_vertex_kernel::geometry::sketch::Sketch;
    use tpt_vertex_kernel::math::Vec2;

    fn box_solid() -> Solid {
        let mut s = Sketch::new();
        s.line(Vec2::ZERO, Vec2::new(2.0, 0.0));
        s.line(Vec2::new(2.0, 0.0), Vec2::new(2.0, 2.0));
        s.line(Vec2::new(2.0, 2.0), Vec2::ZERO);
        let mut tree = FeatureTree::new();
        tree.add(
            Feature::Extrude {
                sketch: s,
                height: 3.0,
            },
            None,
        );
        tree.evaluate().unwrap().final_solid
    }

    #[test]
    fn builtins_are_registered() {
        let r = PluginRegistry::with_builtins();
        let ids: Vec<_> = r.exporters().map(|i| i.id).collect();
        assert!(ids.contains(&"stl-binary"));
        assert!(ids.contains(&"step"));
        assert!(r.importers().any(|i| i.id == "step"));
    }

    #[test]
    fn export_by_id_dispatches() {
        let r = PluginRegistry::with_builtins();
        let bytes = r.export("obj", &box_solid(), "Block").unwrap();
        assert!(String::from_utf8(bytes).unwrap().contains("\nf "));
    }

    #[test]
    fn unknown_id_errors() {
        let r = PluginRegistry::with_builtins();
        assert!(matches!(
            r.export("nope", &box_solid(), "x"),
            Err(PluginError::NotFound(_))
        ));
    }

    #[test]
    fn custom_tool_plugin_runs() {
        struct Reverse;
        impl ToolPlugin for Reverse {
            fn info(&self) -> PluginInfo {
                PluginInfo {
                    id: "reverse-winding",
                    name: "Reverse Winding",
                    extension: "",
                }
            }
            fn run(&self, input: &Solid) -> Result<Solid, PluginError> {
                let mut s = input.clone();
                s.reverse_winding();
                Ok(s)
            }
        }
        let mut r = PluginRegistry::new();
        r.register_tool(Box::new(Reverse));
        let out = r.run_tool("reverse-winding", &box_solid()).unwrap();
        assert_eq!(out.triangle_count(), box_solid().triangle_count());
    }

    #[test]
    fn step_export_import_via_registry_round_trips() {
        let r = PluginRegistry::with_builtins();
        let solid = box_solid();
        let bytes = r.export("step", &solid, "Block").unwrap();
        let back = r.import("step", &bytes).unwrap();
        assert_eq!(back.triangle_count(), solid.triangle_count());
    }
}
