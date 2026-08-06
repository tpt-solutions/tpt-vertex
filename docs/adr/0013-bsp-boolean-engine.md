# ADR-0013: BSP-Tree Triangle-Mesh Boolean Engine

- Status: Accepted
- Date: 2026-08-06

## Context

ADR-0004 chose a hybrid representation — a B-rep solid produced and combined by
CSG-style feature operations — and explicitly allowed "a simplified faceted
B-rep (tessellated)" for v1, deferring an exact engine. What actually shipped
was less than that: `union` concatenated two meshes, and `subtract`/`intersect`
returned the first operand unchanged. They were honestly documented as
placeholders, but they were wired straight into the live feature-tree
evaluator, so every downstream consumer (renderer, slicer, STL/STEP export,
mass properties, simulation) was silently modelling geometry that a CAD user
would consider wrong. The same held for `fillet`/`chamfer`, which returned
their input untouched.

Forces in play:

- **Exact B-rep booleans are out of scope for v1.** A robust exact engine
  (curved surface/surface intersection, tolerant topology, face healing) is a
  multi-year effort or a heavyweight third-party dependency (OpenCascade),
  which conflicts with the deliberately dependency-light kernel of ADR-0003 and
  the pure-Rust/WASM target of ADR-0007.
- **The kernel is already a triangle-mesh kernel in practice.** `Solid` is a
  vertex pool plus triangle `Face`s. Rendering (ADR-0004), planar slicing
  (ADR-0008), STL export, contact/FEA meshing and mass properties all consume
  those triangles. Nothing downstream needs — or currently reads — exact
  surface geometry.
- **Something correct is needed *now*.** The workflows this project targets
  (design → slice → print, plus CAM/drawings) need a difference operation that
  actually removes material: pockets, holes, clearance cuts.
- **BSP CSG is a known-good fit.** The Naylor/Thibault/Amanatides BSP
  formulation (popularised as `csg.js`) is small, dependency-free, works
  directly on triangles, and produces closed, consistently oriented output for
  closed, consistently oriented input. It is the standard v1 answer for mesh
  booleans and is well within the maintenance budget of this crate.

The alternatives considered were: keep the placeholders (rejected — it makes
the whole downstream pipeline unsound); adopt an exact kernel dependency
(rejected for v1 — scope, licensing, WASM story); a marching-cubes/voxel or SDF
boolean (rejected — resolution-bound, destroys flat faces and sharp edges,
inflates triangle counts); a mesh-arrangement boolean à la libigl/Cork
(rejected for now — needs exact predicates and a much larger implementation to
beat BSP on the inputs we actually produce).

## Decision

Implement a BSP-tree boolean engine over triangle meshes in
`tpt-vertex-kernel/src/geometry/csg_bsp.rs`, and route the existing feature-tree
boolean and fillet/chamfer operations through it.

- Triangles are lifted into convex `Polygon`s carrying a supporting `Plane`.
  Splitting a convex polygon by a plane yields convex polygons, so the polygon
  (not the triangle) is the working primitive; results are fan-triangulated on
  the way out so `Solid` keeps storing plain 3-index triangle faces.
- A `BspTree` node owns a partition plane taken from the first polygon inserted
  into it, the polygons coincident with that plane, and front/back subtrees.
  The three primitives are `build` (partition/split), `clip_polygons`/`clip_to`
  (discard the parts of one solid that lie inside another) and `invert` (turn a
  solid inside out). Union, difference and intersection are the classic
  sequences of those primitives.
- The tree lives in an arena (`Vec<BspNode>`, children by index) and every
  traversal is iterative with an explicit work stack, so tree depth is bounded
  by heap rather than by the native call stack. The textbook recursive
  formulation stack-overflows on large or adversarially ordered meshes; a
  panicking kernel is not acceptable.
- Output vertices are welded through a spatial hash (with neighbour-cell
  probing, because the same intersection point computed from two adjacent
  polygons can differ by a few ULPs) and exactly-degenerate triangles are
  dropped.
