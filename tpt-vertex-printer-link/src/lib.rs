//! TPT Vertex printer connectivity.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! A small, dependency-light client library that talks to common FDM printer
//! front-ends over the LAN:
//!
//! - **ESP3D** — the firmware shipped on many ESP32-based printer control
//!   boards (e.g. the common "ESP3D Web UI"). Control is via G-code-over-HTTP
//!   (`/?cmd=...`) plus a multipart file upload.
//! - **OctoPrint** and **Moonraker** (`octoprint_compat`) — the OctoPrint REST
//!   API, which Moonraker also exposes behind its OctoPrint-compatibility
//!   shim. Control is via the `/api/printer`, `/api/job`, and `/api/files`
//!   endpoints.
//!
//! Both are exposed through a single [`PrinterClient`] trait so the rest of
//! Vertex (desktop Tauri commands, the web/frontend panel) can drive any
//! supported printer the same way. A [`PrinterTarget`] describes *how to reach*
//! a printer (host, protocol, credentials) and is deliberately distinct from
//! the physical [`PrinterProfile`](tpt_vertex_slicer::profile::PrinterProfile)
//! in `tpt-vertex-slicer`, which describes the machine's geometry and
//! kinematics.
//!
//! The HTTP layer is behind a pluggable [`HttpTransport`] trait so the clients
//! can be unit-tested against an in-memory mock and integration-tested against
//! a tiny local mock HTTP server — no external mocks required.

pub mod client;
pub mod esp3d;
pub mod octoprint;
pub mod target;
pub mod transport;

#[cfg(test)]
mod mock;

pub use client::{
    ConnectionInfo, JobProgress, PrinterClient, PrinterError, PrinterState, StatusSnapshot,
    Temperature, TEMPERATURE_AMBIENT, make_client,
};
pub use esp3d::Esp3dClient;
pub use octoprint::OctoPrintClient;
pub use target::{PrinterTarget, ProtocolKind};
pub use transport::{HttpTransport, ReqwestTransport};
