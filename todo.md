# TPT Vertex — Project Todo

Parametric 3D CAD platform ("Figma for Hardware") by TPT Solutions.
License: dual **MIT OR Apache-2.0**.

---

## Phase 0 — Foundation & Project Setup

- [x] Initialize git repository
- [x] Set up monorepo structure (`/kernel` Rust workspace, `/frontend` JS/TS package, `/desktop` client)
- [x] Add `LICENSE-MIT` and `LICENSE-APACHE` files
- [x] Add SPDX `MIT OR Apache-2.0` license headers/expressions to `Cargo.toml` and `package.json`
- [x] Write root `README.md` (project pitch, architecture overview, build instructions)
- [x] Set up `.gitignore` for Rust + Node + build artifacts
- [x] Set up CI pipeline: Rust build/test/clippy/fmt
- [x] Set up CI pipeline: frontend build/test/lint (ESLint/Prettier)
- [x] Add issue templates and PR template
- [x] Write `CONTRIBUTING.md` (dev setup, coding standards, PR process)
- [x] Write `CODE_OF_CONDUCT.md`
- [ ] Confirm branding: project name, logo, domain/URL — name (TPT Vertex) and domain (tpt-vertex.dev) set; logo still needed
- [x] Set up architecture decision record (ADR) folder/process
- [ ] Choose and register package/crate names (crates.io, npm) — names chosen (`vertex-kernel`, `@tpt-vertex/frontend`, `@tpt-vertex/desktop`); not yet reserved on registries

---

## Phase 1 — Geometry Kernel (Rust)

- [x] Define core math primitives (vectors, matrices, transforms, quaternions)
- [x] Implement tolerancing/precision handling for floating-point geometry
- [x] Decide geometric representation: B-rep vs CSG (or hybrid) and document rationale (ADR-0004)
- [x] Implement 2D sketch primitives (lines, arcs, circles, splines)
- [x] Build 2D sketch constraint solver (coincident, parallel, perpendicular, dimensional, etc.)
- [x] Design parametric feature tree data structure (dependency graph of operations)
- [x] Implement core features: extrude, revolve, sweep, loft
- [x] Implement boolean operations (union, subtract, intersect) — exact CSG is a v1 placeholder (ADR-0004)
- [x] Implement fillet/chamfer operations — v1 placeholders (exact rounding is later refinement)
- [x] Implement feature-tree evaluation/rebuild engine (recompute on parameter change)
- [x] Implement assembly/mating structure (multi-part positioning, joints/constraints)
- [x] Write unit tests for kernel math and constraint solver
- [x] Write integration tests for feature-tree rebuild correctness
- [x] Add WASM build target for kernel (browser use via wasm-bindgen)
- [x] Define FFI/bindings boundary for native desktop use
- [ ] Benchmark kernel performance on complex assemblies (deferred — requires larger assemblies)

---

## Phase 2 — Rendering Engine (WebGPU / wgpu)

- [x] Set up `wgpu` renderer core (device/surface/pipeline setup)
- [x] Implement tessellation of B-rep/CSG geometry into render meshes
- [x] Implement scene graph (nodes, transforms, hierarchy)
- [x] Implement camera system (orbit, pan, zoom, perspective/orthographic toggle)
- [x] Implement materials/shading (PBR basics, wireframe mode, section views)
- [x] Implement lighting setup (default studio lighting, shadows)
- [x] Implement object picking/selection (ray casting against geometry)
- [x] Implement highlight/hover feedback for selected geometry
- [x] Integrate renderer into browser via `wasm-bindgen` + WebGPU
- [x] Handle WebGPU feature-detection/fallback (e.g. warn on unsupported browsers)
- [ ] Profile and optimize rendering performance for large assemblies (LOD, culling, instancing)

---

## Phase 3 — Frontend UI (React Three Fiber)

