//! OctoPrint (and Moonraker `octoprint_compat`) REST client.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! Both OctoPrint and Moonraker's OctoPrint-compatibility shim expose the same
//! REST surface: `/api/version`, `/api/printer`, `/api/job`, and
//! `/api/files/local`. The only practical difference is the firmware label, so
//! a single client (`OctoPrintClient`) handles both; the `compat` flag merely
//! relabels the connection as Moonraker.

use crate::client::{
    ConnectionInfo, JobProgress, PrinterClient, PrinterError, PrinterState, StatusSnapshot,
    Temperature,
};
use crate::target::PrinterTarget;
use crate::transport::HttpTransport;
use std::sync::Mutex;

/// Client for OctoPrint or Moonraker's OctoPrint-compat API.
pub struct OctoPrintClient {
    #[allow(dead_code)]
    target: PrinterTarget,
    transport: Box<dyn HttpTransport>,
    /// True when this is Moonraker's `octoprint_compat` shim (label only).
    compat: bool,
    info: Mutex<ConnectionInfo>,
}

impl OctoPrintClient {
    /// Create a client for `target`, talking through `transport`.
    ///
    /// `compat` should be true when the target is a Moonraker instance exposing
    /// the OctoPrint API via its `octoprint_compat` component.
    pub fn new(target: PrinterTarget, transport: Box<dyn HttpTransport>, compat: bool) -> Self {
        let info = ConnectionInfo {
            protocol: target.kind,
            host: target.normalized_base(),
            connected: false,
            firmware: None,
        };
        OctoPrintClient {
            target,
            transport,
            compat,
            info: Mutex::new(info),
        }
    }

    fn post_json(&self, path: &str, body: &serde_json::Value) -> Result<(), PrinterError> {
        let text = self
            .transport
            .post(path, &serde_json::to_vec(body).unwrap_or_default(), "application/json")?;
        // 204 No Content is normal for command endpoints; ignore empty bodies.
        if text.trim().is_empty() {
            return Ok(());
        }
        // Some endpoints return an error object on failure; surface it.
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(err) = v.get("error").and_then(|e| e.as_str()) {
                return Err(PrinterError::Http(err.to_string()));
            }
        }
        Ok(())
    }
}

impl PrinterClient for OctoPrintClient {
    fn connection_info(&self) -> ConnectionInfo {
        self.info.lock().unwrap().clone()
    }

    fn test_connection(&self) -> Result<ConnectionInfo, PrinterError> {
        let body = self.transport.get("/api/version")?;
        let v: serde_json::Value = serde_json::from_str(&body)
            .map_err(|e| PrinterError::Parse(format!("version json: {e}")))?;
        let version = v.get("version").and_then(|s| s.as_str()).unwrap_or("?");
        let firmware = if self.compat {
            Some(format!("Moonraker (OctoPrint compat) {version}"))
        } else {
            Some(format!("OctoPrint {version}"))
        };
        let mut info = self.info.lock().unwrap();
        info.connected = true;
        info.firmware = firmware.clone();
        Ok(info.clone())
    }

    fn status(&self) -> Result<StatusSnapshot, PrinterError> {
        let printer = self.transport.get("/api/printer")?;
        let pv: serde_json::Value = serde_json::from_str(&printer)
            .map_err(|e| PrinterError::Parse(format!("printer json: {e}")))?;

        let temps = parse_temps(&pv);

        let state = match pv.get("state").and_then(|s| s.get("flags")) {
            Some(flags) => {
                if flags.get("printing").and_then(|b| b.as_bool()).unwrap_or(false) {
                    PrinterState::Printing
                } else if flags.get("paused").and_then(|b| b.as_bool()).unwrap_or(false) {
                    PrinterState::Paused
                } else if flags.get("error").and_then(|b| b.as_bool()).unwrap_or(false)
                    || flags.get("closedOrError").and_then(|b| b.as_bool()).unwrap_or(false)
                {
                    PrinterState::Error
                } else {
                    PrinterState::Idle
                }
            }
            None => PrinterState::Idle,
        };

        // Job/progress is a separate endpoint; missing it is not fatal.
        let progress = self
            .transport
            .get("/api/job")
            .ok()
            .and_then(|b| serde_json::from_str::<serde_json::Value>(&b).ok())
            .and_then(|j| parse_job(&j));

        let firmware = self.info.lock().unwrap().firmware.clone();
        Ok(StatusSnapshot {
            state,
            temps,
            progress,
            firmware,
        })
    }

