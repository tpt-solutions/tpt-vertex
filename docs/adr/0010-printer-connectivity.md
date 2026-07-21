# ADR-0010: Printer connectivity — unified ESP3D/OctoPrint client

- Status: Accepted
- Date: 2026-07-21

## Context

Phase 10 added an in-app slicer that emits G-code. To actually print, Vertex must
push that G-code to a real printer and let the user monitor/control the job. The
common self-hosted printer front-ends are:

- **ESP3D** — firmware on many ESP32 printer control boards. Control is
  G-code tunnelled over HTTP (`GET /?cmd=<gcode>`) plus a multipart file upload.
- **OctoPrint** — the de-facto Raspberry-Pi print server, with a JSON REST API
  (`/api/printer`, `/api/job`, `/api/files/local`, ...).
- **Moonraker** — Klipper's API host, which also ships an `octoprint_compat`
  component exposing the OctoPrint REST surface.

Forces:

- We want one code path in the UI, not per-protocol branches everywhere.
- Printer endpoints live on the LAN and are almost always plain HTTP; we should
  not force a heavy TLS stack into the build.
- The desktop client embeds the kernel; printer control must be callable from
  the same Tauri core process (see ADR-0007) and from the web frontend via Tauri
  IPC.
- A `PrinterTarget` ("how to reach a printer") is a different concept from the
  physical `PrinterProfile` ("what the machine is") already in
  `tpt-vertex-slicer`. Mixing them would couple connection config to slicing
  config.

## Decision

Add a new standalone crate, **`tpt-vertex-printer-link`** (workspace member,
publish-ready), that:

- Defines `PrinterTarget` / `ProtocolKind` for connection config, distinct from
  `tpt-vertex-slicer::PrinterProfile`.
- Exposes a uniform `PrinterClient` trait (`test_connection`, `status`,
  `upload_gcode`, `start_print`, `pause`, `resume`, `cancel`, `send_gcode`)
  returning shared types (`ConnectionInfo`, `StatusSnapshot`, `PrinterState`,
  `Temperature`, `JobProgress`) and a `PrinterError` enum.
- Ships `Esp3dClient` (G-code-over-HTTP) and `OctoPrintClient` (REST, also used
  for Moonraker `octoprint_compat`; only the firmware label differs).
- Routes construction through `make_client(target) -> Box<dyn PrinterClient>`.
- Talks HTTP through a pluggable `HttpTransport` trait; the real implementation
  is `ReqwestTransport` (reqwest blocking, no TLS by default, `tls` feature
  available). Tests use an in-memory `MockTransport` and a tiny local mock HTTP
  server (no `mockito`/network dependency).

The desktop app persists `PrinterTarget`s with `tauri-plugin-store`
(`printers.json`), exposes Tauri commands (`list_printers`, `save_printer`,
`delete_printer`, `test_printer`, `send_to_printer`, `printer_status`), and the
frontend wraps those via `@tauri-apps/api` in `frontend/src/printer/client.ts`,
with a `PrinterPanel` for management and a "Send to Printer" action in
`SlicerPanel`.

## Consequences

- Positive: one trait, three protocols, testable without real hardware or
  external mocks; clean separation of connection config from machine config.
- Positive: no TLS requirement for the common LAN-HTTP case; HTTPS available via
  a single feature flag.
- Positive: same client reused by desktop (Tauri) and (potentially) direct Rust
  tooling.
- Negative: ESP3D and Moonraker state reporting is approximate (Marlin-style SD
  status / compat shim), so progress/state fidelity depends on the firmware.
- Follow-up (fast-follows): mDNS/zeroconf auto-discovery, streaming G-code
  layer-by-layer as it slices, a native Moonraker client if `octoprint_compat`
  coverage proves insufficient, OS keychain storage for API keys, and feeding
  printer telemetry back into `tpt-vertex-simulation` for closed-loop deviation
  detection. Manual end-to-end verification remains against OctoPrint's Virtual
  Printer, a real Moonraker instance, and an ESP32 running ESP3D.
