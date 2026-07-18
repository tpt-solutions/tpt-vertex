import type { FeatureNode } from "../state/types";

export interface MeshData {
  positions: Float32Array;
  indices: Uint32Array;
}

/**
 * Build a renderable mesh from the current feature list. This is a placeholder
 * tessellation that mirrors the kernel WASM contract (a box extrude produces a
 * unit-cube-derived solid). The real path goes through the Rust kernel's
 * `vertices()`/`indices()` exports once the WASM module is wired in.
 */
export function buildMesh(features: FeatureNode[]): MeshData {
  const box = features.find((f) => f.type === "extrude");
  const w = typeof (box?.params.x1 ?? 40) === "number" ? Number(box?.params.x1 ?? 40) : 40;
  const h = Number(box?.params.height ?? 30);

  const hw = w / 2;
  const hh = h / 2;
  const positions = new Float32Array([
    -hw,
    -hw,
    -hh,
    hw,
    -hw,
    -hh,
    hw,
    hw,
    -hh,
    -hw,
    hw,
    -hh,
    -hw,
    -hw,
    hh,
    hw,
    -hw,
    hh,
    hw,
    hw,
    hh,
    -hw,
    hw,
    hh,
  ]);
  const indices = new Uint32Array([
    0, 1, 2, 0, 2, 3, 4, 6, 5, 4, 7, 6, 0, 4, 5, 0, 5, 1, 1, 5, 6, 1, 6, 2, 2, 6, 7, 2, 7, 3, 3, 7,
    4, 3, 4, 0,
  ]);
  return { positions, indices };
}
