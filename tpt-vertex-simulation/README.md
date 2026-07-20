# tpt-vertex-simulation

Simulation for [TPT Vertex](https://tpt-vertex.dev), the open-source parametric
CAD platform.

Provides linear-elastic static-stress analysis (small deformation, isotropic
material) over a tetrahedralized volume mesh derived from a kernel `Solid`,
plus forward-kinematics motion playback over kernel `Assembly` mates.

**Status:** scaffolding only — the FEA and motion pipelines are not yet
implemented. See the
[`todo.md`](https://github.com/tpt-solutions/vertex/blob/main/todo.md)
Phase 11 checklist for progress.

## License

Dual-licensed under [MIT](../LICENSE-MIT) or [Apache-2.0](../LICENSE-APACHE), at
your option.
