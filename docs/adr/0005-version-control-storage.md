# ADR-0005: Version-control storage — custom manifest + blob engine over Git LFS

- Status: Accepted
- Date: 2026-07-19

## Context

TPT Vertex bakes Git-like version control into the core (branch, merge, review
of 3D geometry). We must decide how design revisions are stored and diffed. The
spec suggests two paths:

- **Git LFS**: keep designs as files in a real Git repository, using Git Large
  File Storage for the binary geometry blobs. We get Git's mature DAG, tooling,
  and hosting for free.
- **Custom binary-diffing engine**: implement content-addressed storage and a
  domain-aware diff/merge directly over our data model.

The forces in play:

- The unit of meaningful change in Vertex is a **feature-tree edit**, not a byte
  range of an opaque binary. A one-parameter change (extrude height) should be a
  tiny, reviewable diff, and a merge of two concurrent parameter edits on
  different features should be automatic. Byte-level LFS diffs cannot express
  this — LFS stores whole-object versions and diffs are opaque.
- We already represent a revision as a `FeatureManifest` (ordered features + a
  per-feature `param_hash`) plus content-addressed `Blob`s for evaluated
  geometry. `Diff::between` already produces `Added/Removed/Modified(id)` at
  feature granularity, and `Repository::merge` detects conflicts per feature.
- Real-time collaboration (Phase 4, CRDT) and version control must share the
  same feature-tree data model so that "commit" is a snapshot of the same state
  CRDTs mutate. Coupling to Git's on-disk format would fight that.
- We still want interoperability with Git hosting for distribution and backup.

## Decision

TPT Vertex uses a **custom, content-addressed manifest + blob engine** as the
canonical version-control model, and treats Git/Git LFS as an *optional export
and transport target*, not the source of truth.

- A revision is a `FeatureManifest` (see `versioning::manifest_from_tree`) plus
  content-addressed `Blob`s for evaluated solids. Both are SHA-256 hashed, so
  identical content deduplicates automatically (the same property LFS provides,
  but at feature/mesh granularity).
- Diff and merge operate on the manifest at **feature granularity**
  (`Change::Modified(feature_id)`), giving small, reviewable, semantically
  meaningful diffs and automatic clean merges of edits to disjoint features.
- The commit DAG (`Repository`) mirrors Git's parents/branches/merge-base model
  so the mental model and future Git bridging are straightforward.
- Interop: a repository can be serialized into a Git repository where each
  object is stored by its content hash and large evaluated blobs are handed to
  Git LFS. This is an export/transport concern layered *on top of* the native
  engine, not a dependency of it.

## Consequences

- Positive: diffs and merges are geometry-aware and reviewable; the versioning
  model shares its data structures with the CRDT collaboration layer; storage is
  deduplicated by content hash; no hard dependency on a Git binary or LFS server.
- Positive: we control the merge semantics for parametric edits, which is the
  product's differentiator.
- Negative: we must maintain our own storage/GC and (eventually) a Git bridge,
  rather than inheriting Git's ecosystem wholesale.
- Follow-up: a persistent object store (currently in-memory `HashMap`), a
  packfile/GC strategy for blobs, and an optional Git/Git-LFS export bridge.
