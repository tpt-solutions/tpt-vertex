# tpt-vertex-kernel

The geometry kernel for [TPT Vertex](https://tpt-vertex.dev), the open-source
parametric CAD platform.

Provides math primitives (vectors, matrices, quaternions, transforms), 2D sketch
primitives and constraint solving, a parametric feature tree (extrude, revolve,
sweep, loft, boolean, fillet/chamfer) with rebuild/recompute, and assembly/mating
for multi-part positioning. Ships with optional `wasm` (browser/WebGPU) and `ffi`
(native desktop) build targets. A `Material` table (density, elastic moduli, yield
strength) is shared with the slicer and simulation crates.

The kernel is **fully implemented** (Phase 1 of the roadmap) and is the source of
truth for geometry across the platform.

## License

Dual-licensed under [MIT](../LICENSE-MIT) or [Apache-2.0](../LICENSE-APACHE), at
your option.
