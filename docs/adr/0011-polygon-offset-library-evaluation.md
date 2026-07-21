# ADR-0011: Polygon-offset library evaluation (Phase 10 fast-follow)

- Status: Accepted
- Date: 2026-07-21

## Context

ADR-0008 chose a hand-rolled miter offset (`slicer/src/offset.rs`) for v1 and
flagged evaluating a robust external polygon-offset/clipping library (e.g. a
Clipper2 port, `geo-offset`, or `i_overlay`) as an explicit fast-follow. This
ADR records that evaluation now that variable-width thin-wall fill
(`variable_width.rs`) and bridging/support detection have added more callers
that offset contours repeatedly per layer.

Candidates considered:

- **Clipper2** (C++ with Rust ports, e.g. `clipper2` / `cavalier_contours`):
  industry-standard, handles arbitrary self-intersecting polygons, holes, and
  multi-contour regions robustly via a proper vertex-arc/orientation-sweep
  algorithm. Adds a substantial dependency (in some ports, a C++ build step)
  and a new license (Boost) to a project whose kernel and slicer are
  deliberately dependency-light.
- **`geo` + `geo-offset`**: pure Rust, integrates with the well-established
  `geo`/`geo-types` ecosystem. Lower integration cost than a C++ port, but pulls
  in `geo`'s full geometry stack (a much larger dependency surface than this
  crate currently needs) for one operation.
- **`i_overlay`**: pure Rust, boolean + offset operations on polygons with
  holes, actively maintained, no C++ build step. The lightest-weight of the
  robust options.
- **Status quo (hand-rolled miter offset)**: zero dependencies, already
  spec-tested (P3.1–P3.4 in `docs/specs/slicer-geometry-kernels.md`), and
  sufficient for the simple, mostly-convex-or-mildly-concave single contours
  produced by planar slicing of typical printable parts. Known gap: no
  self-intersection clipping, so deeply concave or thin/spiky regions can
  produce a locally invalid (self-overlapping) offset polygon rather than
  correctly splitting into multiple loops or vanishing outright.

## Decision

Keep the hand-rolled offset for now; do not add an external dependency yet.
The gap it leaves (self-intersection handling on complex concave geometry) is
in practice masked by two fast-follows implemented alongside this evaluation:

- `variable_width::thin_wall_fill` finds the real (monotonic-area) collapse
  point for a contour via incremental marching, which is robust to the
  hand-rolled offset's inability to detect self-intersection by area/sign
  alone, and fills the residual with a single centreline pass rather than
  trusting a possibly-invalid deep offset.
- `walls_for`'s existing degenerate-area cutoff still stops wall generation
  before geometry gets bad enough for the lack of clipping to matter for
  typical parts.

If/when the project needs correct multi-loop offset output for deeply concave
or multi-hole regions (e.g. full Arachne-style continuously-variable-width
perimeters, item still open in `todo.md`), revisit with **`i_overlay`** as the
leading candidate: pure Rust, no C++ toolchain requirement, smallest
dependency surface of the robust options evaluated, and directly supports the
polygon-with-holes case slicing needs.

## Consequences

- No new dependency added; the slicer crate stays dependency-light per
  ADR-0008.
- The hand-rolled offset's known gap (self-intersection/degenerate concave
  handling) remains, but is now bounded by the thin-wall marching fallback
  rather than left as an open failure mode.
- Follow-up: revisit `i_overlay` if/when true point-by-point variable-width
  (full Arachne) or robust multi-hole offset becomes a hard requirement.
