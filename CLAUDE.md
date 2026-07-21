# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

TPT Vertex is a parametric 3D CAD platform ("Figma for Hardware"): a Rust
geometry kernel + WebGPU renderer, a React Three Fiber web frontend, real-time
CRDT collaboration, git-like version control over feature trees, an FDM
slicer, FEA/motion simulation, and network printer connectivity — wrapped in a
Tauri desktop client.

## Commands

### Rust workspace (kernel, renderer, manufacturing, versioning, collab, platform, slicer, simulation, printer-link)

```sh
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

Run a single crate's tests, or a single test by name:

```sh
cargo test -p tpt-vertex-kernel
cargo test -p tpt-vertex-slicer infill::tests::some_test_name
```

`tpt-vertex-kernel` also has a dependency-free benchmark harness:

```sh
cargo bench -p tpt-vertex-kernel
```

`frontend` and `desktop` are **excluded** from the root Cargo workspace (see
root `Cargo.toml`) so they build independently; `desktop` still depends on the
kernel crates by relative path.

### Frontend (React Three Fiber / Vite / TypeScript)

```sh
cd frontend
npm install
npm run dev      # local dev server
npm run build    # tsc + vite build
npm run lint     # eslint
npm run test     # vitest run
npm run test:watch
```

### Desktop (Tauri)

```sh
cd desktop
npm install
npm run tauri dev
```

## Architecture

```
        Frontend (R3F): viewport, sketch editor, feature tree, inspector, presence
                 |                          |
         WebGPU/wgpu render mesh     CRDT sync (WebSocket)
                 |                          |
              Geometry Kernel (Rust): math, sketch, solver, feature tree, boolean/fillet, assembly
```

- **`tpt-vertex-kernel`** is the single source of truth for geometry. Everything
  else (renderer, manufacturing, versioning, collab, slicer, simulation)
  depends on it by path and consumes its types — never duplicate geometry
  logic in a downstream crate. It compiles three ways: plain `rlib`/`cdylib`
  for native/tests, `wasm` feature (`src/wasm.rs`) for the browser, and `ffi`
  feature (`src/ffi.rs`) for a C ABI surface consumed by the desktop app.
- **Feature tree (`tpt-vertex-kernel/src/feature_tree.rs`)**: a model is a DAG
  of `Feature` nodes (extrude, revolve, sweep, loft, boolean ops, fillet,
  chamfer, etc.). The `Evaluator` topologically sorts and evaluates the graph,
  caching results and only re-running the subgraph downstream of a changed
  parameter. Version control, CRDT collaboration, and undo/redo all operate on
  this parametric tree, not on raw meshes, so merges/diffs stay meaningful —
  don't bypass it by mutating solids directly.
- **`renderer`** tessellates kernel geometry and draws it via `wgpu`; the
  `web` feature pulls in wasm-bindgen/web-sys glue for the browser host.
- **`versioning`** and **`collab`** both operate on feature-tree manifests
  (not meshes): `versioning` is a git-like commit/branch/merge/diff engine,
  `collab` is a custom CRDT + sync hub for concurrent editing. Keep changes to
  the feature-tree data model in sync across kernel, versioning, and collab.
- **`manufacturing`** handles export/import (STL, OBJ, glTF, STEP), BOM
  generation, drawings, and a plugin registry — consumes kernel `Solid`s.
- **`tpt-vertex-slicer`** (ADR-0008) takes kernel geometry through
  `manufacturing` and produces FDM G-code: planar layering, wall/infill
  generation (including adaptive layers, variable width, bridging, seam
  placement, support), polygon offsetting (see ADR-0011 for the offset library
  evaluation), and presets/plugins.
- **`tpt-vertex-simulation`** (ADR-0009) is deliberately scoped to linear
  static-stress FEA plus assembly motion/kinematics, isolated from the main
  solver stack. The global linear solve currently uses a self-contained dense
  LU (`src/solve.rs`) rather than pulling in `faer`, specifically to keep the
  crate dependency-free and auditable for v1 — swapping in `faer` is meant to
  be a contained change inside that one file; don't spread solver-backend
  assumptions elsewhere.
- **`tpt-vertex-printer-link`** (ADR-0010) is a unified client for
  ESP3D/OctoPrint (+ Moonraker-compatible) network printers, used for
  uploading slicer output and tracking print job status. It's TLS-optional by
  default (`tls` feature enables `reqwest/rustls-tls`) so LAN-only usage stays
  lightweight.
- **`platform`** provides accounts, orgs/teams, projects, sharing/permissions,
  and a pluggable storage backend — the only crate with no kernel dependency.
- **`desktop`** (Tauri, ADR-0007) embeds the kernel, manufacturing, slicer,
  simulation, and printer-link crates directly by path for offline-first local
  evaluation with no server dependency, and exposes them to the WebView via
  Tauri commands (`desktop/src-tauri/src/main.rs`).

## Conventions

- New source files carry an SPDX header: `// SPDX-License-Identifier: MIT OR Apache-2.0` (Rust) or the TS equivalent.
- Significant architectural decisions are recorded as ADRs under
  [`docs/adr/`](docs/adr/) (see `docs/adr/0001-record-architecture-decisions.md`
  for the process, `docs/adr/template.md` for the format, and the index in
  `docs/adr/README.md`). If a change involves a notable trade-off, add an ADR
  rather than only leaving comments.
- TypeScript uses strict mode; avoid `any` without a documented reason.
