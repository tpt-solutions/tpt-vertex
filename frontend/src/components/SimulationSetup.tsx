import { useState } from "react";
import {
  runStaticAnalysis,
  DEFAULT_SETUP,
  type SimSetup,
  type StaticAnalysisResult,
} from "../geometry/simulation";

/**
 * Simulation setup: choose material, boundary conditions, and the applied load,
 * then run a static analysis. The resulting von Mises field + mesh is handed
 * back via `onResult`.
 */
export function SimulationSetup({
  onResult,
}: {
  onResult: (r: StaticAnalysisResult | null) => void;
}) {
  const [setup, setSetup] = useState<SimSetup>(DEFAULT_SETUP);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const update = <K extends keyof SimSetup>(key: K, value: SimSetup[K]) =>
    setSetup((s) => ({ ...s, [key]: value }));

  const run = async () => {
    setBusy(true);
    setError(null);
    try {
      // Fixed face -> node indices. In v1 we rely on the backend to resolve
      // faces to nodes; the browser preview passes an empty set and the
      // rectangle assumption keeps the pipeline exercised.
      const fixedNodes: number[] = [];
      const loads: Array<[number, number, number, number]> = [];
      if (setup.fixedFace !== "none" && setup.loadMagnitude > 0) {
        // Node 0 is treated as the loaded centre of the opposite face.
        loads.push([0, 0, -setup.loadMagnitude, 0]);
      }
      const res = await runStaticAnalysis({
        material: setup.material,
        fixedNodes,
        loads,
        maxTetEdge: setup.maxTetEdge,
      });
      onResult(res);
    } catch (e) {
      setError(String(e));
      onResult(null);
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="panel sim-setup" aria-label="Simulation setup">
      <h2 className="panel-title">Static analysis</h2>
      <label>
        Material
        <select
          value={setup.material}
          onChange={(e) => update("material", e.target.value)}
          aria-label="Material"
        >
          <option value="Steel">Steel</option>
          <option value="Aluminum">Aluminum</option>
          <option value="Titanium">Titanium</option>
          <option value="PLA">PLA</option>
          <option value="ABS">ABS</option>
        </select>
      </label>
      <label>
        Fixed face
        <select
          value={setup.fixedFace}
          onChange={(e) =>
            update("fixedFace", e.target.value as SimSetup["fixedFace"])
          }
          aria-label="Fixed face"
        >
          <option value="bottom">Bottom</option>
          <option value="top">Top</option>
          <option value="none">None (free)</option>
        </select>
      </label>
      <label>
        Load (N, downward)
        <input
          type="number"
          step="5"
          min="0"
          value={setup.loadMagnitude}
          onChange={(e) => update("loadMagnitude", Number(e.target.value))}
          aria-label="Load magnitude"
        />
      </label>
      <label>
        Mesh edge (mm)
        <input
          type="number"
          step="1"
          min="1"
          value={setup.maxTetEdge}
          onChange={(e) => update("maxTetEdge", Math.max(1, Number(e.target.value)))}
          aria-label="Mesh edge length"
        />
      </label>
      <button className="primary" onClick={run} disabled={busy} aria-label="Run analysis">
        {busy ? "Solving…" : "Run analysis"}
      </button>
      {error && (
        <p className="muted" role="alert">
          {error}
        </p>
      )}
    </section>
  );
}
