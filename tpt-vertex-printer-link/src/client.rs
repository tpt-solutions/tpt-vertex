//! Core client trait, shared types, errors, and the [`make_client`] factory.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0

use crate::esp3d::Esp3dClient;
use crate::octoprint::OctoPrintClient;
use crate::target::{PrinterTarget, ProtocolKind};
use crate::transport::{HttpTransport, ReqwestTransport};

/// Ambient "no reading" sentinel for temperature fields.
pub const TEMPERATURE_AMBIENT: f64 = f64::NAN;

/// Errors surfaced by any [`PrinterClient`] operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrinterError {
    /// Underlying HTTP/transport failure (connection refused, DNS, TLS, read).
    Transport(String),
    /// An HTTP error status that isn't auth/not-found/timeout.
    Http(String),
    /// Authentication rejected (401/403).
    Auth(String),
    /// Resource missing (404).
    NotFound(String),
    /// Request timed out.
    Timeout(String),
    /// Response could not be parsed (wrong shape / malformed reply).
    Parse(String),
}

impl std::fmt::Display for PrinterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PrinterError::Transport(s) => write!(f, "transport error: {s}"),
            PrinterError::Http(s) => write!(f, "http error: {s}"),
            PrinterError::Auth(s) => write!(f, "auth error: {s}"),
            PrinterError::NotFound(s) => write!(f, "not found: {s}"),
            PrinterError::Timeout(s) => write!(f, "timeout: {s}"),
            PrinterError::Parse(s) => write!(f, "parse error: {s}"),
        }
    }
}

impl std::error::Error for PrinterError {}

/// Connection capability flags reported by [`PrinterClient::test_connection`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ConnectionInfo {
    /// Which protocol this client speaks.
    pub protocol: ProtocolKind,
    /// Host/printer the client is configured for.
    pub host: String,
    /// Whether the handshake succeeded.
    pub connected: bool,
    /// Detected firmware / server name, if reported.
    pub firmware: Option<String>,
}

/// Live printer state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PrinterState {
    /// Could not be reached.
    Disconnected,
    /// Idle / operational, nothing printing.
    Idle,
    /// Actively printing.
    Printing,
    /// Print paused.
    Paused,
    /// Print finished.
    Completed,
    /// Faulted / error state.
    Error,
}

impl PrinterState {
    /// Render a short human label.
    pub fn label(&self) -> &'static str {
        match self {
            PrinterState::Disconnected => "disconnected",
            PrinterState::Idle => "idle",
            PrinterState::Printing => "printing",
            PrinterState::Paused => "paused",
            PrinterState::Completed => "completed",
            PrinterState::Error => "error",
        }
    }
}

/// Temperature readings (°C). Missing readings use [`TEMPERATURE_AMBIENT`].
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Temperature {
    /// Hotend actual temperature.
    pub tool: f64,
    /// Hotend target temperature.
    pub tool_target: f64,
    /// Bed actual temperature.
    pub bed: f64,
    /// Bed target temperature.
    pub bed_target: f64,
}

impl Default for Temperature {
    fn default() -> Self {
        Temperature {
            tool: TEMPERATURE_AMBIENT,
            tool_target: TEMPERATURE_AMBIENT,
            bed: TEMPERATURE_AMBIENT,
            bed_target: TEMPERATURE_AMBIENT,
        }
    }
}

/// Print-job progress, when a job is active.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct JobProgress {
    /// Completion fraction 0..=1.
    pub completion: f64,
    /// Name of the file being printed.
    pub file: Option<String>,
    /// Estimated seconds remaining, if the firmware reports it.
    pub time_left_s: Option<f64>,
}

/// A point-in-time snapshot of the printer.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StatusSnapshot {
    pub state: PrinterState,
    pub temps: Temperature,
    pub progress: Option<JobProgress>,
    /// Detected firmware / server name.
    pub firmware: Option<String>,
}

/// Uniform control surface for any supported printer.
pub trait PrinterClient {
    /// Describe the connection this client talks to.
    fn connection_info(&self) -> ConnectionInfo;

    /// Probe the printer; returns capability info (and marks `connected`).
    fn test_connection(&self) -> Result<ConnectionInfo, PrinterError>;

    /// Fetch current temperatures, state, and job progress.
    fn status(&self) -> Result<StatusSnapshot, PrinterError>;

    /// Upload G-code bytes to the printer's local storage (does not start print).
    fn upload_gcode(&self, filename: &str, gcode: &[u8]) -> Result<(), PrinterError>;

    /// Select and start printing the previously uploaded `filename`.
    fn start_print(&self, filename: &str) -> Result<(), PrinterError>;

    /// Pause the active print.
    fn pause(&self) -> Result<(), PrinterError>;

    /// Resume a paused print.
    fn resume(&self) -> Result<(), PrinterError>;

    /// Cancel/abort the active print.
    fn cancel(&self) -> Result<(), PrinterError>;

    /// Send a single raw G-code line and return the printer's reply text.
    fn send_gcode(&self, line: &str) -> Result<String, PrinterError>;
}

/// Build the right client for a [`PrinterTarget`].
pub fn make_client(target: &PrinterTarget) -> Result<Box<dyn PrinterClient>, PrinterError> {
    let transport: Box<dyn HttpTransport> = Box::new(ReqwestTransport::new(
        &target.base_url,
        target.kind,
        target.api_key.as_deref(),
    )?);
    Ok(match target.kind {
        ProtocolKind::Esp3d => Box::new(Esp3dClient::new(target.clone(), transport)),
        ProtocolKind::OctoPrint => Box::new(OctoPrintClient::new(target.clone(), transport, false)),
        ProtocolKind::MoonrakerCompat => {
            Box::new(OctoPrintClient::new(target.clone(), transport, true))
        }
    })
}
