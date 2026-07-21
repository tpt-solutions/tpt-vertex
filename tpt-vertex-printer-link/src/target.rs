//! Printer connection targets (distinct from the physical `PrinterProfile`).
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0

use serde::{Deserialize, Serialize};

/// The network protocol/control surface a printer exposes.
///
/// This describes *how Vertex talks to* a printer, not the printer's physical
/// capabilities (which live in `tpt-vertex-slicer`'s [`PrinterProfile`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProtocolKind {
    /// ESP3D Web UI firmware (G-code-over-HTTP + multipart upload).
    Esp3d,
    /// Native OctoPrint REST API.
    OctoPrint,
    /// Moonraker's OctoPrint-compatibility shim (`octoprint_compat`).
    MoonrakerCompat,
}

impl ProtocolKind {
    /// Stable string used in serialization and UI labels.
    pub fn as_str(&self) -> &'static str {
        match self {
            ProtocolKind::Esp3d => "esp3d",
            ProtocolKind::OctoPrint => "octoprint",
            ProtocolKind::MoonrakerCompat => "moonraker-compat",
        }
    }

    /// Parse a string (case-insensitive) back into a [`ProtocolKind`].
    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "esp3d" => Some(ProtocolKind::Esp3d),
            "octoprint" => Some(ProtocolKind::OctoPrint),
            "moonraker" | "moonraker-compat" | "moonraker_compat" => Some(ProtocolKind::MoonrakerCompat),
            _ => None,
        }
    }
}

/// A saved printer connection: where it lives on the network and how to talk
/// to it. Persisted by the desktop client (see `tauri-plugin-store`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrinterTarget {
    /// Stable id (UUID or slug) used as the persistence key.
    pub id: String,
    /// Human-friendly display name.
    pub name: String,
    /// Which control protocol the printer speaks.
    pub kind: ProtocolKind,
    /// Base URL, e.g. `http://192.168.1.50` (ESP3D) or `http://octopi.local`
    /// (OctoPrint/Moonraker). Trailing slashes are tolerated.
    pub base_url: String,
    /// API key / access token required by some protocols (OctoPrint, Moonraker).
    /// ESP3D typically does not require one.
    pub api_key: Option<String>,
}

impl PrinterTarget {
    /// Build a new target with an explicit id.
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        kind: ProtocolKind,
        base_url: impl Into<String>,
        api_key: Option<String>,
    ) -> Self {
        PrinterTarget {
            id: id.into(),
            name: name.into(),
            kind,
            base_url: base_url.into(),
            api_key,
        }
    }

    /// Normalize the base URL by trimming trailing slashes.
    pub fn normalized_base(&self) -> String {
        self.base_url.trim_end_matches('/').to_string()
    }
}