- Operands with disjoint bounding boxes take a fast path: union concatenates,
  difference returns the target, intersection returns empty. This is both
  faster and lossless — the general path would needlessly re-split every face.
- `union`/`subtract`/`intersect` in `geometry/features.rs` keep their existing
  signatures and become thin wrappers over `bsp_union`/`bsp_subtract`/
  `bsp_intersect`, so `Feature::Boolean` evaluation gains real semantics with no
  API churn.
- Edge classification lands in `geometry/edges.rs`: derive the unique edges from
  the triangle soup (welded endpoints), and classify each by the dihedral angle
  between its adjacent faces as `Convex`, `Concave`, `Smooth` (within
  tolerance), `Boundary` (one adjacent face) or `NonManifold` (three or more).
- Fillet and chamfer are built on the boolean engine rather than on ad-hoc mesh
  surgery: each selected convex edge contributes one (chamfer) or `n` (fillet)
  cutting planes, and the solid is intersected with the corresponding
  half-spaces. The fillet planes are tangent to the inscribed rolling-ball
  cylinder — the line at distance `radius` from both adjacent face planes — so
  the result is a genuine faceted approximation of the rolling-ball surface
  rather than a cosmetic vertex nudge.

## Consequences

Positive:

- Booleans are real. `subtract` removes material, `intersect` returns the
  common material, `union` counts the overlap once. Every downstream consumer
  (slicer, exporters, mass properties, simulation) now sees geometry that
  matches the feature tree's intent.
- Fillet and chamfer produce genuinely modified solids on planar-faced parts,
  which unblocks the corresponding UI/feature-tree work.
- No new dependencies; pure `f64` Rust, so the engine builds for native, WASM
  and the FFI surface alike.
- Edge classification is reusable beyond rounding: selection UI, draft
  analysis, silhouette/drawing generation and mesh diagnostics all want it.

Negative / limits (accepted, and documented in the module headers):

- **Tessellation-dependent accuracy.** This is a mesh boolean. A cylinder is as
  round as its facets, and a boolean against one inherits that error. It is not
  a substitute for an exact B-rep boolean, and it does not preserve analytic
  surface identity — a face that was "a cylinder of radius r" becomes an
  unlabelled band of triangles.
- **Input contract.** Operands must be closed, manifold and consistently
  outward-oriented. Non-manifold, self-intersecting or inside-out input yields
  undefined (though non-panicking) output. There is currently no automatic
  repair pass in front of the engine.
- **Coplanar and near-degenerate faces.** Exactly coplanar overlapping faces
  are resolved by the coplanar-front/coplanar-back rule, which is correct for
  the common cases but can leave coincident faces on the shared boundary. Near-
  degenerate slivers are dropped on output, not repaired.
- **T-junctions.** BSP splitting is one-sided, so output meshes are
  geometrically closed (vector area `∮ n dA` is zero, which is what the
  integration tests assert) but not always combinatorially 2-manifold. Slicing
  and rendering tolerate this; a future edge-based watertightness pass should
  weld the T-junctions out.
- **Triangle-count growth.** Repeated booleans fragment faces. There is no
  coplanar-face merge/retriangulation pass yet; long feature histories will
  accumulate triangles.
- **Fillet/chamfer restrictions.** Plane cuts only remove material, so only
  convex, manifold edges are supported — concave edges (which need material
  added) and smooth/boundary edges are skipped. A plane cut is global, so a
  radius that is large relative to the feature will slice unrelated geometry;
  cuts that would empty the solid are skipped rather than applied. Fillets are
  circumscribed faceted approximations of the true arc, refined by raising the
  segment count. Variable-radius, face-to-face and corner-blend fillets are not
  supported.

Follow-up work:

- A watertightness/repair pass (T-junction welding, coplanar face merging,
  orientation fixing) in front of and behind the engine.
- Exact predicates or a snap-rounding scheme if coplanar robustness becomes a
  practical problem on real parts.
- Revisit an exact B-rep boolean when NURBS surfaces land; ADR-0004's migration
  boundary (`Solid`/`Face`/`Edge`) still contains that change, and this ADR does
  not close that door.
