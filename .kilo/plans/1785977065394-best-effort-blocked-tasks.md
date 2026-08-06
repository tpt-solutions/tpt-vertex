# Best-effort work for externally-blocked tasks

## Context

`todo.md` has 13 unchecked items. Most are blocked on external resources (signing
certs, real hardware, hosted infra, registry accounts, launch execution). The user
confirmed:

- **Signing is not needed** — macOS is dropped entirely; Windows/Linux will ship
  unsigned builds.
- **Best-effort code can still be written** for the other blocked items where the
  logic is implementable without the external resource. Manual/truly-external items
  stay blocked.

This plan converts the implementable blocked items into concrete code tasks and
updates `todo.md` so they are tracked. All code is self-contained and unit-testable;
items that *require* the missing external resource are explicitly left as blocked
sub-notes rather than deleted.

## Decisions

- macOS packaging/signing: **dropped** (user not catering for macOS).
- Windows/Linux: package **unsigned**; keep the CI matrix, drop signing steps/secrets.
- Slicer print-time/filament: logic already exists uncalibrated
  (`GCode.estimated_filament_mm`, `estimated_time_s` in
  `tpt-vertex-slicer/src/gcode.rs:18-23`). Best-effort = add **filament mass in grams**
  via material density + expose a documented **calibration correction factor** hook
  (uncalibrated default = 1.0). Calibration *against real data* remains blocked.
- G-code validation: add a **static** validator (structure/syntax), not hardware
  validation.
- Collab sync load-test: **local** harness (loopback, N simulated `LocalReplica`
  users against `SyncHub`), no external infra.
- Closed-loop hardware feedback: write **ADR-0012** + a Rust trait/interface
  abstraction in `tpt-vertex-printer-link` (no firmware).
- Cloud-handoff: client-side **stub** in desktop (configurable endpoint, fetches a
  project by id) — unverifiable without a deployed server.
- Logo: create an SVG asset + reference it from README/branding.
- Truly blocked (no code possible): register crate/npm names (registry accounts),
  public launch (execution), manual E2E printer verification (real hardware),
  calibration *against* real printer data (hardware).

## Tasks

### 1. `todo.md` edits (track everything below)
- Line 21 (logo): keep, mark in-progress — implemented by Task 7.
- Line 23 (register names): leave blocked (external accounts) — not in scope.
- Line 91 (Phase 4 load-test): reword to "Write local load-test harness (N simulated
  users, loopback) — infra-dependent scale test deferred."
- Lines 132–134 (Phase 7 signing):
  - 132 → "Package Windows build (unsigned; code-signing not required)."
  - 133 → "macOS packaging dropped — not catering for macOS."
  - 134 → "Package Linux build (unsigned AppImage/deb)."
- Line 142 (Phase 8 load-test): "Write local collaboration load-test harness
  (best-effort; large-scale infra test deferred)."
- Line 149 (launch): leave blocked.
- Line 179 (Phase 9 calibration): reword to "Implement filament-mass (g) estimate +
  calibration correction-factor hook (uncalibrated); calibrate against real printer
  data — blocked on hardware."
- Line 181 (Phase 9 G-code validate): "Implement static G-code validator
  (structure/syntax); validate against real hardware/simulator — blocked."
- Line 182 (Phase 9 closed-loop): "Write ADR-0012 + hardware-feedback trait
  abstraction; firmware-integration design pass — best-effort stub."
- Line 254 (Phase 12 E2E): leave blocked (manual, real hardware).

### 2. Slicer: filament mass + calibration hook — `tpt-vertex-slicer/src/gcode.rs`
- Add `pub estimated_filament_g: f64` to `GCode`.
- After emission, compute
  `estimated_filament_g = estimated_filament_mm * printer.filament_area() * density`
  (density from `Material::from_name(material.name).density`; `filament_area()`
  already used at `gcode.rs:33`).
