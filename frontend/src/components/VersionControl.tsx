import { useMemo, useState } from "react";
import { useVersionStore, diffFeatures } from "../state/versionStore";
import type { FeatureNode } from "../state/types";

/** Timeline of commits with branch labels (Phase 5 history UI). */
export function HistoryPanel() {
  const commits = useVersionStore((s) => s.commits);
  const branches = useVersionStore((s) => s.branches);
  const head = useVersionStore((s) => s.head);
  const selected = useVersionStore((s) => s.selectedCommit);
  const selectCommit = useVersionStore((s) => s.selectCommit);

  const branchTips = useMemo(() => {
    const m: Record<string, string[]> = {};
    for (const [name, tip] of Object.entries(branches)) {
      (m[tip] ??= []).push(name);
    }
    return m;
  }, [branches]);

  // Newest first.
  const ordered = [...commits].reverse();

  return (
    <section className="panel history-timeline" aria-label="Commit history">
      <h2 className="panel-title">History</h2>
      <ul className="timeline">
        {ordered.map((c) => (
          <li
            key={c.id}
            className={
              "timeline-item" +
              (c.id === selected ? " selected" : "") +
              (c.id === head ? " head" : "")
            }
            onClick={() => selectCommit(c.id)}
          >
            <span className="timeline-dot" />
            <div className="timeline-body">
              <div className="timeline-message">{c.message}</div>
              <div className="timeline-meta">
                <span className="mono">{c.id}</span>
                {(branchTips[c.id] ?? []).map((b) => (
                  <span key={b} className="branch-tag">
                    {b}
                  </span>
                ))}
                {c.parents.length > 1 && <span className="branch-tag merge">merge</span>}
              </div>
            </div>
          </li>
        ))}
      </ul>
    </section>
  );
}

/** Before/after diff of the selected commit against its first parent. */
export function DiffViewer() {
  const commits = useVersionStore((s) => s.commits);
  const selected = useVersionStore((s) => s.selectedCommit);

  const changes = useMemo(() => {
    const commit = commits.find((c) => c.id === selected);
    if (!commit) return null;
    const parent = commit.parents[0] ? commits.find((c) => c.id === commit.parents[0]) : undefined;
    return diffFeatures(parent?.features ?? [], commit.features);
  }, [commits, selected]);

  if (!selected) {
    return (
      <section className="panel diff-viewer" aria-label="Diff viewer">
        <h2 className="panel-title">Changes</h2>
        <p className="muted">Select a commit to view its changes.</p>
      </section>
    );
  }

  return (
    <section className="panel diff-viewer" aria-label="Diff viewer">
      <h2 className="panel-title">Changes</h2>
      {changes && changes.length === 0 && <p className="muted">No geometry changes.</p>}
      <ul className="diff-list">
        {changes?.map((ch) => (
          <li key={ch.featureId} className={`diff-row ${ch.kind}`}>
            <span className={`diff-badge ${ch.kind}`}>{ch.kind}</span>
            <span className="diff-label">{ch.label}</span>
            {ch.kind === "modified" && <ParamDelta before={ch.before} after={ch.after} />}
          </li>
        ))}
      </ul>
    </section>
  );
}

function ParamDelta({ before, after }: { before?: FeatureNode; after?: FeatureNode }) {
  if (!before || !after) return null;
  const keys = Array.from(new Set([...Object.keys(before.params), ...Object.keys(after.params)]));
  return (
    <div className="param-delta">
      {keys
        .filter((k) => before.params[k] !== after.params[k])
        .map((k) => (
          <div key={k} className="param-delta-row">
            <span className="mono">{k}</span>
            <span className="old">{String(before.params[k] ?? "—")}</span>
            <span className="arrow">→</span>
            <span className="new">{String(after.params[k] ?? "—")}</span>
          </div>
        ))}
    </div>
  );
}

