import type { FeatureNode } from "../state/types";
import { kernelReady, type KernelModel, type KernelModule } from "../kernel";

export interface MeshData {
  positions: Float32Array;
  indices: Uint32Array;
}

/**
 * Build a renderable mesh from the current feature list.
 *
 * When the compiled `tpt-vertex-kernel` WASM module is available
 * (`src/wasm/tpt_vertex_kernel`, built via `npm run build:wasm`), the mesh is
 * evaluated by the real geometry kernel: the feature list is replayed through
 * the kernel `Model` (extrude / revolve / boolean / fillet / chamfer) and the
 * resulting triangle soup is returned. Otherwise this falls back to the
 * lightweight box approximation that mirrors the kernel contract, so the
 * viewport always has something to draw in a plain browser build.
 */
export function buildMesh(features: FeatureNode[]): MeshData {
  const mod = kernelReady();
  if (mod) {
    try {
      const data = buildFromKernel(mod, features);
      if (data) return data;
    } catch {
      // Fall through to the JS approximation on any kernel error.
    }
  }
  return buildBox(features);
}

/** Replay the feature list through the real kernel `Model` API. */
function buildFromKernel(mod: KernelModule, features: FeatureNode[]): MeshData | null {
  const num = (v: unknown, d: number) => (typeof v === "number" ? v : d);

  let current: KernelModel | null = null;

  for (const f of features) {
    switch (f.type) {
      case "extrude": {
        const x0 = num(f.params.x0, 0);
        const y0 = num(f.params.y0, 0);
        const x1 = num(f.params.x1, 40);
        const y1 = num(f.params.y1, 40);
        const h = num(f.params.height, 30);
        const m = new mod.Model();
        m.add_box(x0, y0, x1, y1, h);
        current = m;
        break;
      }
      case "revolve": {
        const x0 = num(f.params.x0, 0);
        const y0 = num(f.params.y0, 0);
        const x1 = num(f.params.x1, 40);
        const y1 = num(f.params.y1, 40);
        const h = num(f.params.height, 30);
        const angle = num(f.params.angle, 360);
        const m = new mod.Model();
        m.add_box(x0, y0, x1, y1, h);
        m.revolve(angle);
        current = m;
        break;
      }
      case "fillet": {
        if (current) current.fillet(num(f.params.radius, 2));
        break;
      }
      case "chamfer": {
        if (current) current.chamfer(num(f.params.distance, 2));
        break;
      }
      case "boolean": {
        const bx0 = num(f.params.bx0, 0);
        const by0 = num(f.params.by0, 0);
        const bx1 = num(f.params.bx1, 20);
        const by1 = num(f.params.by1, 20);
        const bh = num(f.params.bh, 20);
        const other = new mod.Model();
        other.add_box(bx0, by0, bx1, by1, bh);
        if (current) {
          const op = String(f.params.op ?? "union");
          if (op === "subtract") current.subtract(other);
          else if (op === "intersect") current.intersect(other);
          else current.union(other);
        }
        break;
      }
      case "sketch":
      default:
        break;
    }
  }

  if (!current) return null;
  const positions = current.vertices();
  const indices = current.indices();
  if (!positions.length || !indices.length) return null;
  return { positions, indices };
}

/** Box approximation mirroring the kernel WASM contract (used as a fallback). */
function buildBox(features: FeatureNode[]): MeshData {
  const box = features.find((f) => f.type === "extrude");
  const w = typeof (box?.params.x1 ?? 40) === "number" ? Number(box?.params.x1 ?? 40) : 40;
  const h = Number(box?.params.height ?? 30);

  const hw = w / 2;
  const hh = h / 2;
  const positions = new Float32Array([
    -hw, -hw, -hh, hw, -hw, -hh, hw, hw, -hh, -hw, hw, -hh, -hw, -hw, hh, hw, -hw, hh, hw, hw, hh,
    -hw, hw, hh,
  ]);
  const indices = new Uint32Array([
    0, 1, 2, 0, 2, 3, 4, 6, 5, 4, 7, 6, 0, 4, 5, 0, 5, 1, 1, 5, 6, 1, 6, 2, 2, 6, 7, 2, 7, 3, 3, 7,
    4, 3, 4, 0,
  ]);
  return { positions, indices };
}
