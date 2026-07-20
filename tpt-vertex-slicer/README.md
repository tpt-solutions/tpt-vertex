# tpt-vertex-slicer

FDM 3D-printing slicer for [TPT Vertex](https://tpt-vertex.dev), the open-source
parametric CAD platform.

Turns a kernel `Solid` mesh into printable G-code: planar layering, perimeter/wall
generation via polygon offsetting, rectilinear infill, and toolpath/G-code
emission for a configurable FDM printer profile.

**Status:** scaffolding only — the slicing algorithms are not yet implemented.
See the [`todo.md`](https://github.com/tpt-solutions/vertex/blob/main/todo.md)
Phase 10 checklist for progress.

## License

Dual-licensed under [MIT](../LICENSE-MIT) or [Apache-2.0](../LICENSE-APACHE), at
your option.
