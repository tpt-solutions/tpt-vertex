import { create } from "zustand";
import type { FeatureNode, ModelState, SelectionState } from "./types";
import { useSketchStore } from "./sketchStore";

interface StoreState extends ModelState, SelectionState {
  addFeature: (feature: FeatureNode) => void;
  commitSketch: () => void;
  setSelected: (id: string | null) => void;
  setHovered: (id: string | null) => void;
  updateParam: (id: string, key: string, value: number | string) => void;
  undo: () => void;
  redo: () => void;
}

interface HistoryEntry {
  features: FeatureNode[];
}

const initialFeatures: FeatureNode[] = [
  {
    id: "f0",
    type: "sketch",
    label: "Base Sketch",
    params: { x0: 0, y0: 0, x1: 40, y1: 40 },
  },
  {
    id: "f1",
    type: "extrude",
    label: "Body",
    params: { height: 30, sketch: "f0" },
  },
];

export const useModelStore = create<StoreState>((set, get) => {
  const past: HistoryEntry[] = [];
  const future: HistoryEntry[] = [];

  const snapshot = () => ({ features: get().features.map((f) => ({ ...f })) });

  return {
    features: initialFeatures,
    assemblies: [{ id: "root", name: "Assembly", children: [] }],
    selectedFeatureId: null,
    hoveredFeatureId: null,

    addFeature: (feature) => {
      past.push(snapshot());
      future.length = 0;
      set((s) => ({ features: [...s.features, feature] }));
    },

    commitSketch: () => {
      const entities = useSketchStore.getState().entities;
      if (entities.length === 0) return;
      let minX = Infinity;
      let minY = Infinity;
      let maxX = -Infinity;
      let maxY = -Infinity;
      for (const e of entities) {
        for (const p of e.points) {
          minX = Math.min(minX, p.x);
          minY = Math.min(minY, p.y);
          maxX = Math.max(maxX, p.x);
          maxY = Math.max(maxY, p.y);
        }
      }
      if (!isFinite(minX)) return;
      past.push(snapshot());
      future.length = 0;
      const sketchId = `sk${Date.now()}`;
      const bodyId = `f${Date.now()}`;
      const sketch: FeatureNode = {
        id: sketchId,
        type: "sketch",
        label: "Sketch",
        params: { x0: minX, y0: minY, x1: maxX, y1: maxY },
      };
      const body: FeatureNode = {
        id: bodyId,
        type: "extrude",
        label: "Body from sketch",
        params: { height: 30, sketch: sketchId },
      };
      set((s) => ({ features: [...s.features, sketch, body] }));
      useSketchStore.getState().clear();
      useSketchStore.getState().closeEditor();
    },

    setSelected: (id) => set({ selectedFeatureId: id }),
    setHovered: (id) => set({ hoveredFeatureId: id }),

    updateParam: (id, key, value) => {
      past.push(snapshot());
      future.length = 0;
      set((s) => ({
        features: s.features.map((f) =>
          f.id === id ? { ...f, params: { ...f.params, [key]: value } } : f,
        ),
      }));
    },

    undo: () => {
      const prev = past.pop();
      if (!prev) return;
      future.push(snapshot());
      set({ features: prev.features });
    },

    redo: () => {
      const next = future.pop();
      if (!next) return;
      past.push(snapshot());
      set({ features: next.features });
    },
  };
});
