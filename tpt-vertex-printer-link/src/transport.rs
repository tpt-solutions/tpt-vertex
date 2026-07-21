//! Pluggable HTTP transport so the protocol clients stay testable.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! The protocol clients ([`Esp3dClient`], [`OctoPrintClient`]) never call
//! `reqwest` directly; they go through [`HttpTransport`]. The real
//! implementation ([`ReqwestTransport`]) wraps `reqwest::blocking`, while tests
//! can supply an in-memory [`MockTransport`](crate) or point the real transport
//! at a tiny local mock HTTP server.

use crate::target::ProtocolKind;
use crate::PrinterError;

/// Abstract HTTP surface the clients depend on.
///
/// Paths are relative to the target's base URL (e.g. `/api/printer`), except
/// when a caller passes an absolute URL.
pub trait HttpTransport {
    /// `GET` `path` and return the response body as text.
    fn get(&self, path: &str) -> Result<String, PrinterError>;

    /// `POST` `path` with a raw body and `Content-Type`.
    fn post(&self, path: &str, body: &[u8], content_type: &str) -> Result<String, PrinterError>;

    /// `POST` `path` with a UTF-8 text body and `Content-Type`.
    fn post_text(&self, path: &str, body: &str, content_type: &str) -> Result<String, PrinterError> {
        self.post(path, body.as_bytes(), content_type)
    }

    /// `POST` `path` as `multipart/form-data` with a single file part named
    /// `file` plus any extra scalar form fields (e.g. OctoPrint's `print`).
    fn upload(
        &self,
        path: &str,
        filename: &str,
        data: &[u8],
        extra_fields: &[(&str, &str)],
    ) -> Result<String, PrinterError>;
}

/// Real transport backed by `reqwest::blocking`.
pub struct ReqwestTransport {
    client: reqwest::blocking::Client,
    base: String,
    protocol: ProtocolKind,
    api_key: Option<String>,
}

impl ReqwestTransport {
    /// Create a transport for `base_url`. The optional `api_key` is sent as an
    /// `X-Api-Key` header for protocols that require it (OctoPrint/Moonraker).
    pub fn new(
        base_url: &str,
        protocol: ProtocolKind,
        api_key: Option<&str>,
    ) -> Result<Self, PrinterError> {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .map_err(|e| PrinterError::Transport(format!("http client init: {e}")))?;
        Ok(ReqwestTransport {
            client,
            base: base_url.trim_end_matches('/').to_string(),
            protocol,
            api_key: api_key.map(str::to_string),
        })
    }

    fn url(&self, path: &str) -> String {
        if path.starts_with("http://") || path.starts_with("https://") {
            return path.to_string();
        }
        format!("{}{}", self.base, path)
    }

    fn authenticated(&self, mut req: reqwest::blocking::RequestBuilder) -> reqwest::blocking::RequestBuilder {
        // OctoPrint/Moonraker use X-Api-Key; ESP3D has no auth header.
        if matches!(self.protocol, ProtocolKind::OctoPrint | ProtocolKind::MoonrakerCompat) {
            if let Some(k) = &self.api_key {
                req = req.header("X-Api-Key", k);
            }
        }
        req
    }
}

impl HttpTransport for ReqwestTransport {
    fn get(&self, path: &str) -> Result<String, PrinterError> {
        let resp = self
            .authenticated(self.client.get(self.url(path)))
            .send()
            .map_err(map_reqwest)?;
        read_text(resp)
    }

    fn post(&self, path: &str, body: &[u8], content_type: &str) -> Result<String, PrinterError> {
        let resp = self
            .authenticated(self.client.post(self.url(path)).body(body.to_vec()).header("Content-Type", content_type))
            .send()
            .map_err(map_reqwest)?;
        read_text(resp)
    }

    fn upload(
        &self,
        path: &str,
        filename: &str,
        data: &[u8],
        extra_fields: &[(&str, &str)],
    ) -> Result<String, PrinterError> {
        use reqwest::blocking::multipart::{Form, Part};
        let mut form = Form::new().part("file", Part::bytes(data.to_vec()).file_name(filename.to_string()));
        for (k, v) in extra_fields {
            form = form.text(k.to_string(), v.to_string());
        }
        let resp = self
            .authenticated(self.client.post(self.url(path)).multipart(form))
            .send()
            .map_err(map_reqwest)?;
        read_text(resp)
    }
}

fn map_reqwest(e: reqwest::Error) -> PrinterError {
    if e.is_timeout() {
        PrinterError::Timeout(e.to_string())
    } else if e.is_status() {
        let status = e.status().map(|s| s.as_u16()).unwrap_or(0);
        match status {
            401 | 403 => PrinterError::Auth(format!("HTTP {status}")),
            404 => PrinterError::NotFound(e.to_string()),
            _ => PrinterError::Http(format!("HTTP {status}")),
        }
    } else if e.is_connect() {
        PrinterError::Transport(format!("connection refused: {e}"))
    } else {
        PrinterError::Http(e.to_string())
    }
}

fn read_text(resp: reqwest::blocking::Response) -> Result<String, PrinterError> {
    let status = resp.status();
    let text = resp.text().map_err(|e| PrinterError::Http(format!("read body: {e}")))?;
    if !status.is_success() {
        return match status.as_u16() {
            401 | 403 => Err(PrinterError::Auth(format!("HTTP {status}: {text}"))),
            404 => Err(PrinterError::NotFound(format!("HTTP {status}: {text}"))),
            _ => Err(PrinterError::Http(format!("HTTP {status}: {text}"))),
        };
    }
    Ok(text)
}
