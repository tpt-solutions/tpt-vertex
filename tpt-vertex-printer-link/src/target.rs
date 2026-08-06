//! Printer connection targets (distinct from the physical `PrinterProfile`).
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0

use serde::{Deserialize, Serialize};

/// The network protocol/control surface a printer exposes.
///
/// This describes *how Vertex talks to* a printer, not the printer's physical
/// capabilities (which live in `tpt-vertex-slicer`'s [`PrinterProfile`]).
///
/// The `serde` renames below match [`ProtocolKind::as_str`] exactly, so JSON
/// sent over the Tauri IPC boundary (and thus the frontend's TS union) uses
/// the same lowercase-dash strings as the rest of the crate's serialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProtocolKind {
    /// ESP3D Web UI firmware (G-code-over-HTTP + multipart upload).
    #[serde(rename = "esp3d")]
    Esp3d,
    /// Native OctoPrint REST API.
    #[serde(rename = "octoprint")]
    OctoPrint,
    /// Moonraker's OctoPrint-compatibility shim (`octoprint_compat`).
    #[serde(rename = "moonraker-compat")]
    MoonrakerCompat,
    /// Native Moonraker REST API (no OctoPrint compat shim).
    #[serde(rename = "moonraker")]
    MoonrakerNative,
}

impl ProtocolKind {
    /// Stable string used in serialization and UI labels.
    pub fn as_str(&self) -> &'static str {
        match self {
            ProtocolKind::Esp3d => "esp3d",
            ProtocolKind::OctoPrint => "octoprint",
            ProtocolKind::MoonrakerCompat => "moonraker-compat",
            ProtocolKind::MoonrakerNative => "moonraker",
        }
    }

    /// Parse a string (case-insensitive) back into a [`ProtocolKind`].
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "esp3d" => Some(ProtocolKind::Esp3d),
            "octoprint" => Some(ProtocolKind::OctoPrint),
            "moonraker-compat" | "moonraker_compat" => Some(ProtocolKind::MoonrakerCompat),
            "moonraker" => Some(ProtocolKind::MoonrakerNative),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The JSON wire representation (what the frontend sends/receives over
    /// the Tauri IPC boundary) must match [`ProtocolKind::as_str`] exactly.
    #[test]
    fn protocol_kind_json_matches_as_str() {
        for kind in [
            ProtocolKind::Esp3d,
            ProtocolKind::OctoPrint,
            ProtocolKind::MoonrakerCompat,
            ProtocolKind::MoonrakerNative,
        ] {
            let json = serde_json::to_string(&kind).unwrap();
            assert_eq!(json, format!("\"{}\"", kind.as_str()));
            let round_tripped: ProtocolKind = serde_json::from_str(&json).unwrap();
            assert_eq!(round_tripped, kind);
        }
    }
}
