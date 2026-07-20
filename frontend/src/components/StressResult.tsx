import { useMemo } from "react";
import type { StaticAnalysisResult } from "../geometry/simulation";

/** Map a normalized value 0..1 to a blue→cyan→green→yellow→red heat color. */
function heat(t: number): string {
  const x = Math.max(0, Math.min(1, t));
  // Simple 5-stop gradient.
  const stops: Array<[number, [number, number, number]]> = [
    [0.0, [30, 60, 200]],
    [0.25, [0, 180, 200]],
    [0.5, [40, 200, 60]],
    [0.75, [240, 210, 40]],
    [1.0, [220, 40, 40]],
  ];
  let lo = stops[0];
  let hi = stops[stops.length - 1];
  for (let i = 0; i < stops.length - 1; i++) {
    if (x >= stops[i][0] && x <= stops[i + 1][0]) {
      lo = stops[i];
      hi = stops[i + 1];
      break;
    }
  }
  const span = hi[0] - lo[0] || 1;
  const f = (x - lo[0]) / span;
  const c = lo[1].map((v, i) => Math.round(v + (hi[1][i] - v) * f));
  return `rgb(${c[0]},${c[1]},${c[2]})`;
}

/**
 * Stress-color-mapped results viewer. Reuses the existing 2D projection approach
 * (top-down on XY) to render each tetrahedral element filled by its von Mises
 * value. A legend shows the MPa scale.
 */
export function StressResult({ result }: { result: StaticAnalysisResult | null }) {
  const { polys, max } = useMemo(() => {
    if (!result || result.tets.length === 0) return { polys: [] as JSX.Element[], max: 1 };
    const nodes = result.nodes;
    const tets = result.tets;
    const maxV = Math.max(1e-9, ...result.vonMises);
    const out: JSX.Element[] = [];
    for (let t = 0; t < tets.length; t += 4) {
      const idx = [tets[t], tets[t + 1], tets[t + 2], tets[t + 3]];
      const pts = idx
        .map((i) => `${nodes[i * 3]},${nodes[i * 3 + 1]}`)
        .join(" ");
      const v = result.vonMises[t / 4] ?? 0;
      out.push(
        <polygon
          key={`t${t}`}
          points={pts}
          fill={heat(v / maxV)}
          stroke="rgba(0,0,0,0.15)"
          strokeWidth={0.3}
        >
          <title>{`${v.toFixed(1)} MPa`}</title>
        </polygon>,
      );
    }
    return { polys: out, max: maxV };
  }, [result]);

  if (!result) {
    return (
      <section className="panel sim-stress" aria-label="Stress results">
        <h2 className="panel-title">Stress results</h2>
        <p className="muted">Run an analysis to see the von Mises field.</p>
      </section>
    );
  }

  return (
    <section className="panel sim-stress" aria-label="Stress results">
      <h2 className="panel-title">Von Mises stress</h2>
      <svg
        className="stress-svg"
        viewBox="-30 -35 60 70"
        role="img"
        aria-label="Von Mises stress map"
      >
        <g transform="scale(1,-1)">{polys}</g>
      </svg>
      <ul className="stats-list">
        <li>
          <span>Max von Mises</span>
          <span className="mono">{result.maxVonMises.toFixed(1)} MPa</span>
        </li>
        <li>
          <span>Max displacement</span>
          <span className="mono">{result.maxDisplacement.toFixed(3)} mm</span>
        </li>
      </ul>
      <div className="stress-legend" aria-hidden="true">
        <div className="legend-bar" />
        <div className="legend-labels">
          <span>0</span>
          <span>{max.toFixed(0)} MPa</span>
        </div>
      </div>
    </section>
  );
}