    fn upload_gcode(&self, filename: &str, gcode: &[u8]) -> Result<(), PrinterError> {
        // OctoPrint requires the `file` part plus a `print` field (we upload
        // without auto-starting; `start_print` selects + prints afterwards).
        self.transport
            .upload("/api/files/local", filename, gcode, &[("print", "false")])?;
        Ok(())
    }

    fn start_print(&self, filename: &str) -> Result<(), PrinterError> {
        let path = format!("local/{}", filename);
        let body = serde_json::json!({ "command": "select", "file": path, "print": true });
        self.post_json("/api/job", &body)
    }

    fn pause(&self) -> Result<(), PrinterError> {
        self.post_json("/api/job", &serde_json::json!({ "command": "pause", "action": "pause" }))
    }

    fn resume(&self) -> Result<(), PrinterError> {
        self.post_json("/api/job", &serde_json::json!({ "command": "pause", "action": "resume" }))
    }

    fn cancel(&self) -> Result<(), PrinterError> {
        self.post_json("/api/job", &serde_json::json!({ "command": "cancel" }))
    }

    fn send_gcode(&self, line: &str) -> Result<String, PrinterError> {
        let body = serde_json::json!({ "commands": [line] });
        self.post_json("/api/printer/command", &body)?;
        Ok("ok".to_string())
    }
}

/// Read tool/bed temperatures out of an `/api/printer` payload.
fn parse_temps(pv: &serde_json::Value) -> Temperature {
    let mut t = Temperature::default();
    let temp = pv.get("temperature");
    if let Some(node) = temp.and_then(|x| x.get("tool0")).or_else(|| temp.and_then(|x| x.get("tool"))) {
        t.tool = node.get("actual").and_then(|v| v.as_f64()).unwrap_or(f64::NAN);
        t.tool_target = node.get("target").and_then(|v| v.as_f64()).unwrap_or(f64::NAN);
    }
    if let Some(bed) = temp.and_then(|x| x.get("bed")) {
        t.bed = bed.get("actual").and_then(|v| v.as_f64()).unwrap_or(f64::NAN);
        t.bed_target = bed.get("target").and_then(|v| v.as_f64()).unwrap_or(f64::NAN);
    }
    t
}

