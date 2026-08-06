import { render, screen, fireEvent } from "@testing-library/react";
import { FeatureTreePanel } from "../components/FeatureTreePanel";
import { PropertiesPanel } from "../components/PropertiesPanel";
import { CapabilityStatusPanel } from "../components/CapabilityStatusPanel";
import { useModelStore } from "../state/store";
import { CAPABILITIES, capabilitiesByPhase, getCapability } from "../state/capabilityStatus";

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

describe("capability status manifest", () => {
  it("marks booleans and fillet/chamfer as placeholders", () => {
    expect(getCapability("boolean-ops")?.status).toBe("placeholder");
    expect(getCapability("fillet-chamfer")?.status).toBe("placeholder");
    expect(getCapability("kernel-math")?.status).toBe("real");
    expect(getCapability("nope")).toBeUndefined();
  });

  it("groups every capability by phase", () => {
    const grouped = capabilitiesByPhase();
    const total = Object.values(grouped).reduce((n, list) => n + list.length, 0);
    expect(total).toBe(CAPABILITIES.length);
  });
});

describe("CapabilityStatusPanel", () => {
  it("lists capabilities and closes", () => {
    let closed = false;
    render(<CapabilityStatusPanel onClose={() => (closed = true)} />);
    expect(screen.getByText(/Boolean operations/)).toBeTruthy();
    expect(screen.getAllByText("placeholder").length).toBeGreaterThan(0);
    fireEvent.click(screen.getByLabelText("Close"));
    expect(closed).toBe(true);
  });
});

describe("feature tree badges", () => {
  it("badges boolean features as placeholder", () => {
    useModelStore.getState().addFeature({
      id: "f-bool-test",
      type: "boolean",
      label: "Cut Pocket",
      params: {},
    });
    render(<FeatureTreePanel />);
    expect(screen.getAllByText("placeholder").length).toBeGreaterThan(0);
  });
});
