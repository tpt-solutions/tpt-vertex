# Contributing to TPT Vertex

Thanks for your interest in contributing! TPT Vertex is an open-source,
collaborative 3D CAD platform, and we welcome contributions of all kinds:
code, documentation, bug reports, and design discussion.

## Code of Conduct

By participating, you agree to abide by our
[Code of Conduct](CODE_OF_CONDUCT.md).

## Licensing

All contributions are dual-licensed under **MIT OR Apache-2.0**. By submitting a
pull request, you agree that your contributions may be distributed under those
terms. New source files should carry an SPDX identifier header:

```rust
// SPDX-License-Identifier: MIT OR Apache-2.0
```

```ts
// SPDX-License-Identifier: MIT OR Apache-2.0
```

## Development Setup

### Prerequisites

- [Rust](https://rustup.rs/) (stable)
- [Node.js](https://nodejs.org/) LTS + npm
- A WebGPU-capable browser

### Getting started

1. Fork and clone the repository.
2. Build the kernel:
   ```sh
   cargo build --workspace
   cargo test --workspace
   ```
3. Install frontend dependencies:
   ```sh
   cd frontend && npm install
   ```

## Repository Structure

- `tpt-vertex-kernel/` — Rust geometry kernel (Cargo workspace member)
- `frontend/` — React Three Fiber web UI
- `desktop/` — Tauri desktop client
- `docs/` — Architecture Decision Records (ADRs) and guides

## Coding Standards

### Rust (kernel)

- Format with `cargo fmt --all`.
- Lint with `cargo clippy --workspace --all-targets -- -D warnings`.
- Write unit tests for math, the constraint solver, and feature-tree rebuilds.
- Prefer explicit, documented public APIs at the FFI/WASM boundary.

### TypeScript / React (frontend)

- Format with Prettier and lint with ESLint.
- Use TypeScript strict mode. No `any` without a documented reason.
- Keep components small and testable; co-locate tests with components.

### Commits & Pull Requests

- Keep commits focused and write clear messages.
- Reference issues where relevant (`Closes #123`).
- Open PRs against `main` and fill out the PR template.
- CI must pass (Rust + frontend pipelines).

## Architecture Decisions

Significant architectural choices are recorded as Architecture Decision Records
under [`docs/adr/`](docs/adr/). See
[`docs/adr/0001-record-architecture-decisions.md`](docs/adr/0001-record-architecture-decisions.md)
for the process. If your change involves a notable trade-off, propose an ADR.

## Reporting Bugs & Requesting Features

Use the GitHub issue templates (Bug, Feature, Documentation). Please search
existing issues first to avoid duplicates.

## Questions?

Open a [Discussion](https://github.com/tpt-solutions/vertex/discussions) or
join our community channels (links in the README).
