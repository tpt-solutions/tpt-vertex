# TPT Vertex

> The "Figma for Hardware" — a modern, cross-platform, parametric 3D CAD platform built for real-time collaboration.

[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)

TPT Vertex is an open-source parametric CAD kernel and editor that lets hardware
engineers design, collaborate, and version 3D parts and assemblies the way
software teams write code. It removes the cost and platform barriers of legacy
CAD tools (SolidWorks, CATIA) and replaces clunky, single-user workflows with a
fast, browser-first, collaborative experience.

## Why

The open-source hardware movement — robotics, EVs, prosthetics, space tech — is
bottlenecked by CAD. Professional tools are expensive and Windows-only; the main
open alternative (FreeCAD) is hard to learn and architecturally cannot support
real-time collaboration. Engineers work in silos, emailing huge binary files.

Vertex fixes this:

- **Parametric, rebuildable models** — a feature tree you can edit and recompute.
- **Real-time collaboration** — multiple engineers edit the same assembly at once
  using CRDTs (conflict-free replicated data types) over WebSockets.
- **Git-like version control** — branch, merge, and review 3D geometry changes.
- **Native-speed rendering** — WebGPU via `wgpu` in the browser and a lightweight
  desktop client.
- **Free & open** — dual-licensed under MIT OR Apache-2.0.

## Tech Stack

| Layer                | Technology                                                       |
| -------------------- | ---------------------------------------------------------------- |
| Geometry Kernel      | Rust — memory-safe, high-performance parametric math & solver    |
| Rendering Engine     | WebGPU / `wgpu` (Rust)                                           |
| Collaboration Sync   | Custom Rust CRDT over WebSockets (see ADR-0006)                  |
| Frontend UI          | React Three Fiber (React + Three.js) + Vite + TypeScript         |
| Version Control      | Custom manifest + blob engine (see ADR-0005)                    |
| Desktop Client       | Tauri (Rust core, no Electron bloat)                             |

## Repository Layout

This is a monorepo:

```
vertex/
├── kernel/          # Rust geometry kernel (math, sketch, features, solids)
├── renderer/        # WebGPU/wgpu renderer + culling/LOD helpers
├── manufacturing/   # Export/import (STL, OBJ, glTF, STEP), BOM, drawings, plugins
├── versioning/      # Git-like commits/branches/merge over feature manifests
├── collab/          # CRDT document + sync hub for real-time collaboration
├── platform/        # Accounts, orgs/teams, projects, sharing, storage backend
├── frontend/        # Web UI (React Three Fiber, Vite, TypeScript)
├── desktop/         # Tauri desktop client wrapping the web frontend
├── docs/            # Documentation, user guide, and ADRs
├── .github/         # CI workflows, issue & PR templates
├── LICENSE-MIT      # MIT license
├── LICENSE-APACHE   # Apache-2.0 license
└── NOTICE           # Attribution (Apache requirement)
```

The Rust workspace members are `kernel`, `renderer`, `manufacturing`,
`versioning`, `collab`, and `platform`. The `frontend` and `desktop` packages are
excluded from the Cargo workspace so they use their own JS toolchains (`desktop`
still embeds the kernel crates by path for offline evaluation).

## Architecture Overview

```
        ┌─────────────────────────────────────────────┐
        │               Frontend (R3F)                │
        │   viewport · sketch editor · feature tree    │
        │   inspector · collaboration presence         │
        └───────────────┬───────────────┬─────────────┘
                        │               │
              WebGPU/wgpu │        CRDT sync (WebSocket)
              render mesh │
                        │               │
        ┌───────────────▼───────────────▼─────────────┐
        │            Geometry Kernel (Rust)            │
        │   math · 2D sketch · constraint solver ·     │
        │   feature tree · boolean/fillet · assembly   │
        └───────────────────────────────────────────────┘
```

- The **kernel** is the source of truth for geometry. It exposes a clean API and
  (eventually) a WASM build for the browser plus FFI bindings for the desktop.
- The **renderer** tessellates kernel geometry and draws it with WebGPU.
- The **frontend** orchestrates UI, wraps the renderer, and binds editor state to
  the collaboration layer.
- **Version control** and **collaboration** operate on the parametric feature
  tree, not on raw meshes — so merges and conflict resolution stay meaningful.

## Building

### Prerequisites

- Rust (stable, via [rustup](https://rustup.rs/))
- Node.js (LTS) and a package manager (npm / pnpm / yarn)
- A WebGPU-capable browser (Chrome/Edge 113+, or Firefox/Safari behind flags)

### Kernel (Rust)

```sh
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

### Frontend

```sh
cd frontend
npm install
npm run dev      # local dev server
npm run build    # production build
npm run lint     # eslint + prettier check
npm run test     # vitest
```

### Desktop

```sh
cd desktop
npm install
npm run tauri dev
```

## Documentation

Full documentation lives in [`docs/`](docs/README.md): the
[user guide](docs/user-guide.md), the
[public API & plugin interface](docs/plugin-api.md),
[architecture decision records](docs/adr/README.md), and operational notes
([security](docs/security-review.md), [accessibility](docs/accessibility.md)).

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup, coding standards, and
the pull-request process. By contributing you agree your contributions are
licensed under MIT OR Apache-2.0 (see [LICENSE-MIT](LICENSE-MIT) and
[LICENSE-APACHE](LICENSE-APACHE)).

## License

TPT Vertex is dual-licensed under either of:

- **MIT** ([LICENSE-MIT](LICENSE-MIT))
- **Apache License, Version 2.0** ([LICENSE-APACHE](LICENSE-APACHE))

at your option.

Copyright © 2026 TPT Solutions.
