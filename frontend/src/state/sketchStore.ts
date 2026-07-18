import { create } from "zustand";

export type SketchTool = "select" | "line" | "circle";

export interface SketchPoint {
  x: number;
  y: number;
}

export interface SketchEntity {
  id: string;
  kind: "line" | "circle";
  /** line: [a, b]; circle: [center, rim] */
  points: [SketchPoint, SketchPoint];
}

interface SketchStoreState {
  open: boolean;
  tool: SketchTool;
  entities: SketchEntity[];
  draft: [SketchPoint, SketchPoint] | null;
  openEditor: () => void;
  closeEditor: () => void;
  setTool: (t: SketchTool) => void;
  beginPoint: (p: SketchPoint) => void;
  updateDraft: (p: SketchPoint) => void;
  commitDraft: () => void;
  clear: () => void;
}

let counter = 0;

export const useSketchStore = create<SketchStoreState>((set, get) => ({
  open: false,
  tool: "line",
  entities: [],
  draft: null,

  openEditor: () => set({ open: true }),
  closeEditor: () => set({ open: false, draft: null }),
  setTool: (tool) => set({ tool, draft: null }),

  beginPoint: (p) => {
    const { tool, draft } = get();
    if (tool === "select" || draft) return;
    set({ draft: [p, p] });
  },

  updateDraft: (p) => {
    const { draft } = get();
    if (!draft) return;
    set({ draft: [draft[0], p] });
  },

  commitDraft: () => {
    const { draft, tool } = get();
    if (!draft) return;
    const dist = Math.hypot(draft[1].x - draft[0].x, draft[1].y - draft[0].y);
    if (dist < 1) {
      set({ draft: null });
      return;
    }
    counter += 1;
    set((s) => ({
      entities: [
        ...s.entities,
        {
          id: `s${counter}`,
          kind: tool === "circle" ? "circle" : "line",
          points: draft,
        },
      ],
      draft: null,
    }));
  },

  clear: () => set({ entities: [], draft: null }),
}));
