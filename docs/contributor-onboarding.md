# Contributor Onboarding

Welcome! This guide gets external contributors from zero to a merged pull
request. For coding standards and the full PR process, see
[CONTRIBUTING.md](../CONTRIBUTING.md).

## 1. Understand the shape of the project

TPT Vertex is a monorepo:

| Path             | What it is                                             |
| ---------------- | ------------------------------------------------------ |
| `tpt-vertex-kernel/`     | Rust geometry kernel (math, sketches, features, solids) |
| `renderer/`              | WebGPU/wgpu renderer + culling/LOD helpers              |
| `manufacturing/`         | Export/import (STL, OBJ, glTF, STEP), BOM, plugins      |
| `versioning/`            | Git-like commits/branches/merge over feature manifests |
| `collab/`                | CRDT document + sync hub for real-time collaboration    |
| `platform/`              | Accounts, orgs/teams, projects, sharing, storage        |
| `tpt-vertex-slicer/`     | FDM slicing engine (planar layers, infill, G-code)      |
| `tpt-vertex-simulation/` | Static FEA + assembly motion/kinematics                 |
| `frontend/`      | React Three Fiber UI (Vite + TypeScript + zustand)      |
| `desktop/`       | Tauri desktop client                                    |
| `docs/`          | Documentation and ADRs                                  |

The **kernel is the source of truth**; other crates consume its geometry. Read
the [ADRs](adr/README.md) to understand key decisions before large changes.

## 2. Set up your environment

Prerequisites:

- Rust (stable) with `rustfmt` and `clippy`.
- Node.js LTS.

Clone and build:

```bash
git clone https://github.com/tpt-solutions/vertex
cd vertex

# Rust workspace
cargo build --workspace
cargo test --workspace

# Frontend
cd frontend
npm install
npm test
```

## 3. Run the checks CI runs

Match CI locally before pushing:

```bash
# Rust
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

# Frontend (from frontend/)
npm run lint
npx prettier --check .
npm run build
npm run test
```

## 4. Pick something to work on

- Issues labelled **good first issue** are ideal starting points.
- Comment on the issue to claim it and ask questions.
- For anything large or architectural, open a Discussion or a draft ADR first.

## 5. Make the change

- Follow existing conventions in the crate/package you're editing (module layout,
  inline `#[cfg(test)]` tests for Rust, colocated `*.test.tsx` for the frontend).
- Every new Rust file carries the SPDX header
  `// SPDX-License-Identifier: MIT OR Apache-2.0`.
- Add tests for new behavior. Keep public APIs documented.

## 6. Open a pull request

- Use the PR template; describe what and why, and link the issue.
- Ensure all CI checks pass.
- Be responsive to review feedback; keep commits focused.

## 7. After merge

Your contribution ships under the project's dual MIT OR Apache-2.0 license.
Thank you — you've helped make open hardware design a little more free.
