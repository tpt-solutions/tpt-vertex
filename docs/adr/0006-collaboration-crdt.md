# ADR-0006: Real-time collaboration — custom CRDT over the feature tree

- Status: Accepted
- Date: 2026-07-19

## Context

Vertex's defining feature is real-time, multi-user editing of the same 3D
assembly, conflict-free, using CRDTs over WebSockets (the "Figma for Hardware"
promise). We must choose how to represent and synchronize concurrent edits.

Options:

- **Yjs** (or a Rust port such as `yrs`): a mature, general-purpose CRDT library
  for shared maps/arrays/text with an efficient update protocol and awareness
  (presence) support.
- **Custom CRDT**: purpose-built for the parametric feature tree — a keyed map of
  feature nodes with last-writer-wins (LWW) per parameter and an add/remove set
  for feature membership.

Forces:

- The shared state is *structured*: an ordered set of feature nodes, each with
  typed parameters, plus assemblies/mates. It is not free-form rich text. The
  natural CRDT is a map of `FeatureId -> feature parameters` with per-field LWW,
  plus an OR-Set for feature existence and an ordering CRDT for history position.
- The same data model already backs versioning (ADR-0005:
  `FeatureManifest`/feature tree). Collaboration and version control must mutate
  the *same* structure so a commit is a snapshot of the live CRDT state.
- Vertex is Rust-first (kernel, renderer, versioning). A Rust-native CRDT keeps
  the model in one language, compilable to WASM for the browser and native for
  desktop, sharing types with the kernel.
- We want conflict semantics we fully control (e.g. a concurrent extrude-height
  edit and a fillet-radius edit on different features must both survive; two
  edits to the *same* parameter resolve by LWW with a Lamport/wall-clock tie).

## Decision

TPT Vertex implements a **custom CRDT tailored to the feature tree**, in Rust,
in the `tpt-vertex-collab` crate, synchronized over WebSockets.

- State is a **replicated document** keyed by `FeatureId`:
  - Feature existence is an **OR-Set** (observed-remove) so concurrent add/remove
    converge.
  - Each feature's parameters use **LWW-Register** semantics stamped with a
    hybrid logical clock (Lamport counter + replica id tiebreak).
  - Ordering (history position) is a fractional-index / sequence CRDT.
- Each replica has a unique `ReplicaId`; operations carry a monotonic clock so
  merges are commutative, associative, and idempotent (CRDT guarantees).
- **Presence** (cursors, active selection) is ephemeral awareness state, not part
  of the persisted CRDT, broadcast over the same socket.
- The document converges to a `FeatureTree`, so `versioning::manifest_from_tree`
  can snapshot the collaborative state into a commit at any time.

We deliberately do **not** adopt Yjs as the source of truth to avoid a
JS-centric model and a second data representation; a Yjs interop bridge remains
possible later if needed.

## Consequences

- Positive: one Rust data model spans kernel, collaboration, and versioning;
  conflict semantics are geometry-aware and under our control; compiles to WASM
  and native; presence is cheap and separate from persisted state.
- Positive: snapshots for version control are trivial (the CRDT already *is* the
  feature tree).
- Negative: we own the CRDT correctness burden (convergence, GC of tombstones,
  clock management) instead of inheriting Yjs's battle-testing.
- Follow-up: the sync server (`collab::server`), the client binding, offline
  buffering with reconnect/resync, authentication/rooms, and access control; and
  property-based convergence tests for the merge operators.
