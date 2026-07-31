//! Native Moonraker REST API client.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! Moonraker exposes a native REST API at endpoints like `/printer/info`,
//! `/printer/objects/query`, `/server/files/upload`, etc.  This client talks
//! directly to those endpoints instead of going through the `octoprint_compat`
//! shim, giving access to Moonraker-specific features like:
//! - Printer object queries (extruder, heater_bed, print_stats, etc.)
//! - Moonraker-native file management
//! - Macro execution
//! - Server info and component list

use crate::client::{
    ConnectionInfo, JobProgress, PrinterClient, PrinterError, PrinterState, StatusSnapshot,
    Temperature, TEMPERATURE_AMBIENT,
};
use crate::target::{PrinterTarget, ProtocolKind};
use crate::transport::HttpTransport;

/// Native Moonraker REST API client.
pub struct MoonrakerClient {
    _target: PrinterTarget,
    transport: Box<dyn HttpTransport>,
    info: std::sync::Mutex<ConnectionInfo>,
}

impl MoonrakerClient {
    pub fn new(target: PrinterTarget, transport: Box<dyn HttpTransport>) -> Self {
        let info = ConnectionInfo {
            protocol: ProtocolKind::MoonrakerCompat, // Moonraker native
            host: target.base_url.clone(),
            connected: false,
            firmware: None,
        };
        MoonrakerClient {
            _target: target,
            transport,
            info: std::sync::Mutex::new(info),
        }
    }
}

impl PrinterClient for MoonrakerClient {
    fn connection_info(&self) -> ConnectionInfo {
        self.info.lock().unwrap().clone()
    }

    fn test_connection(&self) -> Result<ConnectionInfo, PrinterError> {
        let resp = self.transport.get("/server/info")?;
        let firmware = extract_json_string(&resp, "software_version");
        let mut info = self.info.lock().unwrap();
        info.connected = true;
        info.firmware = firmware.clone();
        Ok(info.clone())
    }

    fn status(&self) -> Result<StatusSnapshot, PrinterError> {
        // Query printer objects in a single request.
        let resp = self
            .transport
            .get("/printer/objects/query?heater_bed&extruder&print_stats&display_status")?;

        let state = extract_print_state(&resp);
        let temps = extract_temperatures_moonraker(&resp);
        let progress = extract_job_progress_moonraker(&resp);

        let info = self.info.lock().unwrap();
        let firmware = info.firmware.clone();

        Ok(StatusSnapshot {
            state,
            temps,
            progress,
            firmware,
        })
    }

    fn upload_gcode(&self, filename: &str, gcode: &[u8]) -> Result<(), PrinterError> {
        self.transport
            .upload("/server/files/upload", filename, gcode, &[])?;
        Ok(())
    }

    fn start_print(&self, filename: &str) -> Result<(), PrinterError> {
        let body = r#"{"print": true}"#;
        self.transport.post_text(
            &format!("/server/files/print_local/{}", filename),
            body,
            "application/json",
        )?;
        Ok(())
    }

    fn pause(&self) -> Result<(), PrinterError> {
        let body = r#"{"action": "pause"}"#;
        self.transport
            .post_text("/printer/print/pause", body, "application/json")?;
        Ok(())
    }

    fn resume(&self) -> Result<(), PrinterError> {
        let body = r#"{"action": "resume"}"#;
        self.transport
            .post_text("/printer/print/resume", body, "application/json")?;
        Ok(())
    }

    fn cancel(&self) -> Result<(), PrinterError> {
        let body = r#"{"action": "cancel"}"#;
        self.transport
            .post_text("/printer/print/cancel", body, "application/json")?;
        Ok(())
    }

    fn send_gcode(&self, line: &str) -> Result<String, PrinterError> {
        let body = format!(r#"{{"script": "{}"}}"#, escape_json(line));
        let resp = self
            .transport
            .post_text("/printer/gcode/script", &body, "application/json")?;
        Ok(resp)
    }
}

/// Extract a string field from a JSON response (simplified parser).
fn extract_json_string(json: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{key}\"");
    let idx = json.find(&pattern)?;
    let rest = &json[idx + pattern.len()..];
    // Skip ": " or ":"
    let rest = rest.trim_start_matches([':', ' ']);
    if let Some(rest) = rest.strip_prefix('"') {
        let end = rest.find('"')?;
        Some(rest[..end].to_string())
    } else {
        None
    }
}

