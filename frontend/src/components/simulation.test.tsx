import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { SimulationPanel } from "../components/SimulationPanel";
import { quatFromAxisAngle, runMotionFrame, runStaticAnalysis } from "../geometry/simulation";

describe("geometry/simulation", () => {
  it("quatFromAxisAngle matches a 90° rotation about Z", () => {
    const q = quatFromAxisAngle([0, 0, 1], Math.PI / 2);
    // w = cos(45°) ~ 0.7071, z = sin(45°) ~ 0.7071
    expect(q[0]).toBeCloseTo(Math.cos(Math.PI / 4), 6);
    expect(q[3]).toBeCloseTo(Math.sin(Math.PI / 4), 6);
    expect(Math.hypot(q[0], q[1], q[2], q[3])).toBeCloseTo(1, 6);
  });

  it("runStaticAnalysis returns a mesh + stress field", async () => {
    const r = await runStaticAnalysis({
      material: "Steel",
      fixedNodes: [],
      loads: [],
      maxTetEdge: 5,
    });
    expect(r.nodes.length).toBeGreaterThan(0);
    expect(r.tets.length % 4).toBe(0);
    expect(r.vonMises.length).toBe(r.tets.length / 4);
  });

  it("runMotionFrame rotates the mover by the requested angle", async () => {
    const r = await runMotionFrame({
      axis: [0, 0, 1],
      angle: Math.PI / 2,
      anchor: [0, 0, 0],
    });
    const mover = r.parts.find((p) => p.name === "mover");
    expect(mover).toBeDefined();
    expect(mover!.rotation[0]).toBeCloseTo(Math.cos(Math.PI / 4), 6);
  });
});

describe("SimulationPanel", () => {
  it("opens and exposes the analysis + motion controls", async () => {
    render(<SimulationPanel onClose={() => {}} />);
    expect(screen.getByLabelText("Simulation")).toBeInTheDocument();
    expect(screen.getByLabelText("Simulation setup")).toBeInTheDocument();
    expect(screen.getByLabelText("Motion study")).toBeInTheDocument();

    fireEvent.click(screen.getByLabelText("Run analysis"));
    await waitFor(() =>
      expect(screen.getByLabelText("Stress results")).toBeInTheDocument(),
    );
    // A static-analysis result (even the local fallback) populates the legend.
    expect(screen.getByText(/Max von Mises/i)).toBeInTheDocument();
  });
});