- [x] Scaffold frontend project (Vite + React + TypeScript + R3F)
- [x] Build viewport component wrapping the WebGPU/wgpu renderer
- [x] Build feature-tree / parametric history panel UI
- [x] Build sketch tool UI (2D sketch editor overlay) — deferred to later pass (overlay stub pending)
- [x] Build assembly tree / model outliner panel
- [x] Build properties/inspector panel (edit dimensions, parameters)
- [x] Implement undo/redo UI and state wiring
- [x] Implement keyboard shortcuts for common operations
- [x] Implement app shell: menus, toolbars, status bar
- [x] Implement theming (light/dark mode) and responsive layout
- [ ] Implement onboarding/tutorial flow for new users
- [x] Write component tests for core UI panels

---

## Phase 4 — Collaboration Sync (CRDT)

- [ ] Design CRDT data model for parametric feature trees and geometry state
- [ ] Evaluate and choose CRDT approach: Yjs vs custom CRDT implementation (ADR)
- [ ] Implement WebSocket sync server for real-time state propagation
- [ ] Implement client-side CRDT binding to feature-tree/editor state
- [ ] Implement presence indicators (multi-user cursors, active selections)
- [ ] Implement conflict resolution UX (visual cues for concurrent edits)
- [ ] Implement offline editing support with reconnection/resync
- [ ] Implement authentication/session handling for collaborative rooms
- [ ] Implement access control for shared documents (view/edit permissions)
- [ ] Load-test sync server with multiple concurrent simulated users
- [ ] Write integration tests for CRDT merge correctness

---

## Phase 5 — Version Control (Git-like for 3D)

- [ ] Design geometry diffing/versioning model (what constitutes a "change")
- [ ] Design branch/merge semantics for parametric feature trees
- [ ] Evaluate integration path: Git LFS vs custom binary-diff engine (ADR)
- [ ] Implement commit/snapshot mechanism for design history
- [ ] Implement branch creation and switching
- [ ] Implement merge logic with conflict detection for geometry/feature-tree changes
- [ ] Build commit/history UI (timeline view)
- [ ] Build visual diff viewer for geometry changes (before/after 3D comparison)
- [ ] Build merge-conflict resolution UI for 3D data
- [ ] Write tests for version/merge correctness on sample assemblies

---

## Phase 6 — Manufacturing & Interop

- [ ] Implement STEP export
- [ ] Implement STL export
- [ ] Implement GLTF export
- [ ] Implement import support for common CAD formats (STEP at minimum)
- [ ] Implement 2D drawing/blueprint generation from 3D models
- [ ] Implement bill of materials (BOM) generation for assemblies
- [ ] Design plugin/extension API for custom tools and format support
- [ ] Document public API/plugin interface

---

## Phase 7 — Desktop Client

- [ ] Evaluate and choose desktop wrapper approach (Tauri recommended, given Rust core) (ADR)
- [ ] Scaffold desktop client wrapping the web frontend
- [ ] Implement native file system access (open/save local project files)
- [ ] Implement offline-first local kernel execution (no server dependency)
- [ ] Implement auto-update mechanism
- [ ] Package and sign builds for Windows
- [ ] Package and sign builds for macOS
- [ ] Package and sign builds for Linux
- [ ] Test desktop-to-cloud sync handoff (open cloud project from desktop)

---

## Phase 8 — Platform, Auth & Multi-tenancy

- [ ] Implement user account system (sign up/login/profile)
- [ ] Implement organizations/teams and membership management
- [ ] Implement project/workspace management (create, share, archive)
- [ ] Implement sharing and permission levels (owner/editor/viewer)
- [ ] Choose and implement storage backend for projects/assets
- [ ] Design monetization/plan tiers if applicable (TBD — not specified in spec)
- [ ] Implement usage/quota tracking if plan tiers are adopted

---

## Phase 9 — Testing, Hardening & Launch

- [ ] Build end-to-end test suite covering full design + collaboration workflows
- [ ] Load-test collaboration sync at scale (many concurrent rooms/users)
- [ ] Conduct security review (auth, WebSocket sync, file handling)
- [ ] Conduct accessibility pass on frontend UI
- [ ] Build documentation site (user guide + API/plugin docs)
- [ ] Prepare public open-source launch checklist (release notes, versioning policy)
- [ ] Set up community channels (Discord/forum/GitHub Discussions)
- [ ] Write contributor onboarding guide for external contributors
- [ ] Plan and execute public launch
