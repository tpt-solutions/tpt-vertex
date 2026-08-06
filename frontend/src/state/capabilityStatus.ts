/**
 * Capability status manifest (Phase 13 — "Transparency panel").
 *
 * A single, data-driven, deliberately *honest* description of which TPT Vertex
 * subsystems are really implemented and reachable from the running app, and
 * which are placeholders or still being wired up. The UI (status panel +
 * feature-tree badges) renders from this list only — no status text is
 * hardcoded in components — so keeping this file truthful keeps the product
 * truthful.
 *
 * Conventions for `status`:
 *  - `real`        — implemented, tested, and usable for its stated scope.
 *  - `partial`     — usable, but a documented subset of the intended scope.
 *  - `placeholder` — deliberately approximate stand-in; results are not
 *                    trustworthy for production use.
 *  - `wip`         — the underlying logic exists but is not yet wired end to
 *                    end (or is unverified against real hardware).
 */

export type CapabilityStatus = "real" | "partial" | "placeholder" | "wip";

export interface Capability {
  id: string;
  label: string;
  status: CapabilityStatus;
  phase: number;
  notes: string;
}

/** Presentation metadata for each status value (label + badge colour class). */
export const STATUS_META: Record<CapabilityStatus, { label: string; description: string }> = {
  real: {
    label: "real",
    description: "Implemented, tested, and usable for its stated scope.",
  },
  partial: {
    label: "partial",
    description: "Usable, but only a documented subset of the intended scope.",
  },
  placeholder: {
    label: "placeholder",
    description: "Approximate stand-in — results are not production-trustworthy.",
  },
  wip: {
    label: "wip",
    description: "Logic exists but is not yet wired end to end / not yet verified.",
  },
};

/** Status values in the order the legend and summary counts display them. */
export const STATUS_ORDER: CapabilityStatus[] = ["real", "partial", "placeholder", "wip"];

/**
 * The manifest. Ordered by phase so grouped rendering is stable.
 * Phase numbers refer to the roadmap phases in the repo-root `todo.md`.
 */
