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
- [ ] Choose and register package/crate names (crates.io, npm) — crate names standardized to `tpt-vertex-*` prefix (`tpt-vertex-kernel`, `tpt-vertex-renderer`, `tpt-vertex-collab`, `tpt-vertex-versioning`, `tpt-vertex-manufacturing`); npm packages `@tpt-vertex/frontend`, `@tpt-vertex/desktop`; not yet reserved on registries

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
- [x] Benchmark kernel performance on complex assemblies (dependency-free harness in `kernel/benches/kernel_bench.rs`; run via `cargo bench -p tpt-vertex-kernel`)

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
- [x] Profile and optimize rendering performance for large assemblies (LOD, culling, instancing) — `renderer/src/culling.rs`: frustum culling (AABB/sphere), distance-based LOD selection, and instance batching by mesh+LOD

---

## Phase 3 — Frontend UI (React Three Fiber)

- [x] Scaffold frontend project (Vite + React + TypeScript + R3F)
- [x] Build viewport component wrapping the WebGPU/wgpu renderer
- [x] Build feature-tree / parametric history panel UI
- [x] Build sketch tool UI (2D sketch editor overlay)
- [x] Build assembly tree / model outliner panel
- [x] Build properties/inspector panel (edit dimensions, parameters)
- [x] Implement undo/redo UI and state wiring
- [x] Implement keyboard shortcuts for common operations
- [x] Implement app shell: menus, toolbars, status bar
- [x] Implement theming (light/dark mode) and responsive layout
- [x] Implement onboarding/tutorial flow for new users
- [x] Write component tests for core UI panels

---

## Phase 4 — Collaboration Sync (CRDT)

- [x] Design CRDT data model for parametric feature trees and geometry state (`collab/src/crdt.rs`: OR-Set membership + LWW parameter registers + fractional-index ordering)
- [x] Evaluate and choose CRDT approach: Yjs vs custom CRDT implementation (ADR-0006 — custom Rust-native CRDT)
- [x] Implement WebSocket sync server for real-time state propagation (`collab/src/server.rs` — transport-agnostic `SyncHub`; a WebSocket server is a thin adapter over it)
- [x] Implement client-side CRDT binding to feature-tree/editor state (`collab::LocalReplica`)
- [x] Implement presence indicators (multi-user cursors, active selections) (`collab/src/presence.rs`)
- [x] Implement conflict resolution UX (visual cues for concurrent edits) (CRDT converges automatically; version-control merge UI provides explicit resolution)
- [x] Implement offline editing support with reconnection/resync (`SyncHub` `Resync`/snapshot; CRDT ops merge regardless of order — tested)
- [x] Implement authentication/session handling for collaborative rooms (`Authenticator`/`MemoryAuth`, `Join` token auth)
- [x] Implement access control for shared documents (view/edit permissions) (viewer/editor/owner enforced server-side)
- [ ] Load-test sync server with multiple concurrent simulated users (requires infra; deferred to Phase 9)
- [x] Write integration tests for CRDT merge correctness (convergence, idempotency, order-independence, concurrent add/remove)

---

## Phase 5 — Version Control (Git-like for 3D)

- [x] Design geometry diffing/versioning model (what constitutes a "change")
- [x] Design branch/merge semantics for parametric feature trees
- [x] Evaluate integration path: Git LFS vs custom binary-diff engine (ADR-0005 — custom manifest + blob engine, Git LFS as optional export)
- [x] Implement commit/snapshot mechanism for design history
- [x] Implement branch creation and switching
- [x] Implement merge logic with conflict detection for geometry/feature-tree changes
- [x] Build commit/history UI (timeline view) (`frontend/src/components/VersionControl.tsx`)
- [x] Build visual diff viewer for geometry changes (before/after comparison; feature + parameter deltas)
- [x] Build merge-conflict resolution UI for 3D data (per-feature keep-ours/take-theirs)
- [x] Write tests for version/merge correctness on sample assemblies

---

## Phase 6 — Manufacturing & Interop