/// Read job progress out of an `/api/job` payload.
fn parse_job(j: &serde_json::Value) -> Option<JobProgress> {
    let progress_node = j.get("progress")?;
    // OctoPrint reports `completion` as a percentage (0..=100); normalize to a
    // 0..=1 fraction to match [`JobProgress::completion`].
    let completion = progress_node.get("completion").and_then(|v| v.as_f64())? / 100.0;
    let file = j
        .get("job")
        .and_then(|jb| jb.get("file"))
        .and_then(|f| f.get("name"))
        .and_then(|n| n.as_str())
        .map(|s| s.to_string());
    let time_left_s = progress_node.get("printTimeLeft").and_then(|v| v.as_f64());
    Some(JobProgress {
        completion: completion.clamp(0.0, 1.0),
        file,
        time_left_s,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockTransport;
    use crate::target::ProtocolKind;
    use serde_json::json;

    fn target() -> PrinterTarget {
        PrinterTarget::new("p2", "Octo", ProtocolKind::OctoPrint, "http://octo.local", Some("KEY".into()))
    }

    fn mock() -> MockTransport {
        let printer = json!({
            "state": { "text": "Printing", "flags": { "operational": true, "printing": true, "paused": false } },
            "temperature": { "tool0": { "actual": 205.0, "target": 210.0 }, "bed": { "actual": 59.0, "target": 60.0 } }
        })
        .to_string();
        let job = json!({
            "job": { "file": { "name": "part.gcode" } },
            "progress": { "completion": 42.0, "printTimeLeft": 600 }
        })
        .to_string();
        MockTransport::new()
            .respond("/api/version", r#"{"server":"OctoPrint","version":"1.9.0","api":"0.1"}"#)
            .respond("/api/printer", &printer)
            .respond("/api/job", &job)
            .respond("/api/files/local", "")
            .respond("/api/printer/command", "")
    }

    #[test]
    fn connect_labels_octoprint() {
        let c = OctoPrintClient::new(target(), Box::new(mock()), false);
        let info = c.test_connection().unwrap();
        assert!(info.connected);
        assert_eq!(info.firmware.as_deref(), Some("OctoPrint 1.9.0"));
    }

    #[test]
    fn connect_labels_moonraker_compat() {
        let mut t = target();
        t.kind = ProtocolKind::MoonrakerCompat;
        let c = OctoPrintClient::new(t, Box::new(mock()), true);
        let info = c.test_connection().unwrap();
        assert!(info.firmware.unwrap().contains("Moonraker"));
    }

    #[test]
    fn status_maps_flags_and_temps() {
        let c = OctoPrintClient::new(target(), Box::new(mock()), false);
        let s = c.status().unwrap();
        assert_eq!(s.state, PrinterState::Printing);
        assert!((s.temps.tool - 205.0).abs() < 1e-9);
        assert!((s.temps.bed - 59.0).abs() < 1e-9);
        let p = s.progress.unwrap();
        assert!((p.completion - 0.42).abs() < 1e-9);
        assert_eq!(p.file.as_deref(), Some("part.gcode"));
        assert_eq!(p.time_left_s, Some(600.0));
    }

    #[test]
    fn upload_then_start_posts_select_with_print() {
        let t = mock();
        let uploads = t.uploads.clone();
        let posts = t.posts.clone();
        let c = OctoPrintClient::new(target(), Box::new(t), false);
        c.upload_gcode("part.gcode", b"G1 X10").unwrap();
        let uploads = uploads.lock().unwrap();
        assert_eq!(uploads.len(), 1);
        assert_eq!(uploads[0].0, "part.gcode");
        drop(uploads);

        c.start_print("part.gcode").unwrap();
        let posts = posts.lock().unwrap();
        let job_post = posts.iter().find(|(p, _)| p == "/api/job").expect("job posted");
        let body: serde_json::Value = serde_json::from_slice(&job_post.1).unwrap();
        assert_eq!(body["command"], "select");
        assert_eq!(body["file"], "local/part.gcode");
        assert_eq!(body["print"], true);
    }

    #[test]
    fn pause_resume_cancel_post_job_commands() {
        let t = mock();
        let posts = t.posts.clone();
        let c = OctoPrintClient::new(target(), Box::new(t), false);
        c.pause().unwrap();
        c.resume().unwrap();
        c.cancel().unwrap();
        let posts = posts.lock().unwrap();
        let actions: Vec<String> = posts
            .iter()
            .filter(|(p, _)| p == "/api/job")
            .map(|(_, b)| {
                serde_json::from_slice::<serde_json::Value>(b)
                    .ok()
                    .and_then(|v| v["command"].as_str().map(str::to_string))
                    .unwrap_or_default()
            })
            .collect();
        assert!(actions.contains(&"pause".to_string()));
        assert!(actions.contains(&"cancel".to_string()));
    }

    #[test]
    fn malformed_printer_json_is_parse_error() {
        let t = MockTransport::new().respond("/api/printer", "not json");
        let c = OctoPrintClient::new(target(), Box::new(t), false);
        assert!(matches!(c.status(), Err(PrinterError::Parse(_))));
    }
}
