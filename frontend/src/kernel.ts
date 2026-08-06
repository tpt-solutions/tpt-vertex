/**
 * Dynamic loader for the compiled `tpt-vertex-kernel` WebAssembly module.
 *
 * The module is produced by `npm run build:wasm` (wasm-pack) into
 * `src/wasm/tpt_vertex_kernel.js`. It is an *optional* backend: when it is not
 * present (a plain browser build without the wasm artifact, or before the first
 * `build:wasm` run) every caller falls back to a JS approximation so the app
 * still works.
 *
 * The kernel must be loaded asynchronously (the wasm bytes are fetched at
 * runtime), but the geometry pipeline in `buildMesh` runs synchronously on each
 * render. To bridge the two, `loadKernel()` kicks off the fetch and populates a
 * synchronous `kernelReady()` cache that `buildMesh` reads; once the module
 * resolves, the next render uses the real kernel automatically.
 */

/** Subset of the wasm `Model` API that the frontend drives. */
export interface KernelModel {
  add_box(x0: number, y0: number, x1: number, y1: number, height: number): void;
  add_line(x0: number, y0: number, x1: number, y1: number): void;
  add_circle(cx: number, cy: number, r: number): void;
  add_arc(
    x0: number,
    y0: number,
    x1: number,
    y1: number,
    cx: number,
    cy: number,
    ccw: boolean,
  ): void;
  extrude(height: number): void;
  revolve(angle_deg: number): void;
  union(other: KernelModel): void;
  subtract(other: KernelModel): void;
  intersect(other: KernelModel): void;
  fillet(radius: number): void;
  chamfer(distance: number): void;
  vertices(): Float32Array;
  indices(): Uint32Array;
  volume(): number;
}

export interface KernelModule {
  Model: new () => KernelModel;
}

let ready: KernelModule | null = null;
let inflight: Promise<KernelModule | null> | null = null;

/** Synchronous accessor: the loaded module, or `null` until/unless it loads. */
export function kernelReady(): KernelModule | null {
  return ready;
}

/**
 * Start loading the kernel wasm module. Safe to call repeatedly; the first call
 * triggers the fetch and subsequent calls return the same promise. Resolves to
 * the module, or `null` if it cannot be loaded (browser-only build).
 */
export function loadKernel(): Promise<KernelModule | null> {
  if (inflight) return inflight;
  inflight = (async () => {
    try {
      const modulePath = "../wasm/tpt_vertex_kernel";
      const mod = (await import(/* @vite-ignore */ modulePath)) as KernelModule;
      ready = mod ?? null;
      return ready;
    } catch {
      ready = null;
      return null;
    }
  })();
  return inflight;
}
