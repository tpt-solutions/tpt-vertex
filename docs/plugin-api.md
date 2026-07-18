# TPT Vertex Public API & Plugin Interface

This document describes the stable, public extension surface of TPT Vertex: the
crate APIs third parties build against, and the plugin interface for adding
custom tools and file formats without modifying the core.

> Status: v0.1. The interfaces below are the intended stable surface. Anything
> not listed here (private modules, internal helpers) may change without notice.

## Crate map

| Crate                        | Purpose                                              |
| ---------------------------- | ---------------------------------------------------- |
| `tpt-vertex-kernel`          | Geometry: math, sketches, features, `Solid`, assemblies |
| `tpt-vertex-renderer`        | WebGPU/wgpu rendering of kernel geometry             |
| `tpt-vertex-manufacturing`   | Export/import (STL, OBJ, glTF, STEP), BOM, drawings, **plugin registry** |
| `tpt-vertex-versioning`      | Git-like commits/branches/merge over feature manifests |
| `tpt-vertex-collab`          | CRDT document + sync hub for real-time collaboration |

The **kernel `Solid`** (`tpt_vertex_kernel::geometry::solid::Solid`) is the
common currency: a faceted B-rep of a shared vertex pool plus triangle faces
(see [ADR-0004](adr/0004-geometric-representation.md)). Most plugins consume and
produce a `Solid`.

## Plugin interface

The plugin system lives in `tpt_vertex_manufacturing::plugin`. A host application
owns a `PluginRegistry`, registers plugins, and drives them by id. Three
extension points are defined.

### 1. Exporter plugins

Serialize a `Solid` to bytes in some format.

```rust
use tpt_vertex_manufacturing::plugin::{ExporterPlugin, PluginInfo, PluginError};
use tpt_vertex_kernel::geometry::solid::Solid;

struct MyExporter;

impl ExporterPlugin for MyExporter {
    fn info(&self) -> PluginInfo {
        PluginInfo { id: "my-format", name: "My Format", extension: "myf" }
    }
    fn export(&self, solid: &Solid, name: &str) -> Result<Vec<u8>, PluginError> {
        // ... write bytes ...
        Ok(Vec::new())
    }
}
```

### 2. Importer plugins

Parse bytes back into a `Solid`.

```rust
use tpt_vertex_manufacturing::plugin::{ImporterPlugin, PluginInfo, PluginError};
use tpt_vertex_kernel::geometry::solid::Solid;

struct MyImporter;

impl ImporterPlugin for MyImporter {
    fn info(&self) -> PluginInfo {
        PluginInfo { id: "my-format", name: "My Format", extension: "myf" }
    }
    fn import(&self, bytes: &[u8]) -> Result<Solid, PluginError> {
        Ok(Solid::new())
    }
}
```

### 3. Tool plugins

An arbitrary geometry transform (`Solid -> Solid`): decimation, validation,
custom fillets, repair passes, etc.

```rust
use tpt_vertex_manufacturing::plugin::{ToolPlugin, PluginInfo, PluginError};
use tpt_vertex_kernel::geometry::solid::Solid;

struct FlipNormals;

impl ToolPlugin for FlipNormals {
    fn info(&self) -> PluginInfo {
        PluginInfo { id: "flip-normals", name: "Flip Normals", extension: "" }
    }
    fn run(&self, input: &Solid) -> Result<Solid, PluginError> {
        let mut s = input.clone();
        s.reverse_winding();
        Ok(s)
    }
}
```

### Registering and driving plugins

```rust
use tpt_vertex_manufacturing::plugin::PluginRegistry;

// Start with the built-ins (STL binary/ASCII, OBJ, STEP export, STEP import)...
let mut registry = PluginRegistry::with_builtins();

// ...and add your own:
registry.register_exporter(Box::new(MyExporter));
registry.register_tool(Box::new(FlipNormals));

// Enumerate what's available (e.g. to populate an Export menu):
for info in registry.exporters() {
    println!("{} (.{})", info.name, info.extension);
}

// Run by id:
// let bytes = registry.export("step", &solid, "Part")?;
// let solid = registry.import("step", &bytes)?;
// let repaired = registry.run_tool("flip-normals", &solid)?;
```

## Built-in plugins

| Kind     | id           | Format             |
| -------- | ------------ | ------------------ |
| Exporter | `stl-binary` | STL (binary)       |
| Exporter | `stl-ascii`  | STL (ASCII)        |
| Exporter | `obj`        | Wavefront OBJ      |
| Exporter | `step`       | STEP AP203/214     |
| Importer | `step`       | STEP AP203/214     |

glTF export is available directly via `tpt_vertex_manufacturing::export_gltf`
(it returns a `(json, bin)` pair rather than a single byte buffer, so it is not
exposed through the byte-oriented `ExporterPlugin` trait).

## Direct APIs

Formats can also be called directly without the registry:

- `write_stl_binary(w, &solid)`, `write_stl_ascii(w, &solid)`
- `export_obj(w, &solid)`
- `export_gltf(&solid) -> (String, Vec<u8>)`
- `export_step(w, &solid, name)`, `import_step(r) -> Solid`
- `bom::bom_from_assembly(&assembly, &materials)`, `drawing::drawing_svg(&solid)`

## Stability & versioning

Plugin traits and `PluginRegistry` follow the workspace semver. Breaking changes
to a public trait bump the crate's minor version pre-1.0 and major version
post-1.0, and are recorded in the changelog. `PluginInfo.id` values for built-in
plugins are stable identifiers safe to persist in project files.
