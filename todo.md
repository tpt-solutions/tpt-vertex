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
- [ ] Choose and register package/crate names (crates.io, npm) — crate names standardized to `tpt-vertex-*` prefix (`tpt-vertex-kernel`, `tpt-vertex-renderer`, `tpt-vertex-collab`, `tpt-vertex-versioning`, `tpt-vertex-manufacturing`, `tpt-vertex-slicer`, `tpt-vertex-simulation`); npm packages `@tpt-vertex/frontend`, `@tpt-vertex/desktop`; not yet reserved on registries
- [x] Align on-disk directory names with crate names for the standalone-publishable crates (`kernel/` → `tpt-vertex-kernel/`; new `tpt-vertex-slicer/`, `tpt-vertex-simulation/` scaffolds); `tpt-vertex-kernel`/`-slicer`/`-simulation` Cargo.toml metadata (readme, keywords, categories, versioned path deps) is publish-ready — actual `cargo login`/`cargo publish` still needs to be run manually by a repo owner

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
- [ ] Load-test sync server with multiple concurrent simulated users (requires infra; deferred to Phase 8)
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

## Phase 8 — Testing, Hardening & Launch

- [x] Build end-to-end test suite covering full design + collaboration workflows (`frontend/src/test/e2e.test.tsx`: edit/undo/redo, commit/branch/diverge/merge-with-conflict; collab convergence covered in `collab` tests)
- [ ] Load-test collaboration sync at scale (many concurrent rooms/users) (requires infra; harness design noted in security review)
- [x] Conduct security review (WebSocket sync, file handling) (`docs/security-review.md`)
- [x] Conduct accessibility pass on frontend UI (`docs/accessibility.md`; landmarks, skip link, listbox keyboard nav, focus-visible, aria-live)
- [x] Build documentation site (user guide + API/plugin docs) (`docs/` structured for static-site generation: `docs/README.md`, user guide, plugin API)
- [x] Prepare public open-source launch checklist (release notes, versioning policy) (`docs/launch-checklist.md`)
- [x] Set up community channels (Discord/forum/GitHub Discussions) (`docs/community.md` — documented; final invite links pending at launch)
- [x] Write contributor onboarding guide for external contributors (`docs/contributor-onboarding.md`)
- [ ] Plan and execute public launch (see `docs/launch-checklist.md`; execution pending final branding/registry/infra)

---

## Phase 9 — 3D Printing / Slicing

- [x] Scaffold `slicer/` crate (`tpt-vertex-slicer`), add to Cargo workspace members
- [x] Define `PrinterProfile`/`SliceSettings` config structs (`slicer/src/profile.rs`)
- [x] Implement planar layering: triangle/plane intersection + segment stitching into closed loops (`slicer/src/layers.rs`)
- [x] Implement polygon offset/inset for perimeter/wall generation (`slicer/src/offset.rs`)
- [x] Implement basic rectilinear/zigzag infill generation (`slicer/src/infill.rs`)
- [x] Implement toolpath ordering (perimeters, infill, travel/retraction) (`slicer/src/path.rs`)
- [x] Implement G-code emission for a generic configurable FDM printer profile (`slicer/src/gcode.rs`)
- [x] Add `MaterialCalibration` profile (flow ratio/temp offset/cooling curve) for per-material/spool tuning
- [x] Add structural/non-structural body tagging with per-region wall/infill overrides
- [x] Spec/verify highest-risk geometric kernels (plane intersection, contour stitching, polygon offset) via `tpt-telos`
- [x] Write unit tests (plane-intersection on cubes/cylinders, offset correctness, infill coverage, G-code smoke tests)
- [x] Write crate-level end-to-end slice test
- [x] Add desktop Tauri `slice_model` command for local/offline slicing
- [x] Build minimal slicer settings + layer-preview panel in frontend (`SlicerPanel.tsx`)
- [x] Write ADR: slicing architecture (standalone crate vs. plugin trait; hand-rolled offset vs. external dependency)
- [x] (Fast-follow) Implement support structure generation (basic overhang-triggered supports) (`tpt-vertex-slicer/src/support.rs` — grid/pillar supports from overhang detection)
- [ ] (Fast-follow) Implement tree/organic supports
- [x] (Fast-follow) Implement adaptive layer height (`tpt-vertex-slicer/src/adaptive.rs` — slope-driven cusp-bounded layer height)
- [x] (Fast-follow) Implement bridging detection and bridge-specific speed/cooling (`tpt-vertex-slicer/src/bridging.rs`; speed/fan wired in `gcode.rs`)
- [x] (Fast-follow) Implement multi-material/multi-extruder toolpath support (`RegionTag`/`ExtruderProfile` in `profile.rs`; `T{n}` tool-change + XY offset in `gcode.rs`)
- [x] (Fast-follow) Implement Arachne-style variable-width perimeters and adaptive infill density (`tpt-vertex-slicer/src/variable_width.rs` — basic uniform-width thin-wall fill, not true per-point Arachne; adaptive infill density via the `stress` hook below)
- [x] (Fast-follow) Implement seam placement optimization (`tpt-vertex-slicer/src/seam.rs`)
- [x] (Fast-follow) Implement mesh repair/manifold-checking pass before slicing (`tpt-vertex-slicer/src/repair.rs`)
- [x] (Fast-follow) Evaluate robust polygon-offset library integration (ADR-0011 — keep hand-rolled offset; `i_overlay` recommended if/when true multi-hole/Arachne offset is needed)
- [ ] (Fast-follow) Calibrate print-time/filament-usage estimation against real printer data
- [x] (Fast-follow) Support importing/exporting printer-profile presets (Marlin/Klipper) (`tpt-vertex-slicer/src/presets.rs` — Klipper-style `printer.cfg` import/export)
- [ ] (Fast-follow) Validate G-code against real hardware / a G-code simulator
- [ ] (Fast-follow) Closed-loop hardware feedback (filament-width sensors, chamber thermistors) — needs its own firmware-integration design pass/ADR
- [x] (Fast-follow) Simulation-driven adaptive infill using Phase 10's stress field output (`slice_solid_to_gcode`'s `stress: Option<&dyn Fn(f64,f64,f64)->f64>` hook — caller supplies a normalized von Mises field from `tpt-vertex-simulation`; slicing has no opinion on loads/BCs)
- [ ] (Fast-follow) Feature-tree-native slicing (slice directly from `FeatureTree`/CRDT state for live collaborative preview)
- [x] (Fast-follow) Expose slicing as an `ExporterPlugin` adapter for consistency with STL/OBJ/STEP (`tpt-vertex-slicer/src/plugin.rs` — `GcodeExporterPlugin`)

