//! Cloud project hand-off (best-effort client stub).
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! Client-side code to open a cloud-hosted project from the desktop app. This is
//! a **stub**: it performs the HTTP fetch + JSON parse against a configurable
//! endpoint, but the hosted platform / sync deployment it targets does not exist
//! yet, so it cannot be verified end-to-end (that remains a blocked task). The
//! request/response shape is intentionally minimal and stable so the desktop
//! command and frontend wrapper can be wired now.

use serde::{Deserialize, Serialize};

use crate::target::ProtocolKind;
use crate::transport::{HttpTransport, ReqwestTransport};
use crate::PrinterError;

/// Identifies a cloud project to fetch.
#[derive(Debug, Clone)]
pub struct CloudProjectRef {
    /// Base URL of the cloud API, e.g. `https://api.tpt-vertex.dev`.
    pub endpoint: String,
    /// Project id (UUID or slug).
    pub project_id: String,
    /// Optional API key for the cloud API (sent as `X-Api-Key`).
    pub api_key: Option<String>,
}

/// A cloud project returned by the API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudProject {
    /// Stable project id.
    pub id: String,
    /// Human-readable project name.
    pub name: String,
    /// Raw project manifest/document JSON (feature tree, parameters, etc.).
    pub manifest: serde_json::Value,
}

/// Fetch a cloud project's manifest by id using the supplied [`HttpTransport`]
/// (injectable for testing). Builds the URL `{endpoint}/api/projects/{project_id}`.
pub fn fetch_cloud_project_with(
    transport: &dyn HttpTransport,
    r: &CloudProjectRef,
) -> Result<CloudProject, PrinterError> {
    let path = format!("/api/projects/{}", r.project_id);
    let body = transport.get(&path)?;
    serde_json::from_str::<CloudProject>(&body)
        .map_err(|e| PrinterError::Http(format!("parse cloud project: {e}")))
}

/// Fetch a cloud project over the network using the real [`ReqwestTransport`].
///
/// Best-effort: the `api_key` is forwarded as an `X-Api-Key` header (reusing the
/// OctoPrint/Moonraker-compatible header behaviour of [`ReqwestTransport`]).
/// Unverified without a deployed server — see the crate's open "cloud hand-off"
/// task.
pub fn fetch_cloud_project(r: &CloudProjectRef) -> Result<CloudProject, PrinterError> {
    let transport = ReqwestTransport::new(
        &r.endpoint,
        ProtocolKind::MoonrakerCompat,
        r.api_key.as_deref(),
    )?;
    fetch_cloud_project_with(&transport, r)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockTransport;

    #[test]
    fn parses_cloud_project_from_transport() {
        let json = r#"{"id":"abc","name":"Gear","manifest":{"features":[]}}"#;
        let mock = MockTransport::new().respond("/api/projects/abc", json);
        let r = CloudProjectRef {
            endpoint: "https://api.example".into(),
            project_id: "abc".into(),
            api_key: None,
        };
        let project = fetch_cloud_project_with(&mock, &r).expect("fetch");
        assert_eq!(project.id, "abc");
        assert_eq!(project.name, "Gear");
        assert!(project.manifest.get("features").is_some());
    }
}
