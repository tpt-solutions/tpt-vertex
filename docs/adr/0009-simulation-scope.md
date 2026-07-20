# ADR-0009: Simulation scope and solver dependency

- Status: Accepted
- Date: 2026-07-20

## Context

Phase 11 adds engineering simulation to TPT Vertex. SolidWorks-class CAD offers a
broad simulation surface (static/nonlinear FEA, thermal, modal, fatigue, rigid-
body dynamics, CFD). We must decide (1) how much of that to build for v1 and
(2) whether to take on a numerical dependency for the linear solve, given the
project has kept its kernel dependency-free.

Forces:

- The highest-value, most tractable first step is **linear-elastic static
  stress** (small-deformation, isotropic material) — the classic "will this part
  break" check — plus **assembly motion/kinematics** playback, which reuses the
  existing `Assembly`/`Mate` structure and quaternion math.
- A correct FEA pipeline needs a sparse symmetric-positive-definite linear solve.
  Hand-rolling a robust sparse Cholesky/CG is possible but error-prone; a focused
  dependency dramatically de-risks correctness.
- The kernel is deliberately dependency-free and WASM-friendly. Any solver
  dependency should be contained to the simulation crate so the kernel and the
  rest of the workspace stay lean.

## Decision

1. **v1 scope = linear static FEA + forward-kinematics motion.** The
   `tpt-vertex-simulation` crate implements: watertight validation +
   tetrahedralization of a kernel `Solid`; isotropic linear elasticity; fixed
   constraints and point/surface loads; the linear 4-node tetrahedron (constant-
   strain) element; global sparse stiffness assembly with boundary conditions; a
   linear solve; and von Mises stress post-processing. Motion is forward
   kinematics driving DOF-bearing mates (`Revolute`, `Slider`, `Cylindrical`)
   over time/parameter. Everything else (nonlinear, thermal, modal, fatigue,
   dynamics, contact) is an explicit fast-follow in `todo.md`.

2. **Self-contained solver, dependency isolated to the simulation crate.** For
   v1 the crate ships a dependency-free Conjugate-Gradient solver with a Jacobi
   preconditioner (sufficient for SPD stiffness systems and easy to validate
   against analytical benchmarks). The recommended production upgrade is `faer`
   for a fast sparse Cholesky; if adopted it must remain a dependency of
   `tpt-vertex-simulation` only, never the kernel. This keeps the kernel clean
   and WASM-friendly while giving a clear performance upgrade path.

3. **Material lives in the kernel.** A `Material` type (density, Young's modulus,
   Poisson's ratio, yield strength) is added to `tpt-vertex-kernel` and attached
   to `Part`, because material is model data used by BOM (mass) and simulation
   alike. The `manufacturing` BOM density table is folded into this shared table.

## Consequences

- Positive: a correct, testable, analytically-validated static-stress capability
  and motion study with minimal dependency risk; the kernel stays dependency-free
  and WASM-friendly; material is defined once and shared by BOM + FEA.
- Positive: DOF-bearing mates fix the previous `AxisAligned` no-op stub and give
  kinematics real rotational solving.
- Negative: the built-in CG solver is slower than a tuned sparse Cholesky on
  large meshes; large models may need the `faer` upgrade before production use.
- Negative: constant-strain tetrahedra are stiff and need reasonably fine meshes
  for accuracy; higher-order elements and adaptive refinement are fast-follows.
- Follow-up: adopt `faer`; add higher-order elements, adaptive refinement,
  contact, thermal/modal/fatigue, and rigid-body dynamics (see `todo.md`
  Phase 11 fast-follows).