---

## Phase 10 — Simulation (Static FEA + Assembly Motion)

- [x] Write ADR: simulation scope and solver dependency decision
- [x] Add `Material` struct (density, Young's modulus, Poisson's ratio, yield strength) to kernel; attach to `Part` (`kernel/src/material.rs`, `kernel/src/assembly.rs`)
- [x] Fold `manufacturing/src/bom.rs` density table into the new kernel `Material` table
- [x] Add DOF-bearing `Mate` variants (`Revolute`, `Slider`, `Cylindrical`) with axis/angle/offset/limits (`kernel/src/assembly.rs`)
- [x] Implement real rotation solving in `apply_mate` (fixes existing `AxisAligned` no-op stub)
- [x] Scaffold `simulation/` crate (`tpt-vertex-simulation`), add to Cargo workspace members
- [x] Implement `validate_watertight` precondition check and tetrahedralization (`simulation/src/mesh.rs`)
- [x] Implement isotropic elasticity (stress-strain) matrix from material properties (`simulation/src/material.rs`)
- [x] Implement boundary condition representation: fixed constraints, point/surface loads (`simulation/src/bc.rs`)
- [x] Implement linear 4-node tetrahedron element stiffness matrix (`simulation/src/element.rs`)
- [x] Implement global sparse stiffness assembly + boundary condition application (`simulation/src/assembly.rs`)
- [x] Adopt sparse linear solver dependency (recommend `faer`, contained to `simulation/` only) (`simulation/src/solve.rs`)
- [x] Implement stress/strain post-processing and von Mises scalar + surface interpolation (`simulation/src/post.rs`)
- [x] Implement `Motion`/`MotionPlayer` for time/parameter-driven mate playback (`simulation/src/motion.rs`)
- [x] Spec/verify stiffness assembly and boundary-condition application via `tpt-telos`
- [x] Write analytical validation tests: cantilever beam deflection vs. Euler-Bernoulli
- [x] Write analytical validation tests: axial bar stress/elongation vs. closed form
- [x] Write analytical validation tests: plate-with-hole stress concentration vs. Kirsch solution
- [x] Write motion validation tests (driven-angle rotation vs. quaternion math)
- [x] Add desktop Tauri `run_static_analysis`/`run_motion_frame` commands
- [x] Build load/constraint picker UI (`SimulationSetup.tsx`)
- [x] Build stress-color-mapped results viewer (reusing existing WebGPU/PBR rendering)
- [x] Build motion-study timeline/playback UI (`MotionStudy.tsx`)
- [x] (Fast-follow) Nonlinear material models (plasticity, hyperelasticity) (`tpt-vertex-simulation/src/plasticity.rs` — J2 radial return with Perfect/Linear/Swift/Hollomon hardening laws + consistent tangent)
- [x] (Fast-follow) Large-deformation/geometric nonlinearity (`tpt-vertex-simulation/src/nonlinear.rs` — Total Lagrangian Newton-Raphson with material + geometric tangent stiffness)
- [x] (Fast-follow) Contact/interference detection during motion (`tpt-vertex-simulation/src/contact.rs` — AABB broad phase + Möller triangle-triangle narrow phase)
- [x] (Fast-follow) Full rigid-body dynamics (mass, inertia, forces/torques, time integration, joint reaction forces) (`tpt-vertex-simulation/src/dynamics.rs` — semi-implicit Euler free body + RK4 revolute joint with reaction forces)
- [x] (Fast-follow) Thermal analysis (steady-state/transient, thermal stress) (`tpt-vertex-simulation/src/thermal.rs` — steady-state conduction + thermal-stress coupled FEA; transient is a documented fast-follow)
- [x] (Fast-follow) Fatigue/lifetime analysis (`tpt-vertex-simulation/src/fatigue.rs` — S-N curves, Goodman/Gerber mean-stress correction, Miner's cumulative damage)
- [x] (Fast-follow) Modal/frequency analysis (`tpt-vertex-simulation/src/modal.rs` — lumped-mass Jacobi eigensolver for K x = ω² M x)
- [x] (Fast-follow) Buckling analysis (`tpt-vertex-simulation/src/buckling.rs` — geometric stiffness matrix, eigenvalue buckling via K⁻¹G)
- [x] (Fast-follow) Higher-order/quadratic tetrahedral elements (`tpt-vertex-simulation/src/quadratic_tet.rs` — 10-node quad tet with 4-point Gauss quadrature)
- [x] (Fast-follow) Adaptive mesh refinement (`tpt-vertex-simulation/src/adaptivity.rs` — ZZ error estimator, longest-edge bisection, adaptive loop)
- [x] (Fast-follow) Multi-part/assembly-level contact-coupled static FEA (`tpt-vertex-simulation/src/contact_fea.rs` — node-to-surface penalty contact assembled into the Newton-Raphson tangent from `nonlinear`)
- [x] (Fast-follow) In-browser/wasm simulation execution (`tpt-vertex-simulation/src/wasm_api.rs` + `wasm` crate feature that drops the `faer`/rayon dependency in favor of the dense LU solver for `wasm32-unknown-unknown`)
- [x] (Fast-follow) Optimization/topology-optimization studies driven by simulation results (`tpt-vertex-simulation/src/topo_opt.rs` — SIMP method with OC update rule)

---

## Phase 11 — Other SolidWorks-class Functionality (future prioritization)

- [ ] Sheet metal module (flat-pattern unfolding, bend allowances, bend-order sequencing)
- [ ] CAM: toolpath generation for CNC milling/turning, post-processors
- [ ] GD&T/tolerance annotations on drawings
- [ ] Design tables/configurations (same model, multiple parameter sets)
- [ ] Photorealistic rendering material presets (SolidWorks Visualize-style)

---

## Phase 12 — Printer Connectivity (Network Printing)

- [x] Write ADR: printer connectivity architecture — unified ESP3D/OctoPrint client, MVP scope (`docs/adr/0010-printer-connectivity.md`)
- [x] Scaffold `tpt-vertex-printer-link` crate, add to Cargo workspace members
- [x] Define `PrinterTarget`/`ProtocolKind` connection-config types, distinct from `tpt-vertex-slicer`'s physical `PrinterProfile` (`tpt-vertex-printer-link/src/target.rs`)
- [x] Define `PrinterClient` trait, `StatusSnapshot`, `ConnectionInfo`, `PrinterError`, `make_client` factory (`tpt-vertex-printer-link/src/client.rs`)
- [x] Implement ESP3D HTTP client — upload + M115/M105/M27 command polling + M23/M24 print start (`tpt-vertex-printer-link/src/esp3d.rs`)
- [x] Implement OctoPrint/Moonraker (`octoprint_compat`)-compatible REST client (`tpt-vertex-printer-link/src/octoprint.rs`)
- [x] Write unit tests for both clients against a mock HTTP server (mockito), covering success/error/malformed-reply paths
- [x] Add `tauri-plugin-store` and printer profile persistence (`printers.json` in app config dir) (`desktop/src-tauri/src/printer.rs`)
- [x] Add desktop Tauri printer commands: list/save/delete profile, test connection, send G-code, get status (`desktop/src-tauri/src/printer.rs`)
- [x] Add frontend Tauri IPC wrapper (`frontend/src/printer/client.ts`) — first real `@tauri-apps/api` usage in the app
- [x] Build printer profile management panel (`PrinterPanel.tsx`)
- [x] Add "Send to Printer" action + connect/upload/print status feedback to `SlicerPanel.tsx`
- [ ] Manually verify end-to-end against OctoPrint's built-in Virtual Printer, a real Moonraker instance (`octoprint_compat` enabled), and a real ESP32 dev board flashed with ESP3D firmware
- [ ] (Fast-follow) mDNS/zeroconf printer auto-discovery
- [ ] (Fast-follow) Stream G-code to printer layer-by-layer as it's sliced, instead of upload-then-print
- [ ] (Fast-follow) Native Moonraker client (if `octoprint_compat` coverage proves insufficient)
- [ ] (Fast-follow) Move printer API keys from plaintext JSON to OS keychain storage
- [ ] (Fast-follow) Feed printer telemetry/status back into `tpt-vertex-simulation` for closed-loop print-deviation detection
