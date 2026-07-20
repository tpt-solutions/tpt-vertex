import { useState } from "react";
import { SimulationSetup } from "./SimulationSetup";
import { StressResult } from "./StressResult";
import { MotionStudy } from "./MotionStudy";
import type { StaticAnalysisResult } from "../geometry/simulation";

/**
 * Combined simulation panel (Phase 11): static-stress setup + von Mises result
 * viewer, plus a motion-study timeline. Mirrors the `SlicerPanel` modal layout.
 */
export function SimulationPanel({ onClose }: { onClose: () => void }) {
  const [result, setResult] = useState<StaticAnalysisResult | null>(null);

  return (
    <div className="vc-backdrop" role="dialog" aria-label="Simulation">
      <div className="vc-card simulation-card">
        <header className="vc-header">
          <h3>Simulation</h3>
          <div className="spacer" />
          <button onClick={onClose} aria-label="Close">
            Close
          </button>
        </header>

        <div className="simulation-body">
          <SimulationSetup onResult={setResult} />
          <StressResult result={result} />
          <MotionStudy />
        </div>
      </div>
    </div>
  );
}
