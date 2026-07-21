//! Integration tests against a real (tiny local) mock HTTP server.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! These exercise the actual `reqwest` transport end-to-end: the clients issue
//! real HTTP requests to an in-process TCP server that mimics the ESP3D or
//! OctoPrint control surface. No external service (and no `mockito`) required.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::time::Duration;

use tpt_vertex_printer_link::{make_client, PrinterTarget, ProtocolKind, PrinterError, PrinterState};

/// A request target (e.g. `/api/version` or `/?cmd=M115`) maps to a
/// `(status_code, body)` response from the mock server.
type Route = Arc<dyn Fn(&str) -> (u16, String) + Send + Sync>;

fn spawn_server(route: Route) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            if let Ok(mut s) = stream {
                serve_one(&mut s, &route);
            }
        }
    });
    format!("http://{addr}")
}

fn header_end(buf: &[u8]) -> Option<usize> {
    // Accept CRLF or bare LF header termination.
    if let Some(p) = find_subsequence(buf, b"\r\n\r\n") {
        return Some(p + 4);
    }
    find_subsequence(buf, b"\n\n").map(|p| p + 2)
}

fn find_subsequence(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Minimal percent-decoder (handles `%XX` escapes, e.g. `%20` → space) so the
/// mock server matches the paths clients actually send after URL-encoding.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(h), Some(l)) = (hi, lo) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn serve_one(stream: &mut TcpStream, route: &Route) {
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
    let mut buf = vec![0u8; 4096];
    let mut received = Vec::new();
    loop {
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => received.extend_from_slice(&buf[..n]),
            Err(_) => break,
        }
        if let Some(pos) = header_end(&received) {
            let header_text = String::from_utf8_lossy(&received[..pos]);
            let content_length = header_text
                .lines()
                .find_map(|l| {
                    l.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                })
                .unwrap_or(0);
            if received.len() >= pos + content_length {
                break;
            }
        }
        if received.len() > 1 << 20 {
            break;
        }
    }

    let target = String::from_utf8_lossy(&received)
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .unwrap_or("/")
        .to_string();
    let target = percent_decode(&target);

    let (status, body) = route(&target);
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
}

#[test]
fn esp3d_over_real_http_server() {
    let route: Route = Arc::new(|t| match t {
        "/?cmd=M115" => (200, "FIRMWARE_NAME:ESP3D 3.0.0".into()),
        "/?cmd=M105" => (200, "ok T:210.0 /210.0 B:60.0 /60.0".into()),
        "/?cmd=M27" => (200, "SD printing byte 5120/10240".into()),
        "/?cmd=M23 /part.gcode" => (200, "ok".into()),
        "/?cmd=M24" => (200, "ok".into()),
        "/?cmd=M25" => (200, "ok".into()),
        "/?cmd=M524" => (200, "ok".into()),
        "/upload" => (200, "ok".into()),
        _ => (404, String::new()),
    });
    let base = spawn_server(route);
    let target = PrinterTarget::new("esp", "ESP", ProtocolKind::Esp3d, base, None);
    let client = make_client(&target).unwrap();

    let info = client.test_connection().unwrap();
    assert!(info.connected);
    assert_eq!(info.firmware.as_deref(), Some("ESP3D"));

    let status = client.status().unwrap();
    assert_eq!(status.state, PrinterState::Printing);
    assert!((status.temps.tool - 210.0).abs() < 1e-9);
    assert!((status.progress.as_ref().unwrap().completion - 0.5).abs() < 1e-9);

    client.upload_gcode("part.gcode", b"G1 X10").unwrap();
    client.start_print("part.gcode").unwrap();
    client.pause().unwrap();
    client.resume().unwrap();
    client.cancel().unwrap();
}

#[test]
fn octoprint_over_real_http_server() {
    let printer = r#"{"state":{"text":"Printing","flags":{"operational":true,"printing":true,"paused":false}},"temperature":{"tool0":{"actual":205.0,"target":210.0},"bed":{"actual":59.0,"target":60.0}}}"#;
    let job = r#"{"job":{"file":{"name":"part.gcode"}},"progress":{"completion":42.0,"printTimeLeft":600}}"#;
    let route: Route = Arc::new(move |t| match t {
        "/api/version" => (200, r#"{"server":"OctoPrint","version":"1.9.0","api":"0.1"}"#.into()),
        "/api/printer" => (200, printer.to_string()),
        "/api/job" => (200, job.to_string()),
        "/api/files/local" => (201, "{}".into()),
        "/api/printer/command" => (204, String::new()),
        _ => (404, String::new()),
    });
    let base = spawn_server(route);
    let target = PrinterTarget::new("octo", "Octo", ProtocolKind::OctoPrint, base, Some("KEY".into()));
    let client = make_client(&target).unwrap();

    let info = client.test_connection().unwrap();
    assert!(info.firmware.as_deref() == Some("OctoPrint 1.9.0"));

    let status = client.status().unwrap();
    assert_eq!(status.state, PrinterState::Printing);
    assert!((status.temps.bed - 59.0).abs() < 1e-9);
    assert_eq!(status.progress.unwrap().file.as_deref(), Some("part.gcode"));

    client.upload_gcode("part.gcode", b"G1 X10").unwrap();
    client.start_print("part.gcode").unwrap();
    client.pause().unwrap();
    client.cancel().unwrap();
}

#[test]
fn moonraker_compat_uses_octoprint_client() {
    let route: Route = Arc::new(|t| match t {
        "/api/version" => (200, r#"{"version":"0.8.0"}"#.into()),
        "/api/printer" => (200, r#"{"state":{"flags":{"operational":true}},"temperature":{}}"#.into()),
        "/api/job" => (200, r#"{"progress":{}}"#.into()),
        "/api/files/local" => (201, "{}".into()),
        "/api/printer/command" => (204, String::new()),
        _ => (404, String::new()),
    });
    let base = spawn_server(route);
    let target = PrinterTarget::new("moon", "Moon", ProtocolKind::MoonrakerCompat, base, Some("K".into()));
    let client = make_client(&target).unwrap();
    assert_eq!(client.connection_info().protocol, ProtocolKind::MoonrakerCompat);
    assert!(client.test_connection().unwrap().firmware.unwrap().contains("Moonraker"));
}

#[test]
fn connection_refused_is_transport_error() {
    // Bind then immediately drop to get a free-but-closed port.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    let base = format!("http://{addr}");
    let target = PrinterTarget::new("x", "X", ProtocolKind::Esp3d, base, None);
    let client = make_client(&target).unwrap();
    assert!(matches!(client.test_connection(), Err(PrinterError::Transport(_))));
}

#[test]
fn malformed_reply_is_parse_error() {
    let route: Route = Arc::new(|t| match t {
        "/api/version" => (200, r#"{"version":"1.9.0"}"#.into()),
        "/api/printer" => (200, "not json".into()),
        "/api/job" => (200, r#"{"progress":{}}"#.into()),
        "/api/files/local" => (201, "{}".into()),
        "/api/printer/command" => (204, String::new()),
        _ => (404, String::new()),
    });
    let base = spawn_server(route);
    let target = PrinterTarget::new("m", "M", ProtocolKind::OctoPrint, base, None);
    let client = make_client(&target).unwrap();
    assert!(matches!(client.status(), Err(PrinterError::Parse(_))));
}
