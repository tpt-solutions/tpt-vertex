import { create } from "zustand";
import type { FeatureNode, ModelState, SelectionState } from "./types";

interface StoreState extends ModelState, SelectionState {
  addFeature: (feature: FeatureNode) => void;
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
