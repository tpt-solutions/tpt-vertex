/**
 * WebSocket client for the collaboration sync server (Phase 13).
 *
 * Talks to `collab/src/bin/sync_server.rs`, which is a thin WebSocket adapter
 * over the transport-agnostic `SyncHub` in the `tpt-vertex-collab` crate. The
 * wire format is serde's default JSON encoding of the Rust
 * `ClientMessage`/`ServerMessage` enums:
 *
 * - struct variants are externally tagged: `{"Ops":{"ops":[...]}}`
 * - unit variants are bare strings: `"Resync"`, `"Leave"`
 * - payload fields keep Rust's snake_case (`display_name`, `wall_ms`), exactly
 *   like the Tauri IPC wrappers in `printer/client.ts`
 *
 * Everything that faces the React app is camelCase (`RemotePeer.displayName`).
 *
 * Like the rest of the frontend, this degrades gracefully: with no WebSocket
 * implementation, no server listening, or a mid-session drop, the client
 * reports a status instead of throwing, and local edits are queued rather than
 * lost.
 */

/** Default sync server address (`sync_server --host 127.0.0.1 --port 8787`). */
export const DEFAULT_SYNC_URL = "ws://localhost:8787";

/** Default room joined when the caller does not pick one. */
export const DEFAULT_ROOM = "default";

/** Join token accepted by a `sync_server` started without `--token`. */
export const DEFAULT_TOKEN = "dev";

/* ------------------------------------------------------------------ *
 * Wire types — mirror `collab/src/{clock,crdt,presence,protocol}.rs`.
 * ------------------------------------------------------------------ */

/** `collab::clock::ReplicaId` (a `u64` newtype, serialized as a bare number). */
export type ReplicaId = number;

/** `collab::crdt::FeatureKey`. */
export type FeatureKey = number;

/** `collab::clock::HybridClock`. */
export interface HybridClock {
  wall_ms: number;
  lamport: number;
  replica: ReplicaId;
}

/** `collab::crdt::ParamValue`. */
export type ParamValue =
  { Number: number } | { Int: number } | { Text: string } | { Bool: boolean };

/** `collab::crdt::Op`. */
export type Op =
  | { AddFeature: { key: FeatureKey; kind: string; tag: HybridClock; position: string } }
  | { RemoveFeature: { key: FeatureKey; observed_tags: HybridClock[] } }
  | { SetParam: { key: FeatureKey; name: string; value: ParamValue; clock: HybridClock } }
  | { SetPosition: { key: FeatureKey; position: string; clock: HybridClock } };

/** `collab::protocol::AccessLevel`. */
export type AccessLevel = "Viewer" | "Editor" | "Owner";

/** `collab::presence::Presence` as it appears on the wire. */
export interface WirePresence {
  replica: ReplicaId;
  display_name: string;
  /** World-space `[x, y, z]`; the overlay treats x/y as normalized 0..1. */
  cursor: [number, number, number] | null;
  selection: FeatureKey[];
  epoch: number;
}

/**
 * `collab::crdt::CrdtDoc`. Opaque to the frontend for now: the CRDT merge lives
 * in Rust (natively in `sync_server`, in the browser behind the crate's `wasm`
 * feature), so JS only needs to pass snapshots around.
 */
export interface CrdtSnapshot {
  features: Record<string, unknown>;
}

/** `collab::protocol::ClientMessage`. */
export type ClientMessage =
  | { Join: { room: string; token: string; replica: ReplicaId; display_name: string } }
  | { Ops: { ops: Op[] } }
  | { Presence: { presence: WirePresence } }
  | { SetAccess: { subject: string; level: AccessLevel } }
  | "Resync"
  | "Leave";

/** `collab::protocol::ServerMessage`. */
export type ServerMessage =
  | { Welcome: { level: AccessLevel; snapshot: CrdtSnapshot; presence: WirePresence[] } }
  | { Rejected: { reason: string } }
  | { Ops: { from: ReplicaId; ops: Op[] } }
  | { Presence: { presence: WirePresence } }
  | { Left: { replica: ReplicaId } }
  | { Snapshot: { snapshot: CrdtSnapshot } };

