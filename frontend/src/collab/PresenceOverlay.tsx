/**
 * Presence / multi-cursor overlay for the collaboration client (Phase 13).
 *
 * `<CollabLayer />` is a self-contained, absolutely-positioned layer that draws
 * one labelled dot per remote collaborator. It subscribes to a `CollabClient`'s
 * remote-presence callback and never imports from `App`/`Viewport`, so it can be
 * dropped into any positioned container (e.g. the `.viewport` wrapper):
 *
 * ```tsx
 * <div style={{ position: "relative" }}>
 *   <Viewport />
 *   <CollabLayer />
 * </div>
 * ```
 *
 * Cursor coordinates are the first two components of `Presence.cursor`,
 * interpreted as **normalized viewport coordinates** (`0..1`, origin top-left) —
 * the same convention `CollabClient.setLocalCursor(x, y)` publishes. Keeping
 * them normalized means the overlay does not need the camera or canvas size.
 *
 * The layer is `pointer-events: none` throughout, so it never steals clicks
 * from the 3D viewport, and renders nothing at all when no one else is present.
 */
import { useMemo } from "react";

import type { CollabClient, CollabStatus, RemotePeer } from "./client";
import { useCollabStatus, useRemotePeers } from "./usePresence";

export interface CollabLayerProps {
  /** Client to observe; defaults to the shared `getCollabClient()` instance. */
  client?: CollabClient;
  /** Also render a small connection-status pill (default `false`). */
  showStatus?: boolean;
  /** Extra class name on the root element, for app-level styling hooks. */
  className?: string;
}

/** Remote multi-cursor overlay. Renders nothing when the room is empty. */
export function CollabLayer({ client, showStatus = false, className }: CollabLayerProps) {
  const peers = useRemotePeers(client);
  const { status, error } = useCollabStatus(client);

  // Only peers with a live cursor are drawable; keep a stable draw order so
  // colours/labels do not flicker as the map re-emits.
  const cursors = useMemo(
    () => peers.filter((p) => p.cursor !== null).sort((a, b) => a.replica - b.replica),
    [peers],
  );

  if (cursors.length === 0 && !showStatus) return null;

  return (
    <div
      className={className}
      aria-hidden="true"
      style={{
        position: "absolute",
        inset: 0,
        pointerEvents: "none",
        overflow: "hidden",
      }}
      data-collab-peers={cursors.length}
    >
      {cursors.map((peer) => (
        <RemoteCursor key={peer.replica} peer={peer} />
      ))}
      {showStatus ? <StatusPill status={status} error={error} /> : null}
    </div>
  );
}

/** Clamp a normalized coordinate so off-screen cursors stay visible at the edge. */
function clamp01(value: number): number {
  if (!Number.isFinite(value)) return 0;
  return Math.min(1, Math.max(0, value));
}

/** One remote collaborator: coloured dot plus name tag. */
function RemoteCursor({ peer }: { peer: RemotePeer }) {
  const cursor = peer.cursor ?? [0, 0, 0];
  const left = `${clamp01(cursor[0]) * 100}%`;
  const top = `${clamp01(cursor[1]) * 100}%`;

  return (
    <div
      style={{
        position: "absolute",
        left,
        top,
        transform: "translate(-50%, -50%)",
        display: "flex",
        alignItems: "center",
        gap: 6,
        pointerEvents: "none",
        transition: "left 80ms linear, top 80ms linear",
        willChange: "left, top",
      }}
    >
      <span
        style={{
          width: 10,
          height: 10,
          borderRadius: "50%",
          background: peer.color,
          boxShadow: "0 0 0 2px rgba(0, 0, 0, 0.45)",
          flex: "0 0 auto",
        }}
      />
      <span
        style={{
          font: "500 11px/1.4 system-ui, sans-serif",
          color: "#fff",
          background: peer.color,
          borderRadius: 4,
          padding: "1px 6px",
          whiteSpace: "nowrap",
          textShadow: "0 1px 1px rgba(0, 0, 0, 0.4)",
        }}
      >
        {peer.displayName || `replica ${peer.replica}`}
        {peer.selection.length > 0 ? ` · ${peer.selection.length} selected` : ""}
      </span>
    </div>
  );
}

/** Optional corner pill showing whether the sync server is reachable. */
function StatusPill({ status, error }: { status: CollabStatus; error: string | null }) {
  const label: Record<CollabStatus, string> = {
    disconnected: "collab: offline",
    connecting: "collab: connecting…",
    connected: "collab: live",
    reconnecting: "collab: reconnecting…",
    unavailable: "collab: unavailable",
  };
  const dot = status === "connected" ? "#3fb950" : status === "unavailable" ? "#8b949e" : "#d29922";

  return (
    <div
      title={error ?? undefined}
      style={{
        position: "absolute",
        right: 8,
        bottom: 8,
        display: "flex",
        alignItems: "center",
        gap: 6,
        font: "500 11px/1.4 system-ui, sans-serif",
        color: "#c9d1d9",
        background: "rgba(13, 17, 23, 0.72)",
        border: "1px solid rgba(139, 148, 158, 0.35)",
        borderRadius: 999,
        padding: "2px 8px",
        pointerEvents: "none",
      }}
    >
      <span
        style={{ width: 6, height: 6, borderRadius: "50%", background: dot, flex: "0 0 auto" }}
      />
      {label[status]}
    </div>
  );
}
