# tpt-vertex-printer-link

Network printer connectivity for [TPT Vertex](https://tpt-vertex.dev) — a small,
dependency-light Rust client that talks to common FDM printer front-ends over the
LAN so the desktop app and web UI can upload and control prints.

## Supported protocols

| Protocol | Notes |
| --- | --- |
| **ESP3D** | G-code-over-HTTP (`/?cmd=...`) plus multipart file upload. Found on many ESP32 printer control boards. |
| **OctoPrint** | Native OctoPrint REST API (`/api/version`, `/api/printer`, `/api/job`, `/api/files/local`). |
| **Moonraker (`octoprint_compat`)** | Moonraker's OctoPrint-compatibility shim exposes the same REST surface; this crate drives it through the OctoPrint client (label only differs). |

All three are exposed through a single [`PrinterClient`] trait, so the rest of
Vertex drives any supported printer identically.

## Example

```rust
use tpt_vertex_printer_link::{make_client, PrinterTarget, ProtocolKind};

let target = PrinterTarget::new(
    "bench-esp",
    "Bench ESP32",
    ProtocolKind::Esp3d,
    "http://192.168.1.50",
    None,
);
let client = make_client(&target)?;

let info = client.test_connection()?;
println!("connected: {} ({})", info.connected, info.firmware.unwrap_or_default());

client.upload_gcode("part.gcode", gcode_bytes)?;
client.start_print("part.gcode")?;
let status = client.status()?;
println!("state: {}, tool: {}°C", status.state.label(), status.temps.tool);
```

## Design notes

- **Connection vs. machine config.** A [`PrinterTarget`] describes *how to reach*
  a printer (host, protocol, credentials). It is deliberately distinct from the
  physical [`PrinterProfile`](https://docs.rs/tpt-vertex-slicer) in
  `tpt-vertex-slicer`, which describes the machine's geometry and kinematics.
- **Pluggable transport.** The protocol clients talk through an `HttpTransport`
  trait rather than `reqwest` directly, so they are unit-tested against an
  in-memory mock and integration-tested against a tiny local mock HTTP server
  (no external mocks required).
- **TLS.** The crate builds without a TLS backend (`default-features = false`)
  for fast, auditable builds against the plain-HTTP endpoints printers typically
  expose on a LAN. Enable the `tls` feature for HTTPS printer fronts.

## License

Dual **MIT OR Apache-2.0**.