/* ------------------------------------------------------------------ *
 * App-facing types.
 * ------------------------------------------------------------------ */

/** One remote collaborator, in camelCase, with a stable display colour. */
export interface RemotePeer {
  replica: ReplicaId;
  displayName: string;
  cursor: [number, number, number] | null;
  selection: FeatureKey[];
  epoch: number;
  /** Deterministic colour derived from the replica id. */
  color: string;
}

/**
 * Connection status.
 *
 * - `unavailable`: no WebSocket in this environment (SSR, locked-down webview);
 *   the client is inert and never retries.
 */
export type CollabStatus =
  "disconnected" | "connecting" | "connected" | "reconnecting" | "unavailable";

export interface CollabClientOptions {
  /** Sync server URL, default `VITE_COLLAB_URL` or `ws://localhost:8787`. */
  url?: string;
  room?: string;
  token?: string;
  /** Stable replica id for this session; random when omitted. */
  replica?: ReplicaId;
  displayName?: string;
  /** Reconnect automatically after a drop (default `true`). */
  autoReconnect?: boolean;
  /** Base reconnect delay in ms (default `1000`, capped exponential backoff). */
  reconnectDelayMs?: number;
  /** Minimum gap between outgoing cursor updates in ms (default `50`). */
  presenceThrottleMs?: number;
}

/** Unsubscribe handle returned by every `on*` method. */
export type Unsubscribe = () => void;

/** Max messages buffered while the socket is down before the oldest is dropped. */
const MAX_QUEUE = 256;
/** Upper bound on reconnect backoff. */
const MAX_RECONNECT_DELAY_MS = 15_000;

/** Deterministic, well-separated colour per replica id. */
export function peerColor(replica: ReplicaId): string {
  // Golden-angle hue rotation keeps neighbouring ids visually distinct.
  const hue = Math.abs(Math.round(replica * 137.508)) % 360;
  return `hsl(${hue}, 72%, 58%)`;
}

/** Convert a wire presence record into the camelCase app-facing shape. */
export function toRemotePeer(presence: WirePresence): RemotePeer {
  return {
    replica: presence.replica,
    displayName: presence.display_name,
    cursor: presence.cursor ?? null,
    selection: presence.selection ?? [],
    epoch: presence.epoch,
    color: peerColor(presence.replica),
  };
}

function defaultUrl(): string {
  try {
    const fromEnv = import.meta.env?.VITE_COLLAB_URL;
    if (typeof fromEnv === "string" && fromEnv.length > 0) return fromEnv;
  } catch {
    // Not running under a Vite bundle (e.g. a plain node import).
  }
  return DEFAULT_SYNC_URL;
}

function randomReplicaId(): ReplicaId {
  // Positive and safely inside the u64 range the Rust side expects.
  return Math.floor(Math.random() * 0x7fffffff) + 1;
}

/**
 * A collaboration session over one WebSocket.
 *
 * ```ts
 * const client = new CollabClient({ displayName: "Ada" });
 * client.onRemotePresence((peers) => console.log(peers));
 * client.connect();
 * client.setLocalCursor(0.5, 0.5);
 * ```
 */
export class CollabClient {
  readonly url: string;
  readonly room: string;
  readonly replica: ReplicaId;

  private token: string;
  private displayName: string;
  private readonly autoReconnect: boolean;
  private readonly reconnectDelayMs: number;
  private readonly presenceThrottleMs: number;

  private socket: WebSocket | null = null;
  private status: CollabStatus = "disconnected";
  private lastError: string | null = null;
  private level: AccessLevel | null = null;

  /** Remote peers by replica id (never contains the local replica). */
  private peers = new Map<ReplicaId, RemotePeer>();

  private queue: ClientMessage[] = [];
  private epoch = 0;
  private selection: FeatureKey[] = [];
  private cursor: [number, number, number] | null = null;

  private presenceTimer: ReturnType<typeof setTimeout> | null = null;
  private presenceSentAt = 0;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private reconnectAttempts = 0;
  /** Set by `disconnect()` so a deliberate close does not trigger a retry. */
  private closedByUser = false;