- Add a `calibration: Option<CalibrationFactors>` param (or fields on
  `MaterialCalibration`) with `time_factor: f64` / `filament_factor: f64` defaulting
  to 1.0; multiply estimates. Document that factors are uncalibrated placeholders.
- Unit test: known layer plan → expected mm, time, and g (using a known density).

### 3. Slicer: static G-code validator — `tpt-vertex-slicer/src/gcode_validate.rs` (new)
- `pub enum GcodeIssue { UnsupportedCommand, NegativeCoord, UnbalancedExtrusion,
  MissingHome, ... }` and `pub fn validate_gcode(text: &str) -> Vec<GcodeIssue>`.
- Checks: only supported G/M codes; coordinates within printer bounds (optional
  `PrinterProfile`); extrusion `E` monotonic non-negative; `G28`/home present before
  moves; balanced retract/restore.
- Wire a `validate` call in the crate-level end-to-end slice test
  (`tpt-vertex-slicer/src/lib.rs` test) and add unit tests for each issue type.

### 4. Collab: local load-test harness — `collab/benches/loadtest.rs` (new) or `tests/`
- Spin up an in-process `SyncHub` (see `collab/src/server.rs`) and N
  `LocalReplica` clients (see `collab/src/lib.rs` / `crdt.rs`).
- Each replica applies a stream of concurrent feature-tree edits; assert all replicas
  converge to the same state (reuse existing convergence/idempotency assertions).
- Report wall-clock convergence time / ops-per-sec. Document that this is a local
  proxy, not a network-scale test.
- Run via `cargo bench -p collab` or `cargo test -p collab --test load`.

### 5. Printer-link: closed-loop hardware feedback abstraction —
`tpt-vertex-printer-link/src/feedback.rs` (new) + `docs/adr/0012-closed-loop-hardware-feedback.md`
- Trait `HardwareFeedback` with `read_sensors() -> SensorReading` (filament width,
  chamber temp, etc.) and `apply_correction(reading) -> Correction`. Keep it
  data-only / no firmware deps.
- ADR-0012 records scope: design-only for now, firmware-integration pass deferred.
- Unit test: a mock sensor feeding a trivial correction loop.

### 6. Desktop: cloud-handoff stub — `desktop/src-tauri/src/cloud.rs` (new) + `frontend/src/cloud/client.ts`
- Tauri command `open_cloud_project(project_id: String, endpoint: Option<String>)`
  using `reqwest` (already a dep in printer-link; add to desktop if needed) to GET a
  project manifest from a configurable endpoint and load it into the kernel.
- Frontend IPC wrapper mirroring `frontend/src/printer/client.ts` pattern.
- Mark clearly as unverified (no deployed server). No integration test against a real
  endpoint; unit-test the URL/serialization logic with a mock.

### 7. Branding: project logo
- Create `assets/logo.svg` (and a dark variant if trivial) — simple mark + wordmark
  "TPT Vertex", license-friendly (MIT/Apache).
- Reference from `README.md` branding section and the Phase 0 logo todo item.

## Validation
- `cargo test -p tpt-vertex-slicer` (new mass + validator tests pass).
- `cargo bench -p collab` / `cargo test -p collab` (load harness converges).
- `cargo test -p tpt-vertex-printer-link` (feedback mock test passes).
- `cargo clippy` + `cargo fmt` clean across touched crates; `npm run lint` / `npm run
  test` in `frontend` for the cloud IPC wrapper.
- `todo.md` reflects all changes; blocked sub-notes preserved.

## Risks / open questions
- `reqwest` may need to be added to `desktop/src-tauri/Cargo.toml` (async Tauri
  command) — confirm current deps before wiring cloud stub.
- Material density lookup by name string must match `Material::from_name` keys in
  `tpt-vertex-kernel/src/material.rs`; if slicer receives a custom name, fall back to
  a default density and emit a warning.
- Cloud-handoff endpoint auth (API key) is out of scope; stub assumes an open/dev
  endpoint.
