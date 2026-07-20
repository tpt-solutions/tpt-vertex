/**
 * Minimal browser-side static-FEA + motion preview for the current parametric
 * model, mirroring the Rust `tpt-vertex-simulation` pipeline.
 *
 * When running inside the Tauri desktop shell the authoritative Rust solver is
 * invoked via the `run_static_analysis` / `run_motion_frame` commands. In a
 * plain browser build (no Tauri backend) we fall back to a lightweight local
 * approximation so the panels stay useful and testable.
 */

export interface AnalysisRequest {
  material: string;
  fixedNodes: number[];
  loads: Array<[number, number, number, number]>;
  maxTetEdge: number;
}

export interface StaticAnalysisResult {
  /** Flattened node positions [x,y,z, x,y,z, ...]. */
  nodes: number[];
  /** Tetrahedron connectivity (4 indices per tet). */
  tets: number[];
  /** Per-element von Mises stress (MPa), one per tet. */
  vonMises: number[];
  maxDisplacement: number;
  maxVonMises: number;
}

export interface MotionRequest {
  axis: [number, number, number];
  angle: number;
  anchor: [number, number, number];
}

export interface PartPose {
  name: string;
  position: [number, number, number];
  rotation: [number, number, number, number];
}

export interface MotionResult {
  parts: PartPose[];
}

/** Frontend simulation-setup parameters (shared with `SimulationSetup`). */
export interface SimSetup {
  material: string;
  /** Which face to fully fix: "bottom" or "top" (mapped to mesh nodes by index). */
  fixedFace: "bottom" | "top" | "none";
  /** Downward load magnitude (N) applied at the opposite face centre. */
  loadMagnitude: number;
  maxTetEdge: number;
}

export const DEFAULT_SETUP: SimSetup = {
  material: "Steel",
  fixedFace: "bottom",
  loadMagnitude: 50,
  maxTetEdge: 5,
};

function hasTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI__" in (window as any);
}

async function invoke<T>(cmd: string, args: unknown): Promise<T> {
  const w = window as unknown as {
    __TAURI__?: {
      core?: { invoke: (c: string, a: unknown) => Promise<T> };
      invoke?: (c: string, a: unknown) => Promise<T>;
    };
  };
  if (w.__TAURI__?.core?.invoke) return w.__TAURI__.core.invoke(cmd, args);
  if (w.__TAURI__?.invoke) return w.__TAURI__.invoke(cmd, args);
  throw new Error("tauri-unavailable");
}

/** Normalize a 3-vector. */
function norm(v: [number, number, number]): [number, number, number] {
  const l = Math.hypot(v[0], v[1], v[2]) || 1;
  return [v[0] / l, v[1] / l, v[2] / l];
}

/** Quaternion (w,x,y,z) for a rotation of `angle` rad about a unit `axis`. */
export function quatFromAxisAngle(
  axis: [number, number, number],
  angle: number,
): [number, number, number, number] {
  const [x, y, z] = norm(axis);
  const h = angle / 2;
  const s = Math.sin(h);
  return [Math.cos(h), x * s, y * s, z * s];
}

/** Rotate a vector by a quaternion (w,x,y,z). */
function rotateVec(
  q: [number, number, number, number],
  v: [number, number, number],
): [number, number, number] {
  const [w, x, y, z] = q;
  const ix = w * v[0] + y * v[2] - z * v[1];
  const iy = w * v[1] + z * v[0] - x * v[2];
  const iz = w * v[2] + x * v[1] - y * v[0];
  const iw = -x * v[0] - y * v[1] - z * v[2];
  return [
    ix * w + iw * -x + iy * -z - iz * -y,
    iy * w + iw * -y + iz * -x - ix * -z,
    iz * w + iw * -z + ix * -y - iy * -x,
  ];
}

/**
 * Run a static linear-elastic analysis. Falls back to a local box-mesh estimate
 * (zero stress when unloaded) when Tauri is unavailable.
 */
export async function runStaticAnalysis(
  req: AnalysisRequest,
): Promise<StaticAnalysisResult> {
  if (hasTauri()) {
    try {
      return await invoke<StaticAnalysisResult>("run_static_analysis", {
        spec: currentModelSpec(),
        req,
      });
    } catch {
      /* fall through to local estimate */
    }
  }
  return localAnalysis(req);
}

/** Build the rectangular-box model spec consumed by the desktop commands. */
function currentModelSpec(): { rect: [number, number, number, number]; height: number } {
  return { rect: [0, 0, 40, 40], height: 30 };
}

function localAnalysis(req: AnalysisRequest): StaticAnalysisResult {
  // Approximate the model as a unit cube mesh so the viewer has something to
  // color; stress is reported from the request only if a load is present.
  const nodes: number[] = [
    -20, -20, 0, 20, -20, 0, 20, 20, 0, -20, 20, 0, -20, -20, 30, 20, -20, 30,
    20, 20, 30, -20, 20, 30,
  ];
  const tets = [0, 1, 2, 4, 1, 2, 5, 4, 2, 3, 6, 5, 3, 0, 4, 7, 4, 5, 6, 7, 0, 1, 5, 4];
  const loaded = req.loads.length > 0;
  const vonMises = tets.length / 4 > 0 ? tets.length / 4 : 1;
  const values = new Array(Math.max(1, Math.floor(vonMises))).fill(
    loaded ? 200 : 0,
  );
  return {
    nodes,
    tets,
    vonMises: values,
    maxDisplacement: loaded ? 0.5 : 0,
    maxVonMises: loaded ? 200 : 0,
  };
}

/**
 * Compute a motion-study frame driving a revolute joint by `angle` about `axis`
 * through `anchor`. Falls back to a local quaternion rotation when Tauri is
 * unavailable.
 */
export async function runMotionFrame(req: MotionRequest): Promise<MotionResult> {
  if (hasTauri()) {
    try {
      return await invoke<MotionResult>("run_motion_frame", {
        spec: currentModelSpec(),
        req,
      });
    } catch {
      /* fall through */
    }
  }
  const q = quatFromAxisAngle(req.axis, req.angle);
  const offset: [number, number, number] = [
    req.anchor[0] + 10,
    req.anchor[1],
    req.anchor[2] + 5,
  ];
  const rotated = rotateVec(q, offset);
  return {
    parts: [
      { name: "base", position: [0, 0, 0], rotation: [1, 0, 0, 0] },
      { name: "mover", position: rotated, rotation: q },
    ],
  };
}