/// Extract temperature readings from Moonraker query response.
fn extract_temperatures_moonraker(json: &str) -> Temperature {
    let tool_actual =
        extract_nested_f64(json, "extruder", "temperature").unwrap_or(TEMPERATURE_AMBIENT);
    let tool_target = extract_nested_f64(json, "extruder", "target").unwrap_or(TEMPERATURE_AMBIENT);
    let bed_actual =
        extract_nested_f64(json, "heater_bed", "temperature").unwrap_or(TEMPERATURE_AMBIENT);
    let bed_target =
        extract_nested_f64(json, "heater_bed", "target").unwrap_or(TEMPERATURE_AMBIENT);

    Temperature {
        tool: tool_actual,
        tool_target,
        bed: bed_actual,
        bed_target,
    }
}

/// Extract print state from Moonraker response.
fn extract_print_state(json: &str) -> PrinterState {
    // Check print_stats.state
    if let Some(state_str) = extract_nested_string(json, "print_stats", "state") {
        match state_str.to_ascii_lowercase().as_str() {
            "standby" | "idle" => return PrinterState::Idle,
            "printing" => return PrinterState::Printing,
            "paused" | "pause" => return PrinterState::Paused,
            "complete" | "finished" => return PrinterState::Completed,
            "error" | "cancelled" => return PrinterState::Error,
            _ => {}
        }
    }
    // Fallback: check display_status.progress.
    if let Some(p) = extract_nested_f64(json, "display_status", "progress") {
        if p >= 1.0 {
            return PrinterState::Completed;
        } else if p > 0.0 {
            return PrinterState::Printing;
        }
    }
    PrinterState::Idle
}

/// Extract job progress from Moonraker response.
fn extract_job_progress_moonraker(json: &str) -> Option<JobProgress> {
    let completion = extract_nested_f64(json, "display_status", "progress")?;
    let file = extract_nested_string(json, "print_stats", "filename").map(|s| s.to_string());
    Some(JobProgress {
        completion: completion.clamp(0.0, 1.0),
        file,
        time_left_s: None,
    })
}

/// Extract a nested f64 value: `{"key": {"subkey": value}}`.
fn extract_nested_f64(json: &str, outer_key: &str, inner_key: &str) -> Option<f64> {
    let outer_pattern = format!("\"{outer_key}\"");
    let outer_idx = json.find(&outer_pattern)?;
    let rest = &json[outer_idx..];
    let inner_pattern = format!("\"{inner_key}\"");
    let inner_idx = rest.find(&inner_pattern)?;
    let rest = &rest[inner_idx + inner_pattern.len()..];
    let rest = rest.trim_start_matches([':', ' ']);
    // Read the number.
    let end = rest
        .find(|c: char| {
            !c.is_ascii_digit() && c != '.' && c != '-' && c != 'e' && c != 'E' && c != '+'
        })
        .unwrap_or(rest.len());
    let val_str = &rest[..end];
    val_str.parse::<f64>().ok()
}

/// Extract a nested string value: `{"key": {"subkey": "value"}}`.
fn extract_nested_string<'a>(json: &'a str, outer_key: &str, inner_key: &str) -> Option<&'a str> {
    let outer_pattern = format!("\"{outer_key}\"");
    let outer_idx = json.find(&outer_pattern)?;
    let rest = &json[outer_idx..];
    let inner_pattern = format!("\"{inner_key}\"");
    let inner_idx = rest.find(&inner_pattern)?;
    let rest = &rest[inner_idx + inner_pattern.len()..];
    let rest = rest.trim_start_matches([':', ' ']);
    if let Some(rest) = rest.strip_prefix('"') {
        let end = rest.find('"')?;
        Some(&rest[..end])
    } else {
        None
    }
}

/// Escape a string for JSON embedding.
fn escape_json(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_json_string_basic() {
        let json = r#"{"software_version": "v0.8.0-123"}"#;
        assert_eq!(
            extract_json_string(json, "software_version"),
            Some("v0.8.0-123".to_string())
        );
    }

    #[test]
    fn extract_nested_f64_basic() {
        let json = r#"{"extruder": {"temperature": 210.5, "target": 210.0}}"#;
        assert!(
            (extract_nested_f64(json, "extruder", "temperature").unwrap() - 210.5).abs() < 1e-6
        );
        assert!((extract_nested_f64(json, "extruder", "target").unwrap() - 210.0).abs() < 1e-6);
    }

    #[test]
    fn extract_nested_string_basic() {
        let json = r#"{"print_stats": {"filename": "test.gcode"}}"#;
        assert_eq!(
            extract_nested_string(json, "print_stats", "filename"),
            Some("test.gcode")
        );
    }

    #[test]
    fn escape_json_handles_special_chars() {
        assert_eq!(escape_json("hello\"world"), "hello\\\"world");
        assert_eq!(escape_json("line\nbreak"), "line\\nbreak");
    }
}
