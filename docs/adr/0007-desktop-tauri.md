# ADR-0007: Desktop client — Tauri wrapper over the web frontend

- Status: Accepted
- Date: 2026-07-19

## Context

Vertex ships as a browser app and a "lightweight desktop client" (per the spec).
The desktop client must offer native file-system access, offline-first local
kernel execution (no server dependency), auto-update, and small signed installers
for Windows/macOS/Linux. We must choose a desktop wrapper.

Options:

- **Electron**: Chromium + Node bundled per app. Mature and ubiquitous, but large
  binaries (100 MB+), heavier memory use, and a JS/Node backend that duplicates
  our Rust core.
- **Tauri**: Rust backend + the OS's native WebView. Small binaries (a few MB),
  low memory, and a Rust "core process" that can call our kernel directly.

Forces:

- Our kernel, renderer, versioning, and (planned) collaboration crates are all
  **Rust**. A Rust-hosted desktop shell can invoke the kernel in-process via the
  existing `ffi`/native crate boundary, giving true offline kernel execution
  with no bundled server.
- WebGPU rendering already targets both browser (via `wasm-bindgen`) and native
  `wgpu`. Tauri's native window can host the same frontend and use native `wgpu`.
- Binary size and signing/notarization matter for an open-source tool users
  self-install; Tauri's small, per-OS installers and built-in updater fit.

## Decision

TPT Vertex's desktop client uses **Tauri**, wrapping the existing web frontend
and calling the Rust kernel in-process.

- The `desktop/` package hosts a Tauri app (`src-tauri/`) whose Rust backend
  depends on `tpt-vertex-kernel` (and later `-versioning`, `-collab`) directly.
- Native file-system open/save, offline local kernel evaluation, and the Tauri
  updater provide the offline-first, auto-updating experience.
- The same React frontend is reused; Tauri commands expose kernel/versioning
  operations to the WebView, and cloud sync is opened on demand (desktop→cloud
  handoff) rather than required.
- Builds are packaged and signed per OS (Windows, macOS, Linux) via Tauri's
  bundler in CI.

## Consequences

- Positive: tiny installers, low memory, direct in-process Rust kernel calls, one
  shared frontend and core across web and desktop, first-class offline support.
- Positive: no second (Node) backend to maintain.
- Negative: reliance on each OS's WebView means occasional rendering/behaviour
  differences to test across platforms; code-signing/notarization setup per OS is
  required for distribution.
- Follow-up: scaffold `src-tauri`, implement FS commands and offline kernel
  bridge, wire the updater, and set up signed CI packaging for all three OSes.
