# ADR-0003: Monorepo layout (kernel / frontend / desktop)

- Status: Accepted
- Date: 2026-07-18

## Context

TPT Vertex spans a Rust geometry kernel, a TypeScript/React frontend, and a
Tauri desktop client. We need a structure that keeps each component's toolchain
isolated (Cargo vs npm) while sharing a single repository, CI, license, and
governance model.

## Decision

We use a single Git repository with three top-level packages:

- `kernel/` (now `tpt-vertex-kernel/`) — a Cargo workspace member (Rust geometry kernel). The root
  `Cargo.toml` declares the workspace; `frontend/` and `desktop/` are
  `exclude`d so they are not part of the Rust workspace.
- `frontend/` — Vite + React + TypeScript package (own `package.json`).
- `desktop/` — Tauri client (own `package.json`) wrapping the frontend build.

Shared concerns (CI, GitHub templates, license, ADRs) live at the repo root.

## Consequences

- One repo, one issue tracker, one set of release docs.
- Rust and JS toolchains do not interfere with each other.
- CI runs two independent pipelines (Rust, Frontend).
- Cross-package versioning must be coordinated manually until a release process
  is defined.