  private presenceListeners = new Set<(peers: RemotePeer[]) => void>();
  private opsListeners = new Set<(ops: Op[], from: ReplicaId) => void>();
  private statusListeners = new Set<(status: CollabStatus, error: string | null) => void>();
  private snapshotListeners = new Set<(snapshot: CrdtSnapshot) => void>();

  constructor(options: CollabClientOptions = {}) {
    this.url = options.url ?? defaultUrl();
    this.room = options.room ?? DEFAULT_ROOM;
    this.token = options.token ?? DEFAULT_TOKEN;
    this.replica = options.replica ?? randomReplicaId();
    this.displayName = options.displayName ?? `user-${this.replica % 1000}`;
    this.autoReconnect = options.autoReconnect ?? true;
    this.reconnectDelayMs = options.reconnectDelayMs ?? 1000;
    this.presenceThrottleMs = options.presenceThrottleMs ?? 50;
  }

  /* ---------------- lifecycle ---------------- */

  /** True when a WebSocket implementation exists in this environment. */
  static isSupported(): boolean {
    return typeof WebSocket !== "undefined";
  }

  /** Current connection status. */
  getStatus(): CollabStatus {
    return this.status;
  }

  /** Last connection/protocol error, if any. */
  getLastError(): string | null {
    return this.lastError;
  }

  /** Access level granted by the server's `Welcome`, if joined. */
  getAccessLevel(): AccessLevel | null {
    return this.level;
  }

  /** Snapshot of the currently known remote peers. */
  getPeers(): RemotePeer[] {
    return [...this.peers.values()];
  }

  /**
   * Open the socket and join the room. Safe to call repeatedly; never throws —
   * failures surface through `onStatus`.
   */
  connect(): void {
    if (!CollabClient.isSupported()) {
      this.setStatus("unavailable", "WebSocket is not available in this environment.");
      return;
    }
    if (this.socket && (this.status === "connected" || this.status === "connecting")) return;

    this.closedByUser = false;
    this.clearReconnect();
    this.setStatus(this.reconnectAttempts > 0 ? "reconnecting" : "connecting", this.lastError);

    let socket: WebSocket;
    try {
      socket = new WebSocket(this.url);
    } catch (err) {
      // A malformed URL or a blocked scheme: retrying cannot help.
      this.socket = null;
      this.setStatus("unavailable", errorMessage(err));
      return;
    }
    this.socket = socket;

    socket.onopen = () => {
      this.reconnectAttempts = 0;
      this.setStatus("connected", null);
      this.send({
        Join: {
          room: this.room,
          token: this.token,
          replica: this.replica,
          display_name: this.displayName,
        },
      });
      // A reconnect may have missed ops while offline.
      this.send("Resync");
      this.flushQueue();
      if (this.cursor) this.sendPresenceNow();
    };

    socket.onmessage = (event: MessageEvent) => {
      if (typeof event.data !== "string") return;
      this.handleServerMessage(event.data);
    };

    socket.onerror = () => {
      // The browser gives no detail here; `onclose` follows and drives retries.
      this.lastError = `Unable to reach the collaboration server at ${this.url}.`;
    };

    socket.onclose = () => {
      this.socket = null;
      this.level = null;
      if (this.peers.size > 0) {
        this.peers.clear();
        this.emitPresence();
      }
      if (this.closedByUser || !this.autoReconnect) {
        this.setStatus("disconnected", this.lastError);
        return;
      }
      this.setStatus("reconnecting", this.lastError);
      this.scheduleReconnect();
    };
  }

  /** Leave the room and close the socket without reconnecting. */
  disconnect(): void {
    this.closedByUser = true;
    this.clearReconnect();
    if (this.presenceTimer !== null) {
      clearTimeout(this.presenceTimer);
      this.presenceTimer = null;
    }
    const socket = this.socket;
    this.socket = null;
    this.queue = [];
    this.level = null;
    if (socket) {
      try {
        if (socket.readyState === WebSocket.OPEN) socket.send(JSON.stringify("Leave"));
        socket.close();
      } catch {
        // Already closing/closed: nothing to clean up.
      }
    }
    if (this.peers.size > 0) {
      this.peers.clear();
      this.emitPresence();
    }
    this.setStatus("disconnected", this.lastError);
  }