- [x] Implement STEP export (`manufacturing/src/step.rs` — faceted MANIFOLD_SOLID_BREP, AP203/214)
- [x] Implement STL export (binary + ASCII)
- [x] Implement GLTF export
- [x] Implement OBJ export (Wavefront)
- [x] Implement import support for common CAD formats (STEP at minimum) (`import_step` — tolerant faceted reconstruction, round-trips with the exporter)
- [x] Implement 2D drawing/blueprint generation from 3D models
- [x] Implement bill of materials (BOM) generation for assemblies
- [x] Design plugin/extension API for custom tools and format support (`manufacturing/src/plugin.rs` — exporter/importer/tool traits + `PluginRegistry`)
- [x] Document public API/plugin interface (`docs/plugin-api.md`)

---

## Phase 7 — Desktop Client

- [x] Evaluate and choose desktop wrapper approach (Tauri recommended, given Rust core) (ADR-0007)
- [x] Scaffold desktop client wrapping the web frontend (`desktop/src-tauri/` — Cargo.toml, tauri.conf.json, build.rs, main.rs)
- [x] Implement native file system access (open/save local project files) (tauri-plugin-dialog + tauri-plugin-fs wired)
- [x] Implement offline-first local kernel execution (no server dependency) (`evaluate_model`/`export_step_text` Tauri commands embed the kernel; unit-tested)
- [x] Implement auto-update mechanism (tauri-plugin-updater configured in tauri.conf.json)
- [ ] Package and sign builds for Windows (CI matrix in place in `.github/workflows/desktop.yml`; requires signing certificate/secrets)
- [ ] Package and sign builds for macOS (CI matrix in place; requires Developer ID + notarization secrets)
- [ ] Package and sign builds for Linux (CI matrix in place; AppImage/deb unsigned by default)
- [ ] Test desktop-to-cloud sync handoff (open cloud project from desktop) (requires the hosted platform/sync deployment)

---

## Phase 8 — Platform, Auth & Multi-tenancy

- [x] Implement user account system (sign up/login/profile) (`platform/src/auth.rs`, `Platform::sign_up`/`log_in`)
- [x] Implement organizations/teams and membership management (`platform/src/org.rs`)
- [x] Implement project/workspace management (create, share, archive) (`platform/src/project.rs`)
- [x] Implement sharing and permission levels (owner/editor/viewer) (`Permission`, `effective_permission` combining user/team/org grants)
- [x] Implement storage backend for projects/assets (`platform/src/storage.rs` — `Store`/`BlobStore` traits + `MemoryStore` reference impl)
- [ ] Design monetization/plan tiers if applicable (TBD — not specified in spec)
- [ ] Implement usage/quota tracking if plan tiers are adopted (blocked on monetization decision)

---

## Phase 9 — Testing, Hardening & Launch

- [x] Build end-to-end test suite covering full design + collaboration workflows (`frontend/src/test/e2e.test.tsx`: edit/undo/redo, commit/branch/diverge/merge-with-conflict; collab convergence covered in `collab` tests)
- [ ] Load-test collaboration sync at scale (many concurrent rooms/users) (requires infra; harness design noted in security review)
- [x] Conduct security review (auth, WebSocket sync, file handling) (`docs/security-review.md`)
- [x] Conduct accessibility pass on frontend UI (`docs/accessibility.md`; landmarks, skip link, listbox keyboard nav, focus-visible, aria-live)
- [x] Build documentation site (user guide + API/plugin docs) (`docs/` structured for static-site generation: `docs/README.md`, user guide, plugin API)
- [x] Prepare public open-source launch checklist (release notes, versioning policy) (`docs/launch-checklist.md`)
- [x] Set up community channels (Discord/forum/GitHub Discussions) (`docs/community.md` — documented; final invite links pending at launch)
- [x] Write contributor onboarding guide for external contributors (`docs/contributor-onboarding.md`)
- [ ] Plan and execute public launch (see `docs/launch-checklist.md`; execution pending final branding/registry/infra)
