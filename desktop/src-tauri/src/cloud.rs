//! Desktop Tauri command for the cloud project hand-off stub.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! Thin wrapper over `tpt-vertex-printer-link`'s `fetch_cloud_project`. This is a
//! best-effort stub: it performs the network fetch, but the hosted platform it
//! targets is not deployed yet, so the end-to-end hand-off cannot be verified.

use tpt_vertex_printer_link::cloud::{fetch_cloud_project, CloudProject, CloudProjectRef};

/// Tauri command: open a cloud-hosted project by id and return its manifest.
///
/// Returns the parsed [`CloudProject`] (id, name, manifest JSON) for the
/// frontend to load into the editor.
#[tauri::command]
pub fn open_cloud_project(
    endpoint: String,
    project_id: String,
    api_key: Option<String>,
) -> Result<CloudProject, String> {
    let r = CloudProjectRef {
        endpoint,
        project_id,
        api_key,
    };
    fetch_cloud_project(&r).map_err(|e| e.to_string())
}
