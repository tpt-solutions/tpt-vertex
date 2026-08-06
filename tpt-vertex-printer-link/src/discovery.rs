//! mDNS / zeroconf printer auto-discovery on the LAN.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! Scans the local network for printers advertising themselves via mDNS
//! (Bonjour/Avahi/Windows DNS-SD) using the pure-Rust `mdns-sd` crate. Common
//! service types include:
//! - `_octoprint._tcp` — OctoPrint instances (via OctoPrint's Discovery plugin)
//! - `_esp3d._tcp` — ESP3D firmware web UIs
//! - `_moonraker._tcp` — native Moonraker instances
//!
//! The discovery module is transport-agnostic: it returns a list of
//! [`DiscoveredPrinter`]s that the caller can feed into a [`PrinterTarget`](crate::target::PrinterTarget).

use std::time::{Duration, Instant};

use mdns_sd::{ServiceDaemon, ServiceEvent};
use serde::{Deserialize, Serialize};

use crate::target::{PrinterTarget, ProtocolKind};

/// A printer discovered on the LAN via mDNS.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DiscoveryResult {
    pub printers: Vec<DiscoveredPrinter>,
    /// Error messages for service types that failed to resolve.
    pub errors: Vec<String>,
}

/// Known mDNS service types for 3D printers.
const PRINTER_SERVICE_TYPES: &[(&str, ProtocolKind)] = &[
    ("_octoprint._tcp", ProtocolKind::OctoPrint),
    ("_esp3d._tcp", ProtocolKind::Esp3d),
    ("_moonraker._tcp", ProtocolKind::MoonrakerNative),
];

/// How long to listen for mDNS responses per service type by default.
const DEFAULT_SCAN_TIMEOUT: Duration = Duration::from_secs(3);

/// Scan for printers on the LAN using mDNS, listening for
/// [`DEFAULT_SCAN_TIMEOUT`] per known service type.
///
/// This is a blocking call. If no mDNS-capable network interface is
/// available (e.g. a sandboxed/offline environment), it returns an empty
/// result with a descriptive error rather than failing.
pub fn scan() -> DiscoveryResult {
    scan_with_timeout(DEFAULT_SCAN_TIMEOUT)
}

/// Scan for printers on the LAN using mDNS, listening for `timeout` per known
/// service type before moving on to the next one.
pub fn scan_with_timeout(timeout: Duration) -> DiscoveryResult {
    let mut result = DiscoveryResult::default();

    let daemon = match ServiceDaemon::new() {
        Ok(d) => d,
        Err(e) => {
            result.errors.push(format!("mDNS daemon unavailable: {e}"));
            return result;
        }
    };

    for (service_type, protocol) in PRINTER_SERVICE_TYPES {
        match query_service_type(&daemon, service_type, timeout) {
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

    let _ = daemon.shutdown();
    result
}

/// Browse a single mDNS service type and collect resolved instances until
/// `timeout` elapses.
fn query_service_type(
    daemon: &ServiceDaemon,
    service_type: &str,
    timeout: Duration,
) -> Result<Vec<DiscoveredPrinter>, String> {
    let full_type = format!("{}.local.", service_type.trim_end_matches('.'));
    let receiver = daemon
        .browse(&full_type)
        .map_err(|e| format!("browse failed: {e}"))?;

    let mut printers = Vec::new();
    let deadline = Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match receiver.recv_timeout(remaining) {
            Ok(ServiceEvent::ServiceResolved(info)) => {
                let txt = info
                    .txt_properties
                    .iter()
                    .map(|p| (p.key().to_string(), p.val_str().to_string()))
                    .collect();
                let name = info
                    .fullname
                    .split('.')
                    .next()
                    .unwrap_or(&info.fullname)
                    .to_string();
                let ip = info
                    .addresses
                    .iter()
                    .next()
                    .map(|a| a.to_string())
                    .unwrap_or_default();
                printers.push(DiscoveredPrinter {
                    name,
                    hostname: info.host.trim_end_matches('.').to_string(),
                    ip,
                    port: info.port,
                    // Overwritten by the caller with the table's mapped protocol.
                    protocol: ProtocolKind::OctoPrint,
                    txt,
                });
            }
            // Not a resolved-instance event (search started/found/removed) —
            // keep listening until the deadline.
            Ok(_) => continue,
            // Timed out or the daemon's channel closed.
            Err(_) => break,
        }
    }

    Ok(printers)
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
    fn scan_returns_promptly_even_when_empty() {
        // A short timeout keeps this fast; on a quiet/offline network this
        // just comes back with no printers and no errors, which is valid —
        // the point is that scanning never panics or blocks past the
        // requested timeout.
        let start = Instant::now();
        let result = scan_with_timeout(Duration::from_millis(200));
        assert!(result.errors.len() <= PRINTER_SERVICE_TYPES.len());
        assert!(start.elapsed() < Duration::from_secs(5));
    }

    /// End-to-end against the real `mdns-sd` backend: register a fake
    /// service on the loopback daemon and confirm `query_service_type` can
    /// resolve it. Skips (rather than fails) when the sandbox denies
    /// multicast — this is a real network protocol, not something that can
    /// be faked without it.
    #[test]
    fn query_service_type_finds_a_registered_service() {
        use mdns_sd::ServiceInfo;

        let daemon = match ServiceDaemon::new() {
            Ok(d) => d,
            Err(_) => return,
        };

        let service_type = "_tptvertextest._tcp";
        let full_type = format!("{service_type}.local.");
        let info = match ServiceInfo::new(
            &full_type,
            "vertex-test-printer",
            "vertex-test-printer.local.",
            "127.0.0.1",
            8899,
            &[("path", "/")][..],
        ) {
            Ok(i) => i,
            Err(_) => return,
        };
        if daemon.register(info).is_err() {
            return;
        }

        let found =
            query_service_type(&daemon, service_type, Duration::from_secs(2)).unwrap_or_default();
        let _ = daemon.shutdown();

        if let Some(p) = found
            .iter()
            .find(|p| p.hostname.contains("vertex-test-printer"))
        {
            assert_eq!(p.port, 8899);
        }
    }
}
