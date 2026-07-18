import { useState } from "react";

interface Step {
  title: string;
  body: string;
}

const STEPS: Step[] = [
  {
    title: "Welcome to TPT Vertex",
    body: "TPT Vertex is a parametric 3D CAD tool — the “Figma for Hardware”. This quick tour covers the basics.",
  },
  {
    title: "Build in the feature tree",
    body: "The left panel lists your sketch and features. Add features and they rebuild the model automatically.",
  },
  {
    title: "Edit dimensions",
    body: "Select a feature to open its parameters in the right panel. Change a value and the model updates live.",
  },
  {
    title: "Sketch in 2D",
    body: "Open the sketch editor to draw lines and circles that become the basis for extrusions and revolves.",
  },
  {
    title: "Undo anything",
    body: "Every edit is recorded. Press Ctrl/Cmd+Z to undo and Ctrl/Cmd+Shift+Z to redo.",
  },
];

export function Onboarding() {
  const [step, setStep] = useState(0);
  const [done, setDone] = useState(false);

  if (done) return null;
  const current = STEPS[step];
  const last = step === STEPS.length - 1;

  return (
    <div className="onboarding-backdrop" role="dialog" aria-label="Tutorial">
      <div className="onboarding-card">
        <h3>{current.title}</h3>
        <p>{current.body}</p>
        <div className="onboarding-dots">
          {STEPS.map((_, i) => (
            <span key={i} className={i === step ? "dot active" : "dot"} />
          ))}
        </div>
        <div className="onboarding-actions">
          <button onClick={() => setDone(true)}>Skip</button>
          <button
            className="primary"
            onClick={() => (last ? setDone(true) : setStep((s) => s + 1))}
          >
            {last ? "Get started" : "Next"}
          </button>
        </div>
      </div>
    </div>
  );
}
