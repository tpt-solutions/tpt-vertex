import { render, screen, fireEvent } from "@testing-library/react";
import { SlicerPanel } from "../components/SlicerPanel";
import {
  sliceModel,
  modelBox,
  DEFAULT_SLICE_SETTINGS,
} from "../geometry/slicer";
import type { FeatureNode } from "../state/types";

const box: FeatureNode[] = [
  { id: "f0", type: "sketch", label: "Base", params: { x0: 0, y0: 0, x1: 20, y1: 10 } },
  { id: "f1", type: "extrude", label: "Body", params: { height: 6, sketch: "f0" } },
];

describe("sliceModel", () => {
  it("reads the model footprint and height", () => {
    const b = modelBox(box);
    expect(b.width).toBe(20);
    expect(b.depth).toBe(10);
    expect(b.height).toBe(6);
  });

  it("produces layers, perimeters, infill and gcode", () => {
    const r = sliceModel(box, DEFAULT_SLICE_SETTINGS);
    // height 6 / layer 0.2 => ~30 layers.
    expect(r.layerCount).toBeGreaterThan(20);
    expect(r.layers.every((l) => l.perimeters.length >= 1)).toBe(true);
    expect(r.gcode).toContain("G1 X");
    expect(r.estimatedFilamentMm).toBeGreaterThan(0);
    expect(r.size).toEqual([20, 10, 6]);
  });

  it("more walls reduce the infill region", () => {
    const few = sliceModel(box, { ...DEFAULT_SLICE_SETTINGS, wallCount: 1 });
    const many = sliceModel(box, { ...DEFAULT_SLICE_SETTINGS, wallCount: 4 });
    const fewInfill = few.layers[5].infill.length;
    const manyInfill = many.layers[5].infill.length;
    expect(manyInfill).toBeLessThanOrEqual(fewInfill);
  });

  it("zero infill density yields no infill lines", () => {
    const r = sliceModel(box, { ...DEFAULT_SLICE_SETTINGS, infillDensity: 0 });
    expect(r.layers.every((l) => l.infill.length === 0)).toBe(true);
  });
});

describe("SlicerPanel", () => {
  it("slices on demand and shows estimates", () => {
    render(<SlicerPanel onClose={() => {}} />);
    expect(screen.getByText("Adjust settings and press Slice.")).toBeTruthy();
    fireEvent.click(screen.getByText("Slice"));
    // Estimates section now shows a layer count row.
    expect(screen.getByText("Layers")).toBeTruthy();
    expect(screen.getByLabelText(/Layer selector/)).toBeTruthy();
  });

  it("closes when Close is pressed", () => {
    let closed = false;
    render(<SlicerPanel onClose={() => (closed = true)} />);
    fireEvent.click(screen.getByLabelText("Close"));
    expect(closed).toBe(true);
  });
});
