//! ESP3D Web UI client.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! ESP3D is the firmware running on many ESP32-based printer control boards and
//! exposes a tiny HTTP control surface: G-code is tunnelled through `GET
//! /?cmd=<gcode>` requests, and files are uploaded via a multipart `POST
//! /upload`. Print control maps onto the usual Marlin-style SD commands
//! (`M23` select, `M24` start/resume, `M25` pause, `M524` abort).

use crate::client::{
    ConnectionInfo, JobProgress, PrinterClient, PrinterError, PrinterState, StatusSnapshot,
    Temperature,
};
use crate::target::PrinterTarget;
use crate::transport::HttpTransport;
use std::sync::Mutex;

/// Client for an ESP3D-flashed printer.
pub struct Esp3dClient {
    #[allow(dead_code)]
    target: PrinterTarget,
    transport: Box<dyn HttpTransport>,
    info: Mutex<ConnectionInfo>,
}

impl Esp3dClient {
    /// Create a client for `target`, talking through `transport`.
    pub fn new(target: PrinterTarget, transport: Box<dyn HttpTransport>) -> Self {
        let info = ConnectionInfo {
            protocol: target.kind,
            host: target.normalized_base(),
            connected: false,
            firmware: None,
        };
        Esp3dClient {
            target,
            transport,
            info: Mutex::new(info),
        }
    }

    fn cmd(&self, gcode: &str) -> Result<String, PrinterError> {
        // ESP3D replies are plain text; the body usually starts with "ok".
        self.transport.get(&format!("/?cmd={}", gcode))
    }
}

impl PrinterClient for Esp3dClient {
    fn connection_info(&self) -> ConnectionInfo {
        self.info.lock().unwrap().clone()
    }

    fn test_connection(&self) -> Result<ConnectionInfo, PrinterError> {
        let reply = self.cmd("M115")?;
        let firmware = parse_firmware_name(&reply).or_else(|| Some("ESP3D".to_string()));
        let mut info = self.info.lock().unwrap();
        info.connected = true;
        info.firmware = firmware.clone();
        Ok(info.clone())
    }

    fn status(&self) -> Result<StatusSnapshot, PrinterError> {
        let temp_reply = self.cmd("M105")?;
        let progress_reply = self.cmd("M27").unwrap_or_default();

        let temps = parse_temps(&temp_reply);

        let (state, progress) = if let Some(frac) = parse_sd_progress(&progress_reply) {
            // ESP3D prints "SD printing byte X/Y"; pausing is reported as a
            // paused flag by some builds. We treat an active SD print as
            // Printing unless the reply explicitly mentions "pause".
            let paused = progress_reply.to_ascii_lowercase().contains("pause");
            let state = if paused { PrinterState::Paused } else { PrinterState::Printing };
            let progress = Some(JobProgress {
                completion: frac,
                file: parse_sd_filename(&progress_reply),
                time_left_s: None,
            });
            (state, progress)
        } else {
            (PrinterState::Idle, None)
        };

        let firmware = self.info.lock().unwrap().firmware.clone();
        Ok(StatusSnapshot {
            state,
            temps,
            progress,
            firmware,
        })
    }

    fn upload_gcode(&self, filename: &str, gcode: &[u8]) -> Result<(), PrinterError> {
        // ESP3D accepts a multipart upload with a `file` part. We do not set
        // the `print` field (OctoPrint-only) — upload and print are separate.
        self.transport.upload("/upload", filename, gcode, &[])?;
        Ok(())
    }

    fn start_print(&self, filename: &str) -> Result<(), PrinterError> {
        // Select the file on the printer's storage then start the SD print.
        self.cmd(&format!("M23 /{}", filename))?;
        self.cmd("M24")?;
        Ok(())
    }

    fn pause(&self) -> Result<(), PrinterError> {
        self.cmd("M25")?;
        Ok(())
    }

    fn resume(&self) -> Result<(), PrinterError> {
        // M24 resumes a paused SD print on Marlin/ESP3D.
        self.cmd("M24")?;
        Ok(())
    }

    fn cancel(&self) -> Result<(), PrinterError> {
        // M524 aborts the active SD print on Marlin-derived firmwares.
        self.cmd("M524")?;
        Ok(())
    }

    fn send_gcode(&self, line: &str) -> Result<String, PrinterError> {
        self.cmd(line)
    }
}

/// Extract `FIRMWARE_NAME:...` or fall back to the leading token.
fn parse_firmware_name(reply: &str) -> Option<String> {
    let reply = reply.trim();
    if let Some(rest) = reply.strip_prefix("FIRMWARE_NAME:") {
        let name = rest.split_whitespace().next().unwrap_or("ESP3D");
        return Some(name.to_string());
    }
    if reply.is_empty() || reply.eq_ignore_ascii_case("ok") {
        None
    } else {
        Some(reply.split_whitespace().next().unwrap_or("ESP3D").to_string())
    }
}

/// Parse `T:<actual> /<target> B:<actual> /<target>` from an `M105`-style reply.
fn parse_temps(reply: &str) -> Temperature {
    let mut t = Temperature::default();
    if let Some((actual, target)) = scan_pair(reply, "T:") {
        t.tool = actual;
        t.tool_target = target;
    }
    if let Some((actual, target)) = scan_pair(reply, "B:") {
        t.bed = actual;
        t.bed_target = target;
    }
    t
}