  /* ---------------- outbound ---------------- */

  /** Broadcast locally produced CRDT ops. Queued while offline. */
  sendOps(ops: Op[]): void {
    if (ops.length === 0) return;
    this.send({ Ops: { ops } });
  }

  /** Ask the server for a full document snapshot (used after an outage). */
  resync(): void {
    this.send("Resync");
  }

  /**
   * Report the local cursor. `x`/`y` are normalized viewport coordinates
   * (`0..1`, origin top-left) so the overlay can place cursors without knowing
   * the camera; `z` is optional world depth.
   */
  setLocalCursor(x: number, y: number, z = 0): void {
    this.cursor = [x, y, z];
    this.sendPresenceThrottled();
  }

  /** Clear the local cursor (pointer left the viewport). */
  clearLocalCursor(): void {
    if (this.cursor === null) return;
    this.cursor = null;
    this.sendPresenceNow();
  }

  /** Update any part of the local presence record. */
  updatePresence(patch: {
    cursor?: [number, number, number] | null;
    selection?: FeatureKey[];
    displayName?: string;
  }): void {
    if (patch.cursor !== undefined) this.cursor = patch.cursor;
    if (patch.selection !== undefined) this.selection = [...patch.selection];
    if (patch.displayName !== undefined) this.displayName = patch.displayName;
    this.sendPresenceNow();
  }

  /** Owner-only: change another member's access level. */
  setAccess(subject: string, level: AccessLevel): void {
    this.send({ SetAccess: { subject, level } });
  }

  /* ---------------- subscriptions ---------------- */

  /**
   * Subscribe to remote presence. The callback fires immediately with the
   * current peers and on every change. Returns an unsubscribe function.
   */
  onRemotePresence(cb: (peers: RemotePeer[]) => void): Unsubscribe {
    this.presenceListeners.add(cb);
    safeInvoke(() => cb(this.getPeers()));
    return () => {
      this.presenceListeners.delete(cb);
    };
  }

  /** Subscribe to remote CRDT ops. */
  onOps(cb: (ops: Op[], from: ReplicaId) => void): Unsubscribe {
    this.opsListeners.add(cb);
    return () => {
      this.opsListeners.delete(cb);
    };
  }

  /** Subscribe to connection status changes. */
  onStatus(cb: (status: CollabStatus, error: string | null) => void): Unsubscribe {
    this.statusListeners.add(cb);
    safeInvoke(() => cb(this.status, this.lastError));
    return () => {
      this.statusListeners.delete(cb);
    };
  }

  /** Subscribe to full-document snapshots (`Welcome` and `Snapshot`). */
  onSnapshot(cb: (snapshot: CrdtSnapshot) => void): Unsubscribe {
    this.snapshotListeners.add(cb);
    return () => {
      this.snapshotListeners.delete(cb);
    };
  }

  /* ---------------- internals ---------------- */

  private send(message: ClientMessage): void {
    const socket = this.socket;
    if (!socket || socket.readyState !== WebSocket.OPEN) {
      this.enqueue(message);
      return;
    }
    try {
      socket.send(JSON.stringify(message));
    } catch (err) {
      this.lastError = errorMessage(err);
      this.enqueue(message);
    }
  }

  private enqueue(message: ClientMessage): void {
    // Presence is ephemeral: only ops and control messages are worth replaying.
    if (typeof message === "object" && "Presence" in message) return;
    if (this.queue.length >= MAX_QUEUE) this.queue.shift();
    this.queue.push(message);
  }

  private flushQueue(): void {
    if (this.queue.length === 0) return;
    const pending = this.queue;
    this.queue = [];
    for (const message of pending) this.send(message);
  }

  private localPresence(): WirePresence {
    this.epoch += 1;
    return {
      replica: this.replica,
      display_name: this.displayName,
      cursor: this.cursor,
      selection: this.selection,
      epoch: this.epoch,
    };
  }

  private sendPresenceNow(): void {
    if (this.presenceTimer !== null) {
      clearTimeout(this.presenceTimer);
      this.presenceTimer = null;
    }
    this.presenceSentAt = Date.now();
    this.send({ Presence: { presence: this.localPresence() } });
  }

