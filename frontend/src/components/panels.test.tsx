import { render, screen, fireEvent } from "@testing-library/react";
import { FeatureTreePanel } from "../components/FeatureTreePanel";
import { PropertiesPanel } from "../components/PropertiesPanel";
import { useModelStore } from "../state/store";

describe("FeatureTreePanel", () => {
  it("lists features and selects on click", () => {
    render(<FeatureTreePanel />);
    const items = screen.getAllByText(/Base Sketch|Body/);
    expect(items.length).toBeGreaterThan(0);
    fireEvent.click(items[0]);
    expect(useModelStore.getState().selectedFeatureId).not.toBeNull();
  });
});

describe("PropertiesPanel", () => {
  it("shows params for the selected feature", () => {
    const id = useModelStore.getState().features[0].id;
    useModelStore.getState().setSelected(id);
    render(<PropertiesPanel featureId={id} />);
    expect(screen.getByText("Base Sketch")).toBeTruthy();
  });

  it("prompts when nothing is selected", () => {
    useModelStore.getState().setSelected(null);
    render(<PropertiesPanel featureId={null} />);
    expect(screen.getByText(/No feature selected/)).toBeTruthy();
  });
});
