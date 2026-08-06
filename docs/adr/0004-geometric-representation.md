# ADR-0004: Geometric representation — hybrid B-rep with CSG feature ops

- Status: Accepted (amended 2026-08-06 — see the note below and
  [ADR-0013](0013-bsp-boolean-engine.md))
- Date: 2026-07-18

> **Status note (2026-08-06).** Exact B-rep booleans remain **deferred**. The
> "simplified faceted B-rep is acceptable initially" allowance in the
> Consequences section below is now realised concretely: the v1 boolean path is
> the BSP triangle-mesh boolean engine specified in
> [ADR-0013](0013-bsp-boolean-engine.md) (`geometry/csg_bsp.rs`), which also
> backs the v1 fillet/chamfer (`geometry/edges.rs`). Accuracy is therefore
> tessellation-dependent and the engine is explicitly *not* a substitute for an
> exact B-rep boolean; ADR-0013 records the known limits (coplanar and
> near-degenerate faces, non-manifold inputs, T-junctions, triangle-count
> growth). This ADR's decision is unchanged — the feature tree is still the
> source of truth and the migration boundary is still the
> `Solid`/`Face`/`Edge` API — and a future exact/NURBS kernel supersedes only
> the boolean engine, not this representation choice.

## Context

The kernel must represent solid parts for parametric modeling, real-time
rendering, boolean/fillet operations, and manufacturing export (STEP/STL/GLTF).
Two classic representations compete:

- **B-rep (boundary representation):** stores faces/edges/vertices explicitly.
  Compact, exact, ideal for rendering, fillets, and STEP export, but boolean
  operations and feature edits require robust topology kernels.
- **CSG (constructive solid geometry):** stores a tree of primitive solids
  combined with boolean operators. Trivial to evaluate and great for
  parametric "feature" semantics, but poor for direct editing, fillets, and
  downstream manufacturing data.

We also need the representation to map cleanly onto a **parametric feature
tree** (extrude, revolve, boolean, fillet…) where each feature is a node that
consumes a solid and produces a new solid.

## Decision

TPT Vertex uses a **hybrid**: a B-rep solid as the persistent runtime
representation, produced and combined via CSG-style feature operations.

- The runtime solid is a B-rep: a `Solid` owns `Face`s, each `Face` owns
  `Edge`s/`Vertex`s, with a half-edge topology for watertight shells.
- Feature operations (extrude, revolve, sweep, loft) generate B-rep solids from
  sketches.
- Boolean operations (union/subtract/intersect) are expressed in the feature
  tree as CSG combinators but resolved into B-rep via the kernel's boolean
  engine.
- The feature tree is the *source of truth*; the B-rep is a *derived, cached
  evaluation result* that is recomputed on parameter change.

Rationale: B-rep gives us exact manufacturing output and efficient rendering;
keeping features as a CSG-like tree preserves parametric editability and makes
the rebuild/versioning story tractable.

## Consequences

- We must implement a robust boolean/B-rep engine (Phase 1/2). Initially a
  simplified faceted B-rep (tessellated) is acceptable; exact NURBS B-rep is a
  later refinement. *(Realised as the BSP triangle-mesh boolean engine of
  [ADR-0013](0013-bsp-boolean-engine.md); exact B-rep booleans stay deferred.)*
- Feature editability and collaboration/versioning operate on the feature tree,
  not the mesh — merges stay meaningful.
- Rendering consumes tessellated B-rep; the renderer need not understand
  feature semantics.
- A future migration to a fully exact kernel (e.g. OpenCascade-style) is
  contained behind the `Solid`/`Face`/`Edge` API.
