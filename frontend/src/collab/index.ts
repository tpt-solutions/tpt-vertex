/**
 * Real-time collaboration frontend entry point (Phase 13).
 *
 * ```tsx
 * import { CollabLayer, getCollabClient } from "./collab";
 *
 * const client = getCollabClient({ displayName: "Ada", room: "demo" });
 * client.connect();
 * ```
 *
 * The client speaks to `collab/src/bin/sync_server.rs`
 * (`cargo run -p tpt-vertex-collab --bin sync_server`).
 */
export {
  CollabClient,
  DEFAULT_ROOM,
  DEFAULT_SYNC_URL,
  DEFAULT_TOKEN,
  getCollabClient,
  peerColor,
  resetCollabClient,
  toRemotePeer,
  type AccessLevel,
  type ClientMessage,
  type CollabClientOptions,
  type CollabStatus,
  type CrdtSnapshot,
  type FeatureKey,
  type HybridClock,
  type Op,
  type ParamValue,
  type RemotePeer,
  type ReplicaId,
  type ServerMessage,
  type Unsubscribe,
  type WirePresence,
} from "./client";

export { CollabLayer, type CollabLayerProps } from "./PresenceOverlay";

export { useCollabStatus, useRemotePeers } from "./usePresence";
