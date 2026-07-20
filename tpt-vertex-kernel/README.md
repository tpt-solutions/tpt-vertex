# tpt-vertex-kernel

The geometry kernel for [TPT Vertex](https://tpt-vertex.dev), the open-source
parametric CAD platform.

Provides math primitives (vectors, matrices, quaternions, transforms), 2D sketch
primitives and constraint solving, a parametric feature tree (extrude, revolve,
sweep, loft, boolean, fillet/chamfer) with rebuild/recompute, and assembly/mating
for multi-part positioning. Ships with optional `wasm` (browser/WebGPU) and `ffi`
(native desktop) build targets.

`Solid` — a faceted boundary representation (triangle mesh over a shared vertex
pool) — is the kernel's output type, consumed by the renderer, manufacturing
export, slicer, and simulation crates.

## License

Dual-licensed under [MIT](../LICENSE-MIT) or [Apache-2.0](../LICENSE-APACHE), at
your option.
