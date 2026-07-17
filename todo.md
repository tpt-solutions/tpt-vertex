# TPT Vertex — Project Todo

Parametric 3D CAD platform ("Figma for Hardware") by TPT Solutions.
License: dual **MIT OR Apache-2.0**.

---

## Phase 0 — Foundation & Project Setup

- [ ] Initialize git repository
- [ ] Set up monorepo structure (`/kernel` Rust workspace, `/frontend` JS/TS package, `/desktop` client)
- [ ] Add `LICENSE-MIT` and `LICENSE-APACHE` files
- [ ] Add SPDX `MIT OR Apache-2.0` license headers/expressions to `Cargo.toml` and `package.json`
- [ ] Write root `README.md` (project pitch, architecture overview, build instructions)
- [ ] Set up `.gitignore` for Rust + Node + build artifacts
- [ ] Set up CI pipeline: Rust build/test/clippy/fmt
- [ ] Set up CI pipeline: frontend build/test/lint (ESLint/Prettier)
- [ ] Add issue templates and PR template
- [ ] Write `CONTRIBUTING.md` (dev setup, coding standards, PR process)
- [ ] Write `CODE_OF_CONDUCT.md`
- [ ] Confirm branding: project name, logo, domain/URL
- [ ] Set up architecture decision record (ADR) folder/process
- [ ] Choose and register package/crate names (crates.io, npm)

---

## Phase 1 — Geometry Kernel (Rust)

- [ ] Define core math primitives (vectors, matrices, transforms, quaternions)
- [ ] Implement tolerancing/precision handling for floating-point geometry
- [ ] Decide geometric representation: B-rep vs CSG (or hybrid) and document rationale (ADR)
- [ ] Implement 2D sketch primitives (lines, arcs, circles, splines)
- [ ] Build 2D sketch constraint solver (coincident, parallel, perpendicular, dimensional, etc.)
- [ ] Design parametric feature tree data structure (dependency graph of operations)
- [ ] Implement core features: extrude, revolve, sweep, loft
- [ ] Implement boolean operations (union, subtract, intersect)
- [ ] Implement fillet/chamfer operations
- [ ] Implement feature-tree evaluation/rebuild engine (recompute on parameter change)
- [ ] Implement assembly/mating structure (multi-part positioning, joints/constraints)
- [ ] Write unit tests for kernel math and constraint solver
- [ ] Write integration tests for feature-tree rebuild correctness
- [ ] Add WASM build target for kernel (browser use via wasm-bindgen)
- [ ] Define FFI/bindings boundary for native desktop use
- [ ] Benchmark kernel performance on complex assemblies

---

## Phase 2 — Rendering Engine (WebGPU / wgpu)

- [ ] Set up `wgpu` renderer core (device/surface/pipeline setup)
- [ ] Implement tessellation of B-rep/CSG geometry into render meshes
- [ ] Implement scene graph (nodes, transforms, hierarchy)
- [ ] Implement camera system (orbit, pan, zoom, perspective/orthographic toggle)
- [ ] Implement materials/shading (PBR basics, wireframe mode, section views)
- [ ] Implement lighting setup (default studio lighting, shadows)
- [ ] Implement object picking/selection (ray casting against geometry)
- [ ] Implement highlight/hover feedback for selected geometry
- [ ] Integrate renderer into browser via `wasm-bindgen` + WebGPU
- [ ] Handle WebGPU feature-detection/fallback (e.g. warn on unsupported browsers)
- [ ] Profile and optimize rendering performance for large assemblies (LOD, culling, instancing)

---

## Phase 3 — Frontend UI (React Three Fiber)

- [ ] Scaffold frontend project (Vite + React + TypeScript + R3F)
- [ ] Build viewport component wrapping the WebGPU/wgpu renderer
- [ ] Build feature-tree / parametric history panel UI
- [ ] Build sketch tool UI (2D sketch editor overlay)
- [ ] Build assembly tree / model outliner panel
- [ ] Build properties/inspector panel (edit dimensions, parameters)
- [ ] Implement undo/redo UI and state wiring
- [ ] Implement keyboard shortcuts for common operations
- [ ] Implement app shell: menus, toolbars, status bar
- [ ] Implement theming (light/dark mode) and responsive layout
- [ ] Implement onboarding/tutorial flow for new users
- [ ] Write component tests for core UI panels

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
