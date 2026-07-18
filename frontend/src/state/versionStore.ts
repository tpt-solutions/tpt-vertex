import { create } from "zustand";
import type { FeatureNode } from "./types";
import { useModelStore } from "./store";

/**
 * Client-side mirror of the `tpt-vertex-versioning` model: commits form a DAG
 * with branches (named refs), and diffs/merges operate on the feature manifest
 * at feature granularity (see ADR-0005). This store powers the history timeline,
 * diff viewer, and merge-conflict resolution UI without requiring the WASM
 * kernel to be wired in.
 */

export type ChangeKind = "added" | "removed" | "modified";

export interface FeatureChange {
  kind: ChangeKind;
  featureId: string;
  label: string;
  /** Parameter-level before/after for modified features. */
  before?: FeatureNode;
  after?: FeatureNode;
}

export interface Commit {
  id: string;
  message: string;
  parents: string[];
  branch: string;
  timestamp: number;
  /** Snapshot of the feature list at this commit. */
  features: FeatureNode[];
}

export interface MergeConflict {
  featureId: string;
  label: string;
  ours: FeatureNode;
  theirs: FeatureNode;
  base?: FeatureNode;
  /** Resolution chosen by the user, if any. */
  resolution?: "ours" | "theirs";
}

interface VersionState {
  commits: Commit[];
  branches: Record<string, string>; // name -> tip commit id
  currentBranch: string;
  head: string;
  selectedCommit: string | null;
  conflicts: MergeConflict[];

  commit: (message: string) => void;
  createBranch: (name: string) => void;
  checkout: (branch: string) => void;
  selectCommit: (id: string | null) => void;
  merge: (fromBranch: string) => void;
  resolveConflict: (featureId: string, pick: "ours" | "theirs") => void;
  applyMerge: (message: string) => void;
}

let counter = 0;
function nextId(): string {
  counter += 1;
  return `c${counter}`;
}

function snapshotFeatures(): FeatureNode[] {
  return useModelStore.getState().features.map((f) => ({
    ...f,
    params: { ...f.params },
  }));
}

/** Feature-granular diff between two snapshots (mirrors Diff::between). */
export function diffFeatures(base: FeatureNode[], other: FeatureNode[]): FeatureChange[] {
  const baseById = new Map(base.map((f) => [f.id, f]));
  const otherById = new Map(other.map((f) => [f.id, f]));
  const changes: FeatureChange[] = [];

  for (const f of other) {
    const b = baseById.get(f.id);
    if (!b) {
      changes.push({ kind: "added", featureId: f.id, label: f.label, after: f });
    } else if (paramHash(b) !== paramHash(f)) {
      changes.push({
        kind: "modified",
        featureId: f.id,
        label: f.label,
        before: b,
        after: f,
      });
    }
  }
  for (const b of base) {
    if (!otherById.has(b.id)) {
      changes.push({ kind: "removed", featureId: b.id, label: b.label, before: b });
    }
  }
  return changes;
}

function paramHash(f: FeatureNode): string {
  return f.type + "|" + JSON.stringify(f.params);
}

const rootFeatures = snapshotFeatures();
const rootCommit: Commit = {
  id: "c0",
  message: "Initial commit",
  parents: [],
  branch: "main",
  timestamp: Date.now(),
  features: rootFeatures,
};

export const useVersionStore = create<VersionState>((set, get) => ({
  commits: [rootCommit],
  branches: { main: "c0" },
  currentBranch: "main",
  head: "c0",
  selectedCommit: null,
  conflicts: [],

  commit: (message) => {
    const id = nextId();
    const state = get();
    const commit: Commit = {
      id,
      message,
      parents: [state.head],
      branch: state.currentBranch,
      timestamp: Date.now(),
      features: snapshotFeatures(),
    };
    set({
      commits: [...state.commits, commit],
      branches: { ...state.branches, [state.currentBranch]: id },
      head: id,
    });
  },

  createBranch: (name) => {
    const state = get();
    if (state.branches[name]) return;
    set({
      branches: { ...state.branches, [name]: state.head },
      currentBranch: name,
    });
  },

  checkout: (branch) => {
    const state = get();
    const tip = state.branches[branch];
    if (!tip) return;
    set({ currentBranch: branch, head: tip });
  },

  selectCommit: (id) => set({ selectedCommit: id }),

  merge: (fromBranch) => {
    const state = get();
    const oursTip = state.branches[state.currentBranch];
    const theirsTip = state.branches[fromBranch];
    if (!oursTip || !theirsTip) return;

    const ours = state.commits.find((c) => c.id === oursTip)!;
    const theirs = state.commits.find((c) => c.id === theirsTip)!;
    const baseCommit = mergeBase(state.commits, oursTip, theirsTip);
    const base = baseCommit?.features ?? [];

    const ourChanges = diffFeatures(base, ours.features);
    const theirChanges = diffFeatures(base, theirs.features);
    const ourById = new Map(ourChanges.map((c) => [c.featureId, c]));

    const conflicts: MergeConflict[] = [];
    for (const t of theirChanges) {
      const o = ourById.get(t.featureId);
      if (o && (o.after || o.before) && (t.after || t.before)) {
        // Both sides touched the same feature => conflict.
        conflicts.push({
          featureId: t.featureId,
          label: t.label,
          ours: (o.after ?? o.before)!,
          theirs: (t.after ?? t.before)!,
          base: base.find((f) => f.id === t.featureId),
        });
      }
    }
    set({ conflicts });
  },

  resolveConflict: (featureId, pick) => {
    set((s) => ({
      conflicts: s.conflicts.map((c) =>
        c.featureId === featureId ? { ...c, resolution: pick } : c,
      ),
    }));
  },

  applyMerge: (message) => {
    const state = get();
    if (state.conflicts.some((c) => !c.resolution)) return; // unresolved
    // Build merged feature set: apply chosen resolutions onto current features.
    const merged = useModelStore.getState().features.map((f) => {
      const c = state.conflicts.find((x) => x.featureId === f.id);
      if (c && c.resolution === "theirs") return c.theirs;
      return f;
    });
    useModelStore.setState({ features: merged });

    const id = nextId();
    const commit: Commit = {
      id,
      message,
      parents: [state.head],
      branch: state.currentBranch,
      timestamp: Date.now(),
      features: merged.map((f) => ({ ...f, params: { ...f.params } })),
    };
    set({
      commits: [...state.commits, commit],
      branches: { ...state.branches, [state.currentBranch]: id },
      head: id,
      conflicts: [],
    });
  },
}));

/** Lowest common ancestor of two commits (mirrors Repository::merge_base). */
export function mergeBase(commits: Commit[], a: string, b: string): Commit | undefined {
  const byId = new Map(commits.map((c) => [c.id, c]));
  const ancestors = (start: string): Set<string> => {
    const seen = new Set<string>();
    const stack = [start];
    while (stack.length) {
      const id = stack.pop()!;
      if (seen.has(id)) continue;
      seen.add(id);
      const c = byId.get(id);
      if (c) stack.push(...c.parents);
    }
    return seen;
  };
  const aAnc = ancestors(a);
  // Walk b's ancestry breadth-first; first hit in aAnc is the merge base.
  const queue = [b];
  const visited = new Set<string>();
  while (queue.length) {
    const id = queue.shift()!;
    if (visited.has(id)) continue;
    visited.add(id);
    if (aAnc.has(id)) return byId.get(id);
    const c = byId.get(id);
    if (c) queue.push(...c.parents);
  }
  return undefined;
}
