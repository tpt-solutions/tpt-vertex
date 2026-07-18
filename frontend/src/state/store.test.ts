import { useModelStore } from "../state/store";

describe("model store", () => {
  it("updates a feature parameter", () => {
    const { features, updateParam } = useModelStore.getState();
    const id = features[0].id;
    updateParam(id, "x1", 99);
    const updated = useModelStore.getState().features.find((f) => f.id === id);
    expect(updated?.params.x1).toBe(99);
  });

  it("supports undo/redo of param changes", () => {
    const { features, updateParam, undo, redo } = useModelStore.getState();
    const id = features[0].id;
    const before = features[0].params.x1;
    updateParam(id, "x1", 123);
    expect(useModelStore.getState().features[0].params.x1).toBe(123);
    undo();
    expect(useModelStore.getState().features[0].params.x1).toBe(before);
    redo();
    expect(useModelStore.getState().features[0].params.x1).toBe(123);
  });

  it("adds features", () => {
    const start = useModelStore.getState().features.length;
    useModelStore.getState().addFeature({
      id: "fx",
      type: "extrude",
      label: "New",
      params: { height: 10 },
    });
    expect(useModelStore.getState().features.length).toBe(start + 1);
  });
});