export const CAPABILITIES: Capability[] = [
  {
    id: "kernel-math",
    label: "Kernel math & primitives",
    status: "real",
    phase: 1,
    notes:
      "Vectors, matrices, transforms, quaternions and tolerance handling in tpt-vertex-kernel; unit tested.",
  },
  {
    id: "sketch-solver",
    label: "2D sketch + constraint solver",
    status: "real",
    phase: 1,
    notes:
      "Lines, arcs, circles and splines with a working constraint solver (coincident, parallel, perpendicular, dimensional).",
  },
  {
    id: "feature-tree",
    label: "Feature tree (extrude / revolve / sweep / loft)",
    status: "real",
    phase: 1,
    notes:
      "Parametric dependency graph with a real rebuild engine; extrude, revolve, sweep and loft are implemented.",
  },
  {
    id: "boolean-ops",
    label: "Boolean operations (union / subtract / intersect)",
    status: "real",
    phase: 1,
    notes:
      "Real BSP-tree triangle-mesh CSG engine (ADR-0013) over the kernel's faceted Solid; union/subtract/intersect are wired through feature.rs into csg_bsp and unit tested for volume/watertightness.",
  },
  {
    id: "fillet-chamfer",
    label: "Fillet / chamfer",
    status: "real",
    phase: 1,
    notes:
      "Real edge-classification + subtractive rolling-ball fillet and bevel tools (edges.rs) operate on convex manifold edges of planar-faced solids; faceted approximation of the exact blend (documented limits).",
  },
  {
    id: "rendering",
    label: "Rendering (WebGPU / wgpu)",
    status: "real",
    phase: 2,
    notes:
      "wgpu renderer with scene graph, camera, PBR materials, picking/hover, frustum culling, LOD and instancing.",
  },
  {
    id: "frontend-ui",
    label: "Frontend UI panels",
    status: "real",
    phase: 3,
    notes:
      "App shell, viewport, feature tree, sketch editor, assembly tree, properties, undo/redo, theming and onboarding.",
  },
  {
    id: "collab-crdt",
    label: "Collaboration CRDT (library)",
    status: "real",
    phase: 4,
    notes:
      "Custom Rust CRDT (OR-Set + LWW registers + fractional indexing) with convergence, presence and offline-resync tests.",
  },
  {
    id: "collab-live",
    label: "Real-time multi-user sync (running app)",
    status: "real",
    phase: 4,
    notes:
      "The browser client connects to the `sync_server` WebSocket binary (adapting SyncHub), relays CRDT ops and renders remote cursors via the `CollabLayer` overlay. The kernel `collab` crate carries a `wasm` feature mirroring the kernel/simulation pattern.",
  },
  {
    id: "version-control",
    label: "Version control (git-like for 3D)",
    status: "real",
    phase: 5,
    notes:
      "Commits, branches, merge with conflict detection, timeline UI, visual diff and per-feature conflict resolution.",
  },
  {
    id: "export-formats",
    label: "STEP / STL / glTF / OBJ export",
    status: "real",
    phase: 6,
    notes:
      "Faceted STEP (AP203/214), binary + ASCII STL, glTF and Wavefront OBJ exporters in tpt-vertex-manufacturing.",
  },
  {
    id: "step-import",
    label: "STEP import",
    status: "real",
    phase: 6,
    notes: "Tolerant faceted STEP reconstruction that round-trips with the exporter.",
  },
  {
    id: "desktop-client",
    label: "Desktop client (Tauri)",
    status: "real",
    phase: 7,
    notes:
      "Tauri shell with native file access, offline-first local kernel evaluation, export and slicing commands; Windows/Linux packaging in CI.",
  },
  {
    id: "desktop-cloud-sync",
    label: "Desktop ↔ cloud sync handoff",
    status: "wip",
    phase: 7,
    notes:
      "Client-side `open_cloud_project` entry point exists, but the hosted platform/sync deployment it hands off to is not live.",
  },
  {
    id: "slicing",
    label: "Slicing (FDM)",
    status: "real",
    phase: 9,
    notes:
      "Real planar slicing: layering, perimeter offsets, infill, supports (grid + tree), adaptive layers, bridging, seams and G-code emission.",
  },
  {
    id: "simulation",
    label: "Simulation (FEA / motion)",
    status: "real",
    phase: 10,
    notes:
      "Static FEA (tet elements, sparse solve, von Mises) plus motion playback, validated against analytical cantilever/bar/Kirsch cases.",
  },
  {
    id: "simulation-wasm",
    label: "In-browser (wasm) simulation execution",
    status: "wip",
    phase: 10,
    notes:
      "A wasm crate feature exists, but the browser build is not wired into the frontend yet — simulation runs via the desktop app.",
  },
  {
    id: "sheet-metal-cam-gdt",
    label: "Sheet metal / CAM / GD&T",
    status: "real",
    phase: 11,
    notes:
      "Flat-pattern unfolding with K-factor bend allowances, CNC contour/drill/pocket toolpaths, and GD&T feature control frames on drawings.",
  },
  {
    id: "printer-connectivity",
    label: "Printer connectivity (ESP3D / OctoPrint / Moonraker)",
    status: "real",
    phase: 12,
    notes:
      "Real HTTP clients for ESP3D, OctoPrint/Moonraker-compat and native Moonraker, with mDNS discovery and G-code streaming; unit tested against a mock server.",
  },
  {
    id: "printer-hardware-verification",
    label: "End-to-end printer hardware verification",
    status: "wip",
    phase: 12,
    notes:
      "Not yet exercised against a real ESP3D board, Moonraker host or OctoPrint virtual printer — treat live printing as unverified.",
  },
  {
    id: "printer-keychain",
    label: "OS keychain for printer API keys",
    status: "real",
    phase: 13,
    notes:
      "Printer API keys are stored in the OS keychain via the `keyring` crate rather than plaintext in printers.json.",
  },
  {
    id: "auth-hashing",
    label: "Auth password hashing (Argon2id)",
    status: "real",
    phase: 13,
    notes:
      "Platform auth hashes passwords with Argon2id, replacing the earlier placeholder FNV-1a hash.",
  },
];

const BY_ID = new Map(CAPABILITIES.map((c) => [c.id, c]));

/** Look up a single capability by its stable id. */
export function getCapability(id: string): Capability | undefined {
  return BY_ID.get(id);
}

/** Group the manifest by roadmap phase, preserving declaration order per phase. */
export function capabilitiesByPhase(): Record<number, Capability[]> {
  const grouped: Record<number, Capability[]> = {};
  for (const cap of CAPABILITIES) {
    if (!grouped[cap.phase]) grouped[cap.phase] = [];
    grouped[cap.phase].push(cap);
  }
  return grouped;
}

/**
 * Map a feature-tree node type onto the capability that implements it, so the
 * feature tree can badge entries backed by placeholder/WIP subsystems.
 */
const FEATURE_TYPE_CAPABILITY: Record<string, string> = {
  sketch: "sketch-solver",
  extrude: "feature-tree",
  revolve: "feature-tree",
  sweep: "feature-tree",
  loft: "feature-tree",
  boolean: "boolean-ops",
  union: "boolean-ops",
  subtract: "boolean-ops",
  intersect: "boolean-ops",
  fillet: "fillet-chamfer",
  chamfer: "fillet-chamfer",
};

/** Capability backing a given feature type, if one is known. */
export function capabilityForFeatureType(type: string): Capability | undefined {
  const id = FEATURE_TYPE_CAPABILITY[type];
  return id ? getCapability(id) : undefined;
}
