import { render, screen } from "@testing-library/react";
import { HistoryPanel, DiffViewer } from "../components/VersionControl";
import { useVersionStore, diffFeatures, mergeBase } from "../state/versionStore";
import type { FeatureNode } from "../state/types";

function reset() {
  // Re-seed the store to a known single-commit state.
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
}

describe("versionStore.diffFeatures", () => {
  it("detects added, removed, and modified features", () => {
    const base: FeatureNode[] = [
      { id: "a", type: "extrude", label: "A", params: { h: 1 } },
      { id: "b", type: "extrude", label: "B", params: { h: 2 } },
    ];
    const other: FeatureNode[] = [
      { id: "a", type: "extrude", label: "A", params: { h: 5 } }, // modified
      { id: "c", type: "sketch", label: "C", params: {} }, // added
    ];
    const changes = diffFeatures(base, other);
    const kinds = changes.map((c) => `${c.kind}:${c.featureId}`).sort();
    expect(kinds).toEqual(["added:c", "modified:a", "removed:b"]);
  });
});

describe("versionStore.mergeBase", () => {
  it("finds the lowest common ancestor", () => {
    const commits = [
      { id: "c0", message: "", parents: [], branch: "main", timestamp: 0, features: [] },
      { id: "c1", message: "", parents: ["c0"], branch: "main", timestamp: 0, features: [] },
      { id: "c2", message: "", parents: ["c0"], branch: "dev", timestamp: 0, features: [] },
    ];
    expect(mergeBase(commits, "c1", "c2")?.id).toBe("c0");
  });
});

describe("HistoryPanel", () => {
  it("renders commits newest-first with branch tags", () => {
    reset();
    useVersionStore.getState().commit("Second commit");
    render(<HistoryPanel />);
    expect(screen.getByText("Second commit")).toBeTruthy();
    expect(screen.getByText("main")).toBeTruthy();
  });
});

describe("DiffViewer", () => {
  it("prompts when no commit is selected", () => {
    reset();
    render(<DiffViewer />);
    expect(screen.getByText(/Select a commit/)).toBeTruthy();
  });

  it("shows changes for the selected commit", () => {
    reset();
    // Add a feature to the model and commit so there is a diff vs parent.
    useVersionStore.setState((s) => ({
      commits: [
        ...s.commits,
        {
          id: "c9",
          message: "Add body",
          parents: ["c0"],
          branch: "main",
          timestamp: 1,
          features: [{ id: "x", type: "extrude", label: "Body", params: { height: 3 } }],
        },
      ],
    }));
    useVersionStore.getState().selectCommit("c9");
    render(<DiffViewer />);
    expect(screen.getByText("added")).toBeTruthy();
    expect(screen.getByText("Body")).toBeTruthy();
  });
});

describe("merge conflict resolution", () => {
  it("records a resolution choice", () => {
    reset();
    useVersionStore.setState({
      conflicts: [
        {
          featureId: "x",
          label: "Body",
          ours: { id: "x", type: "extrude", label: "Body", params: { h: 1 } },
          theirs: { id: "x", type: "extrude", label: "Body", params: { h: 9 } },
        },
      ],
    });
    useVersionStore.getState().resolveConflict("x", "theirs");
    expect(useVersionStore.getState().conflicts[0].resolution).toBe("theirs");
  });
});
