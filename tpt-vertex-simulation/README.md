# tpt-vertex-simulation

Simulation for [TPT Vertex](https://tpt-vertex.dev), the open-source parametric
CAD platform.

Provides linear-elastic static-stress analysis and assembly motion/kinematics over
geometry derived from a kernel `Solid` / `Assembly`. The solver is the `faer`
sparse linear algebra library, contained to this crate only.

## What's implemented

- **Static FEA** — watertight precondition + tetrahedralization, isotropic elasticity,
  boundary conditions (fixed constraints, point/surface loads), sparse stiffness
  assembly, and post-processing (von Mises scalar, surface interpolation).
- **Motion study** — `Motion` / `MotionPlayer` for time/parameter-driven mate
  playback over kernel assembly mates.
- **Advanced analyses (fast-follows)** — nonlinear (geometric + J2 plasticity),
  contact/interference, rigid-body dynamics, thermal + thermal-stress, fatigue,
  modal/frequency, buckling, higher-order tetrahedra, adaptive mesh refinement, and
  contact-coupled static FEA.
- **WASM execution** — `wasm` feature drops `faer`/rayon for a dense LU solver so
  analysis runs in the browser (`wasm32-unknown-unknown`).
- **Optimization** — topology optimization (SIMP + OC) driven by analysis results.
- **Spec-verified kernels** — stiffness assembly, BC application, and the advanced
  solvers are verified against analytical solutions (cantilever beam, axial bar,
  plate-with-hole Kirsch stress) via `tpt-telos`.

## Example

```rust
use tpt_vertex_kernel::material::Material;
use tpt_vertex_simulation::{run_static_analysis, AnalysisSettings, BoundaryCondition};

let material = Material::from_name("Steel");
let bc = BoundaryCondition::new().fix_node(0);
let settings = AnalysisSettings::new(material, bc, 0.5);
let res = run_static_analysis(&solid, &settings).expect("static analysis");
println!("max von Mises: {:.1} MPa", res.max_von_mises);
```

## License

Dual-licensed under [MIT](../LICENSE-MIT) or [Apache-2.0](../LICENSE-APACHE), at
your option.
