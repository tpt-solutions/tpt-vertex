# tpt-vertex-slicer

FDM 3D-printing slicer for [TPT Vertex](https://tpt-vertex.dev), the open-source
parametric CAD platform.

Turns a kernel `Solid` mesh into printable G-code: planar layering, perimeter/wall
generation via polygon offsetting, rectilinear/zigzag infill, and toolpath/G-code
emission for a configurable FDM printer profile. Ships feature-tree-native slicing
(`slice_feature_tree`) and an `ExporterPlugin` adapter so it slots into the
manufacturing export pipeline.

**Status:** implemented — not a placeholder. The crate provides a full FDM slicing
pipeline: layering, perimeters, infill, toolpath ordering, G-code emission,
supports, adaptive layer height, bridging, multi-material, variable-width
perimeters, seam placement, mesh repair, and feature-tree-native slicing.

## What's implemented

- **Layering** — planar slicing of a triangle mesh into closed contours; adaptive
  layer height driven by surface slope.
- **Walls & infill** — polygon-offset perimeters plus rectilinear/zigzag infill
  with stress-driven density (fed a `tpt-vertex-simulation` von Mises field).
- **Supports** — basic grid/pillar and tree/organic supports; mesh repair pass.
- **Toolpaths** — ordered perimeters → infill → travel/retraction, multi-extruder
  tool changes, bridging detection, seam placement, variable-width thin-wall fill.
- **G-code emission** — Marlin/Klipper-style program with estimated print time and
  filament usage.
- **Estimates** — `GCode` reports `estimated_time_s`, `estimated_filament_mm`, and
  `estimated_filament_g` (mass from extruded volume × material density). The
  `CalibrationFactors` (`time_factor`, `filament_factor`) tune these against
  measured printer data — uncalibrated by default (see the
  [todo](https://github.com/tpt-solutions/vertex/blob/main/todo.md) "calibrate
  against real printer data" task).
- **Static validation** — `gcode_validate::validate_gcode` checks emitted G-code
  for unsupported commands, negative extrusion, missing home, and out-of-bounds
  coordinates (structure/syntax only; not a hardware/simulator validation).

## Example

```rust
use tpt_vertex_kernel::geometry::solid::Solid;
use tpt_vertex_slicer::{slice_solid, CalibrationFactors};

let solid: Solid = /* ... evaluated kernel solid ... */;
let res = slice_solid(&solid);
println!(
    "{} layers, ~{:.1} g filament, ~{:.0} s",
    res.layers.len(), res.gcode.estimated_filament_g, res.gcode.estimated_time_s
);

// Re-emit with calibration factors once measured on real hardware
// (default factors are 1.0 — uncalibrated):
let _calibrated = CalibrationFactors { time_factor: 1.05, filament_factor: 0.98 };
// emit_gcode_calibrated(&res.layers, &printer, &material, &_calibrated)
```

## License

Dual-licensed under [MIT](../LICENSE-MIT) or [Apache-2.0](../LICENSE-APACHE), at
your option.