/** Merge-conflict resolution UI for concurrent feature edits. */
export function MergeConflictPanel() {
  const conflicts = useVersionStore((s) => s.conflicts);
  const resolveConflict = useVersionStore((s) => s.resolveConflict);
  const applyMerge = useVersionStore((s) => s.applyMerge);

  if (conflicts.length === 0) return null;
  const allResolved = conflicts.every((c) => c.resolution);

  return (
    <section className="panel merge-conflicts" aria-label="Merge conflicts">
      <h2 className="panel-title">Merge Conflicts</h2>
      <ul className="conflict-list">
        {conflicts.map((c) => (
          <li key={c.featureId} className="conflict-row">
            <div className="conflict-title">{c.label}</div>
            <div className="conflict-choices">
              <button
                className={c.resolution === "ours" ? "active" : ""}
                onClick={() => resolveConflict(c.featureId, "ours")}
              >
                Keep ours
              </button>
              <button
                className={c.resolution === "theirs" ? "active" : ""}
                onClick={() => resolveConflict(c.featureId, "theirs")}
              >
                Take theirs
              </button>
            </div>
          </li>
        ))}
      </ul>
      <button
        className="primary"
        disabled={!allResolved}
        onClick={() => applyMerge("Merge branch")}
      >
        {allResolved ? "Complete merge" : "Resolve all conflicts"}
      </button>
    </section>
  );
}

/** The version-control modal shell that hosts history, diff, and merge UIs. */
export function VersionControl({ onClose }: { onClose: () => void }) {
  const commit = useVersionStore((s) => s.commit);
  const createBranch = useVersionStore((s) => s.createBranch);
  const checkout = useVersionStore((s) => s.checkout);
  const merge = useVersionStore((s) => s.merge);
  const branches = useVersionStore((s) => s.branches);
  const currentBranch = useVersionStore((s) => s.currentBranch);
  const [message, setMessage] = useState("");
  const [branchName, setBranchName] = useState("");
  const [mergeFrom, setMergeFrom] = useState("");

  return (
    <div className="vc-backdrop" role="dialog" aria-label="Version control">
      <div className="vc-card">
        <header className="vc-header">
          <h3>Version Control</h3>
          <span className="branch-tag current">{currentBranch}</span>
          <div className="spacer" />
          <button onClick={onClose} aria-label="Close">
            Close
          </button>
        </header>

        <div className="vc-actions">
          <input
            placeholder="Commit message"
            value={message}
            onChange={(e) => setMessage(e.target.value)}
            aria-label="Commit message"
          />
          <button
            onClick={() => {
              if (message.trim()) {
                commit(message.trim());
                setMessage("");
              }
            }}
          >
            Commit
          </button>
          <input
            placeholder="New branch"
            value={branchName}
            onChange={(e) => setBranchName(e.target.value)}
            aria-label="New branch name"
          />
          <button
            onClick={() => {
              if (branchName.trim()) {
                createBranch(branchName.trim());
                setBranchName("");
              }
            }}
          >
            Branch
          </button>
          <select
            value=""
            onChange={(e) => e.target.value && checkout(e.target.value)}
            aria-label="Checkout branch"
          >
            <option value="">Checkout…</option>
            {Object.keys(branches).map((b) => (
              <option key={b} value={b}>
                {b}
              </option>
            ))}
          </select>
          <select
            value={mergeFrom}
            onChange={(e) => setMergeFrom(e.target.value)}
            aria-label="Merge from branch"
          >
            <option value="">Merge from…</option>
            {Object.keys(branches)
              .filter((b) => b !== currentBranch)
              .map((b) => (
                <option key={b} value={b}>
                  {b}
                </option>
              ))}
          </select>
          <button onClick={() => mergeFrom && merge(mergeFrom)}>Merge</button>
        </div>

        <div className="vc-body">
          <HistoryPanel />
          <DiffViewer />
        </div>
        <MergeConflictPanel />
      </div>
    </div>
  );
}
