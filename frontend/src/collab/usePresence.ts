/**
 * React hooks for the collaboration client (Phase 13).
 *
 * Kept out of `PresenceOverlay.tsx` so that file only exports components (Vite
 * fast refresh requirement). Any panel can reuse these to show collaborators
 * without rendering the multi-cursor overlay.
 */
import { useEffect, useState } from "react";

import { CollabClient, getCollabClient, type CollabStatus, type RemotePeer } from "./client";

/**
 * Subscribe to a client's remote peers. Defaults to the shared client from
 * `getCollabClient()`; the callback fires once immediately on mount.
 */
export function useRemotePeers(client?: CollabClient): RemotePeer[] {
  const [peers, setPeers] = useState<RemotePeer[]>([]);
  useEffect(() => {
    const target = client ?? getCollabClient();
    return target.onRemotePresence(setPeers);
  }, [client]);
  return peers;
}

/** Connection status of a client (also fires once immediately on mount). */
export function useCollabStatus(client?: CollabClient): {
  status: CollabStatus;
  error: string | null;
} {
  const [state, setState] = useState<{ status: CollabStatus; error: string | null }>({
    status: "disconnected",
    error: null,
  });
  useEffect(() => {
    const target = client ?? getCollabClient();
    return target.onStatus((status, error) => setState({ status, error }));
  }, [client]);
  return state;
}
