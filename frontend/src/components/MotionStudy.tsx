import { useEffect, useRef, useState } from "react";
import { runMotionFrame, type MotionResult, type PartPose } from "../geometry/simulation";

/**
 * Motion-study timeline + playback. Drives a revolute joint through its angle
 * range and shows the resulting part poses. Relies on the Rust `MotionPlayer`
 * (desktop) or the local quaternion fallback (browser).
 */
export function MotionStudy() {
  const [angle, setAngle] = useState(0);
  const [poses, setPoses] = useState<PartPose[] | null>(null);
  const [playing, setPlaying] = useState(false);
  const raf = useRef<number | null>(null);

  const axis: [number, number, number] = [0, 0, 1];
  const anchor: [number, number, number] = [0, 0, 0];

  const compute = (a: number) => {
    runMotionFrame({ axis, angle: a, anchor }).then((r: MotionResult) => {
      setPoses(r.parts);
    });
  };

  useEffect(() => {
    compute(angle);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [angle]);

  useEffect(() => {
    if (!playing) return;
    let last = performance.now();
    const tick = (now: number) => {
      const dt = (now - last) / 1000;
      last = now;
      setAngle((a) => {
        const next = a + dt * 1.5; // rad/s
        return next > Math.PI * 2 ? 0 : next;
      });
      raf.current = requestAnimationFrame(tick);
    };
    raf.current = requestAnimationFrame(tick);
    return () => {
      if (raf.current) cancelAnimationFrame(raf.current);
    };
  }, [playing]);

  return (
    <section className="panel sim-motion" aria-label="Motion study">
      <h2 className="panel-title">Motion study</h2>
      <div className="motion-controls">
        <button
          className="primary"
          onClick={() => setPlaying((p) => !p)}
          aria-label={playing ? "Pause" : "Play"}
        >
          {playing ? "Pause" : "Play"}
        </button>
        <output className="mono" aria-live="polite">
          {(angle * (180 / Math.PI)).toFixed(0)}°
        </output>
      </div>
      <label className="motion-slider">
        Angle
        <input
          type="range"
          min={0}
          max={360}
          value={(angle * 180) / Math.PI}
          onChange={(e) => {
            setPlaying(false);
            setAngle((Number(e.target.value) * Math.PI) / 180);
          }}
          aria-label="Joint angle"
        />
      </label>
      {poses && (
        <ul className="stats-list">
          {poses.map((p) => (
            <li key={p.name}>
              <span>{p.name}</span>
              <span className="mono">
                [{p.position.map((v) => v.toFixed(1)).join(", ")}]
              </span>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
