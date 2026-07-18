# ADR-0004: Geometric representation — hybrid B-rep with CSG feature ops

- Status: Accepted
- Date: 2026-07-18

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
  later refinement.
- Feature editability and collaboration/versioning operate on the feature tree,
  not the mesh — merges stay meaningful.
- Rendering consumes tessellated B-rep; the renderer need not understand
  feature semantics.
- A future migration to a fully exact kernel (e.g. OpenCascade-style) is
  contained behind the `Solid`/`Face`/`Edge` API.
