# Security Review — TPT Vertex

Status: initial review (v0.1). This document records the threat model and the
security posture of TPT Vertex's authentication, collaboration sync, and file
handling, with findings and follow-ups. It is a living document; re-review before
each release.

## Scope & assets

- **Design data**: parametric feature trees and evaluated geometry (customer IP).
- **Accounts & sessions**: user credentials, session tokens.
- **Collaboration rooms**: real-time document state and presence.
- **Files**: imported/exported CAD files (STEP/STL/OBJ/glTF) and local project
  files (desktop).

## Trust boundaries

1. Browser/desktop client ↔ sync server (WebSocket).
2. Client ↔ platform API (auth, projects, sharing).
3. Server ↔ storage backend (metadata + blobs).
4. Local file system ↔ desktop app (Tauri).

## Authentication & sessions

- Passwords are never stored in plaintext; hashing is isolated behind
  `platform::auth::hash_password`/`verify_password`.
  - **Finding (high)**: the v0.1 hash is a fast FNV digest placeholder. **Action**:
    replace with Argon2id (memory-hard) before any production deployment. The
    interface is already abstracted, so this is a localized change.
- A password policy (`is_acceptable_password`) enforces a minimum length.
  - **Follow-up**: add breach-list and complexity checks; add rate limiting and
    lockout on repeated failures.
- Sessions are opaque ids resolved server-side (`Platform::authenticate`).
  - **Follow-up**: session expiry/rotation, secure+HttpOnly+SameSite cookies (web),
    OS keychain storage (desktop), and CSRF protection for state-changing routes.

## Collaboration sync

- The sync hub authenticates every `Join` via a pluggable `Authenticator`;
  unauthenticated joins are rejected.
- **Access control** is enforced server-side: viewers cannot submit ops
  (`SyncHub::on_ops` rejects `Viewer`), and only owners may change access
  (`on_set_access`). Client UI restrictions are defense-in-depth only.
- Ops are CRDT operations applied to an authoritative document; malformed or
  duplicate ops are idempotent and cannot corrupt state.
  - **Follow-up**: per-connection op rate limiting and payload size caps to
    mitigate DoS; validate op clocks are plausible; bound room membership.
- Presence is ephemeral and dropped on disconnect; it carries no secrets.
- **Transport**: production must run WebSockets over TLS (`wss://`) and validate
  `Origin`. Tokens must be short-lived and scoped to a room.

## File handling

- Exporters write to a caller-provided `Write`; importers read from a `Read`.
- The STEP importer is **tolerant and bounded**: it parses text records and
  reconstructs faceted geometry; it does not `eval` or execute file content.
  - **Follow-up**: enforce a maximum input size and entity count to prevent
    memory-exhaustion from malicious files; fuzz the parser (`cargo fuzz`).
- glTF/OBJ/STL writers emit only numeric geometry; no path or command injection
  surface.
- **Desktop**: file dialogs and FS access go through Tauri's `dialog`/`fs`
  plugins with a scoped capability allowlist. Do not grant blanket FS access;
  restrict to user-selected paths.

## Storage

- Metadata and blobs go through the `Store`/`BlobStore` traits. Blobs are
  content-addressed.
  - **Follow-up**: encrypt at rest, signed URLs for blob access, tenant isolation
    checks on every read/write keyed by resolved permission.

## Dependencies & supply chain

- Rust: run `cargo audit` and `cargo deny` in CI; pin and review updates.
- JS: `npm audit` in CI; lockfile committed.
- **Follow-up**: enable Dependabot and SBOM generation.

## Summary of required actions before production

1. Replace the placeholder password hash with Argon2id. (high)
2. TLS + origin checks + short-lived scoped tokens for sync. (high)
3. Input size/entity caps + fuzzing for importers. (medium)
4. Rate limiting on auth and sync. (medium)
5. Encryption at rest + tenant isolation tests for storage. (medium)
6. `cargo audit`/`npm audit`/`cargo deny` in CI. (medium)
