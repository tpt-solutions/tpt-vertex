# ADR-0008: Slicing architecture — standalone crate with hand-rolled offset

- Status: Accepted
- Date: 2026-07-20

## Context

Phase 10 adds FDM 3D-printing support: turning a kernel `Solid` mesh into
printable G-code (planar layering, perimeter/wall generation, infill, toolpath
ordering, G-code emission). Two axes of decision:

1. **Packaging** — should slicing live in a standalone crate, inside the
   `manufacturing` crate, or be expressed purely as an `ExporterPlugin`
   (`manufacturing/src/plugin.rs`)?
2. **Polygon offset** — the riskiest kernel. Should we depend on an external
   robust polygon-offset/clipping library (e.g. a Clipper port) or hand-roll a
   miter offset?

Forces:

- The slicer is a substantial, self-contained algorithmic pipeline with its own
  configuration surface (printer profiles, material calibration) and its own
  cadence of fast-follow work (supports, adaptive layers, Arachne walls).
- The existing plugin API is oriented around one-shot mesh exporters
  (`&Solid -> bytes`); slicing needs richer configuration and produces
  intermediate artifacts (layer plans) useful for a live preview UI.
- Offset robustness is the classic slicer failure mode; an external library is
  more robust but adds a dependency and a licensing/porting surface to a project
  that has kept its kernel dependency-free.

## Decision

1. **Standalone crate `tpt-vertex-slicer`.** Slicing is its own workspace member
   depending only on `tpt-vertex-kernel`. This keeps the algorithm cohesive,
   independently testable/benchmarkable, and free to evolve its own API. The
   crate still exposes a thin `slice_solid` convenience entry point, and a future
   `ExporterPlugin` adapter (a Phase 10 fast-follow) can wrap it so slicing is
   also reachable through the generic plugin registry.

2. **Hand-rolled miter offset for v1.** `offset::offset_contour` uses the
   standard miter vertex `v' = v + d·(n1 + n2)/(1 + n1·n2)` with unit outward
   edge normals, clamping the denominator to bound spikes. This is dependency-
   free, correct for convex and mildly concave contours, and — critically —
   does not blow up at collinear vertices (the failure mode we hit and fixed;
   see `docs/specs/slicer-geometry-kernels.md`, P3.2). Evaluating a robust
   external offset/clipping library remains an explicit fast-follow for complex
   concave/self-intersecting regions and holes.

## Consequences

- Positive: cohesive, dependency-free slicer; fast unit + property tests; clear
  place for the large Phase 10 fast-follow backlog; the kernel stays clean.
- Positive: the intermediate `LayerPlan` representation directly feeds a
  layer-preview UI and the desktop `slice_model` command.
- Negative: the hand-rolled offset is not robust for arbitrary self-intersecting
  polygons or multi-contour regions with holes; those require the fast-follow
  library evaluation before production use on complex parts.
- Negative: two slicing entry points (native crate API + future plugin adapter)
  must be kept consistent; the adapter is deferred until the plugin API grows a
  configuration channel.
- Follow-up: expose slicing as an `ExporterPlugin`; evaluate a robust polygon
  offset library; add support generation, adaptive layers, and Arachne-style
  variable-width walls (tracked in `todo.md` Phase 10 fast-follows).
