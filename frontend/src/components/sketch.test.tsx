import { render, screen, fireEvent, act } from "@testing-library/react";
import { SketchEditor } from "../components/SketchEditor";
import { Onboarding } from "../components/Onboarding";
import { useSketchStore } from "../state/sketchStore";

describe("SketchEditor", () => {
  it("is hidden until opened", () => {
    act(() => useSketchStore.getState().closeEditor());
    const { container } = render(<SketchEditor />);
    expect(container.querySelector(".sketch-editor")).toBeNull();
  });

  it("opens and draws a line", () => {
    const store = useSketchStore.getState();
    act(() => {
      store.openEditor();
      store.setTool("line");
    });
    render(<SketchEditor />);
    expect(screen.getByText("Sketch")).toBeTruthy();

    act(() => {
      store.beginPoint({ x: 10, y: 10 });
      store.updateDraft({ x: 50, y: 50 });
      store.commitDraft();
    });
    expect(useSketchStore.getState().entities.length).toBe(1);

    act(() => store.clear());
    expect(useSketchStore.getState().entities.length).toBe(0);
  });
});

describe("Onboarding", () => {
  it("advances through steps and finishes", () => {
    render(<Onboarding />);
    expect(screen.getByText("Welcome to TPT Vertex")).toBeTruthy();
    const next = screen.getByText("Next");
    fireEvent.click(next);
    expect(screen.getByText("Build in the feature tree")).toBeTruthy();
    fireEvent.click(screen.getByText("Skip"));
    expect(screen.queryByText("Welcome to TPT Vertex")).toBeNull();
  });
});
