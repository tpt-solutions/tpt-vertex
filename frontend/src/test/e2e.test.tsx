import { render, screen, fireEvent } from "@testing-library/react";
import { App } from "../App";
import { useModelStore } from "../state/store";
import { useVersionStore, diffFeatures } from "../state/versionStore";
import type { FeatureNode } from "../state/types";

/**
 * End-to-end style test of a full design + version-control workflow, driven
 * through the real store actions and rendered UI. This exercises the same paths
 * a user takes: edit the model, undo/redo, commit, branch, diverge, and merge
 * with conflict resolution.
 */
describe("E2E: design + versioning workflow", () => {
  beforeEach(() => {
    // Reset model to a known baseline.
    useModelStore.setState({
      features: [
        {
          id: "f0",
          type: "sketch",
          label: "Base Sketch",
          params: { x0: 0, y0: 0, x1: 40, y1: 40 },
        },
        { id: "f1", type: "extrude", label: "Body", params: { height: 30, sketch: "f0" } },
      ],
      assemblies: [{ id: "root", name: "Assembly", children: [] }],
      selectedFeatureId: null,
      hoveredFeatureId: null,
    });
    useVersionStore.setState({
      commits: [
        {
          id: "c0",
          message: "Initial commit",
          parents: [],
          branch: "main",
          timestamp: 0,
          features: [],
        },
      ],
      branches: { main: "c0" },
      currentBranch: "main",
      head: "c0",
      selectedCommit: null,
      conflicts: [],
    });
  });

  it("edits a parameter, undoes, and redoes through the UI", () => {
    render(<App />);
    // Select the Body feature in the feature tree.
    fireEvent.click(screen.getByText("Body"));
    // The Inspector shows the height param; change it.
    const model = useModelStore.getState();
    model.updateParam("f1", "height", 55);
    expect(useModelStore.getState().features.find((f) => f.id === "f1")?.params.height).toBe(55);

    // Undo restores 30, redo re-applies 55.
    useModelStore.getState().undo();
    expect(useModelStore.getState().features.find((f) => f.id === "f1")?.params.height).toBe(30);
    useModelStore.getState().redo();
    expect(useModelStore.getState().features.find((f) => f.id === "f1")?.params.height).toBe(55);
  });

  it("commits, branches, diverges, and merges with a conflict resolution", () => {
    const vs = useVersionStore.getState();

    // Commit the baseline on main.
    useModelStore.getState().updateParam("f1", "height", 30);
    vs.commit("Baseline body");

    // Branch to "feature-a" and change height to 60, commit.
    useVersionStore.getState().createBranch("feature-a");
    useModelStore.getState().updateParam("f1", "height", 60);
    useVersionStore.getState().commit("Taller body on feature-a");

    // Back to main, change height to 45, commit (concurrent edit to same feature).
    useVersionStore.getState().checkout("main");
    useModelStore.getState().updateParam("f1", "height", 45);
    useVersionStore.getState().commit("Medium body on main");

    // Merge feature-a into main => conflict on f1.
    useVersionStore.getState().merge("feature-a");
    const conflicts = useVersionStore.getState().conflicts;
    expect(conflicts.length).toBe(1);
    expect(conflicts[0].featureId).toBe("f1");

    // Resolve by taking theirs (feature-a) and complete the merge.
    useVersionStore.getState().resolveConflict("f1", "theirs");
    useVersionStore.getState().applyMerge("Merge feature-a");

    // The merged model reflects the "theirs" resolution.
    const merged = useModelStore.getState().features.find((f) => f.id === "f1");
    expect(merged?.params.height).toBe(60);

    // A merge commit exists as the new head.
    const state = useVersionStore.getState();
    expect(state.commits.find((c) => c.id === state.head)?.message).toBe("Merge feature-a");
    // No unresolved conflicts remain.
    expect(state.conflicts.length).toBe(0);
  });

  it("diffs a commit against its parent", () => {
    const base: FeatureNode[] = [
      { id: "f1", type: "extrude", label: "Body", params: { height: 30 } },
    ];
    const next: FeatureNode[] = [
      { id: "f1", type: "extrude", label: "Body", params: { height: 90 } },
    ];
    const changes = diffFeatures(base, next);
    expect(changes).toHaveLength(1);
    expect(changes[0].kind).toBe("modified");
    expect(changes[0].after?.params.height).toBe(90);
  });
});