  private sendPresenceThrottled(): void {
    const elapsed = Date.now() - this.presenceSentAt;
    if (elapsed >= this.presenceThrottleMs) {
      this.sendPresenceNow();
      return;
    }
    if (this.presenceTimer !== null) return;
    this.presenceTimer = setTimeout(() => {
      this.presenceTimer = null;
      this.sendPresenceNow();
    }, this.presenceThrottleMs - elapsed);
  }

  private handleServerMessage(raw: string): void {
    let message: ServerMessage;
    try {
      message = JSON.parse(raw) as ServerMessage;
    } catch (err) {
      this.lastError = `Malformed frame from sync server: ${errorMessage(err)}`;
      return;
    }
    if (typeof message !== "object" || message === null) return;

    if ("Welcome" in message) {
      this.level = message.Welcome.level;
      this.peers.clear();
      for (const presence of message.Welcome.presence) this.mergePresence(presence);
      this.emitPresence();
      this.emitSnapshot(message.Welcome.snapshot);
      return;
    }
    if ("Presence" in message) {
      if (this.mergePresence(message.Presence.presence)) this.emitPresence();
      return;
    }
    if ("Left" in message) {
      if (this.peers.delete(message.Left.replica)) this.emitPresence();
      return;
    }
    if ("Ops" in message) {
      const { ops, from } = message.Ops;
      for (const cb of [...this.opsListeners]) safeInvoke(() => cb(ops, from));
      return;
    }
    if ("Snapshot" in message) {
      this.emitSnapshot(message.Snapshot.snapshot);
      return;
    }
    if ("Rejected" in message) {
      this.lastError = message.Rejected.reason;
      this.emitStatus();
    }
  }

  /** Apply one presence record; returns whether the peer map changed. */
  private mergePresence(presence: WirePresence): boolean {
    if (presence.replica === this.replica) return false;
    const existing = this.peers.get(presence.replica);
    if (existing && existing.epoch >= presence.epoch) return false;
    this.peers.set(presence.replica, toRemotePeer(presence));
    return true;
  }

  private emitPresence(): void {
    const peers = this.getPeers();
    for (const cb of [...this.presenceListeners]) safeInvoke(() => cb(peers));
  }

  private emitSnapshot(snapshot: CrdtSnapshot): void {
    for (const cb of [...this.snapshotListeners]) safeInvoke(() => cb(snapshot));
  }

  private emitStatus(): void {
    for (const cb of [...this.statusListeners]) safeInvoke(() => cb(this.status, this.lastError));
  }

  private setStatus(status: CollabStatus, error: string | null): void {
    const changed = this.status !== status || this.lastError !== error;
    this.status = status;
    this.lastError = error;
    if (changed) this.emitStatus();
  }

  private scheduleReconnect(): void {
    if (this.reconnectTimer !== null) return;
    this.reconnectAttempts += 1;
    const delay = Math.min(
      this.reconnectDelayMs * 2 ** (this.reconnectAttempts - 1),
      MAX_RECONNECT_DELAY_MS,
    );
    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null;
      if (!this.closedByUser) this.connect();
    }, delay);
  }

  private clearReconnect(): void {
    if (this.reconnectTimer !== null) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
  }
}

function errorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

/** Never let one bad subscriber break the others (or the socket callbacks). */
function safeInvoke(fn: () => void): void {
  try {
    fn();
  } catch (err) {
    console.warn("[collab] listener threw:", err);
  }
}

/* ------------------------------------------------------------------ *
 * Shared instance — lets the overlay subscribe without prop drilling.
 * ------------------------------------------------------------------ */

let sharedClient: CollabClient | null = null;

/**
 * Get (creating on first call) the process-wide client. `options` are only
 * honoured on the first call; use `resetCollabClient()` to reconfigure.
 */
export function getCollabClient(options: CollabClientOptions = {}): CollabClient {
  if (!sharedClient) sharedClient = new CollabClient(options);
  return sharedClient;
}

/** Tear down the shared client (tests, or switching servers/rooms). */
export function resetCollabClient(): void {
  sharedClient?.disconnect();
  sharedClient = null;
}