/// Find `key<actual> /<target>` and parse both as f64.
fn scan_pair(hay: &str, key: &str) -> Option<(f64, f64)> {
    let idx = hay.find(key)? + key.len();
    let rest = &hay[idx..];
    let actual = rest.split_whitespace().next()?.parse::<f64>().ok()?;
    // target follows a '/'
    let after = rest.split('/').nth(1)?;
    let target = after.split_whitespace().next()?.parse::<f64>().ok()?;
    Some((actual, target))
}

/// Parse `SD printing byte X/Y` into a 0..=1 fraction, if present.
fn parse_sd_progress(reply: &str) -> Option<f64> {
    let lower = reply.to_ascii_lowercase();
    let marker = lower.find("byte ")?;
    let rest = &reply[marker + 5..];
    let nums: Vec<f64> = rest
        .split(|c: char| !c.is_ascii_digit() && c != '.')
        .filter_map(|s| s.parse::<f64>().ok())
        .collect();
    if nums.len() >= 2 && nums[1] > 0.0 {
        Some((nums[0] / nums[1]).clamp(0.0, 1.0))
    } else {
        None
    }
}

/// Best-effort filename extraction from an `M27` reply.
fn parse_sd_filename(reply: &str) -> Option<String> {
    let lower = reply.to_ascii_lowercase();
    let start = lower.find("/")?;
    let end = reply[start..]
        .find(".gco")
        .or_else(|| reply[start..].find(".gcode"))?;
    let name = &reply[start..start + end + 5];
    Some(name.trim_start_matches('/').to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockTransport;
    use crate::target::ProtocolKind;

    fn target() -> PrinterTarget {
        PrinterTarget::new("p1", "ESP", ProtocolKind::Esp3d, "http://esp.local", None)
    }

    fn mock() -> MockTransport {
        MockTransport::new()
            .respond("/?cmd=M115", "FIRMWARE_NAME:ESP3D 3.0.0")
            .respond("/?cmd=M105", "ok T:210.0 /210.0 B:60.0 /60.0 @:0 B@:0")
            .respond("/?cmd=M27", "SD printing byte 5120/10240")
            .respond("/?cmd=M23 /part.gcode", "ok")
            .respond("/?cmd=M24", "ok")
            .respond("/?cmd=M25", "ok")
            .respond("/?cmd=M524", "ok")
            .respond("/?cmd=G28", "ok")
            .respond("/upload", "ok")
    }

    #[test]
    fn connect_reports_firmware() {
        let c = Esp3dClient::new(target(), Box::new(mock()));
        let info = c.test_connection().unwrap();
        assert!(info.connected);
        assert_eq!(info.firmware.as_deref(), Some("ESP3D"));
    }

    #[test]
    fn status_parses_temps_and_progress() {
        let c = Esp3dClient::new(target(), Box::new(mock()));
        let s = c.status().unwrap();
        assert_eq!(s.state, PrinterState::Printing);
        assert!((s.temps.tool - 210.0).abs() < 1e-9);
        assert!((s.temps.bed - 60.0).abs() < 1e-9);
        let p = s.progress.unwrap();
        assert!((p.completion - 0.5).abs() < 1e-9);
        // ESP3D's `M27` reply reports byte progress without a filename.
        assert!(p.file.is_none());
    }

    #[test]
    fn upload_records_file_and_start_sends_commands() {
        let t = mock();
        let uploads = t.uploads.clone();
        let commands = t.commands.clone();
        let c = Esp3dClient::new(target(), Box::new(t));
        c.upload_gcode("part.gcode", b"G1 X10").unwrap();
        let uploaded = uploads.lock().unwrap();
        assert_eq!(uploaded.len(), 1);
        assert_eq!(uploaded[0].0, "part.gcode");
        drop(uploaded);

        c.start_print("part.gcode").unwrap();
        let cmds = commands.lock().unwrap();
        assert!(cmds.contains(&"/?cmd=M23 /part.gcode".to_string()));
        assert!(cmds.contains(&"/?cmd=M24".to_string()));
    }

    #[test]
    fn pause_resume_cancel_send_commands() {
        let t = mock();
        let commands = t.commands.clone();
        let c = Esp3dClient::new(target(), Box::new(t));
        c.pause().unwrap();
        c.resume().unwrap();
        c.cancel().unwrap();
        c.send_gcode("G28").unwrap();
        let cmds = commands.lock().unwrap();
        assert!(cmds.contains(&"/?cmd=M25".to_string()));
        assert!(cmds.contains(&"/?cmd=M524".to_string()));
    }

    #[test]
    fn malformed_reply_does_not_panic() {
        let t = MockTransport::new().respond("/?cmd=M105", "garbage with no temps");
        let c = Esp3dClient::new(target(), Box::new(t));
        let s = c.status().unwrap();
        assert!(s.temps.tool.is_nan());
        assert_eq!(s.state, PrinterState::Idle);
    }
}
