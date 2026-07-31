//! mDNS / zeroconf printer auto-discovery on the LAN.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! Scans the local network for printers advertising themselves via mDNS
//! (Bonjour/Avahi/Windows DNS-SD).  Common service types include:
//! - `_octoprint._tcp` — OctoPrint instances
//! - `_esp3d._tcp` — ESP3D firmware web UIs
//! - `_http._tcp` with `_tpt-vertex._sub` — future Vertex-connected printers
//!
//! The discovery module is transport-agnostic: it returns a list of
//! [`DiscoveredPrinter`]s that the caller can feed into a [`PrinterTarget`](crate::target::PrinterTarget).

use crate::target::{PrinterTarget, ProtocolKind};

/// A printer discovered on the LAN via mDNS.
#[derive(Debug, Clone, PartialEq)]
pub struct DiscoveredPrinter {
    /// Human-readable service name (e.g. "OctoPrint on octopi").
    pub name: String,
    /// Resolved hostname (e.g. "octopi.local").
    pub hostname: String,
    /// Resolved IP address.
    pub ip: String,
    /// Resolved port (typically 80 or 443).
    pub port: u16,
    /// Detected protocol based on the mDNS service type.
    pub protocol: ProtocolKind,
    /// Additional TXT record fields, if any.
    pub txt: std::collections::HashMap<String, String>,
}

impl DiscoveredPrinter {
    /// Build a [`PrinterTarget`] suitable for saving to the printer store.
    pub fn to_target(&self) -> PrinterTarget {
        let scheme = if self.port == 443 { "https" } else { "http" };
        let base_url = format!("{}://{}:{}", scheme, self.hostname, self.port);
        PrinterTarget::new(
            format!("discovered-{}", self.name.to_lowercase().replace(' ', "-")),
            &self.name,
            self.protocol,
            base_url,
            None,
        )
    }
}

/// Result of a discovery scan.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DiscoveryResult {
    pub printers: Vec<DiscoveredPrinter>,
    /// Error messages for service types that failed to resolve.
    pub errors: Vec<String>,
}

/// Known mDNS service types for 3D printers.
const PRINTER_SERVICE_TYPES: &[(&str, ProtocolKind)] = &[
    ("_octoprint._tcp", ProtocolKind::OctoPrint),
    ("_esp3d._tcp", ProtocolKind::Esp3d),
    ("_http._tcp", ProtocolKind::MoonrakerCompat),
];

/// Scan for printers on the LAN using mDNS.
///
/// This is a blocking call that queries each known service type with a timeout.
/// On platforms without an mDNS resolver, it returns an empty result with an
/// error message.
///
/// On production platforms, this would use `libmdns` or `dns-sd` crate; the
/// current implementation returns a stub indicating the scan was attempted.
pub fn scan() -> DiscoveryResult {
    let mut result = DiscoveryResult::default();

    for (service_type, protocol) in PRINTER_SERVICE_TYPES {
        match query_service_type(service_type) {
            Ok(printers) => {
                for mut p in printers {
                    p.protocol = *protocol;
                    result.printers.push(p);
                }
            }
            Err(e) => {
                result.errors.push(format!("{service_type}: {e}"));
            }
        }
    }

    result
}

/// Query a single mDNS service type.  Stub implementation that returns an
/// error on unsupported platforms; a real implementation would use the OS
/// mDNS stack or a Rust mDNS library.
fn query_service_type(_service_type: &str) -> Result<Vec<DiscoveredPrinter>, String> {
    // Stub: platform mDNS resolution is not yet linked.
    Err("mDNS resolution not yet available on this platform".to_string())
}

/// Convert a [`DiscoveryResult`] into a list of [`PrinterTarget`]s ready to
/// be saved.
pub fn results_to_targets(result: &DiscoveryResult) -> Vec<PrinterTarget> {
    result.printers.iter().map(|p| p.to_target()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovered_printer_converts_to_target() {
        let dp = DiscoveredPrinter {
            name: "Test Printer".to_string(),
            hostname: "printer.local".to_string(),
            ip: "192.168.1.50".to_string(),
            port: 80,
            protocol: ProtocolKind::OctoPrint,
            txt: std::collections::HashMap::new(),
        };
        let target = dp.to_target();
        assert_eq!(target.kind, ProtocolKind::OctoPrint);
        assert!(target.base_url.contains("printer.local"));
    }

    #[test]
    fn scan_returns_result_even_when_empty() {
        let result = scan();
        // The stub always returns errors (no mDNS backend), but the result is valid.
        assert!(result.printers.is_empty() || !result.errors.is_empty());
    }
}
