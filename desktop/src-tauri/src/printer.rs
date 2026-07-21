//! Desktop printer management + control commands (Phase 13).
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! Bridges the web frontend to `tpt-vertex-printer-link`. Saved printer
//! connection configs (`PrinterTarget`s) are persisted in `printers.json` via
//! `tauri-plugin-store`; live control (test/upload/start/status) builds a
//! client on demand through [`make_client`].

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_plugin_store::{Store, StoreExt};
use tpt_vertex_printer_link::{
    make_client, ConnectionInfo, PrinterTarget, StatusSnapshot,
};

/// Store file backing the saved-printer list.
const STORE_FILE: &str = "printers.json";
/// Key under which the printer list is kept in the store.
const PRINTERS_KEY: &str = "printers";

/// A saved printer plus its last-known liveness, for the management panel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedPrinter {
    pub target: PrinterTarget,
    pub connected: bool,
}

fn store(app: &AppHandle) -> Result<Arc<Store>, String> {
    app.store(STORE_FILE).map_err(|e| e.to_string())
}

fn read_all(store: &Store) -> Vec<PrinterTarget> {
    store
        .get(PRINTERS_KEY)
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default()
}

fn write_all(store: &Store, printers: &[PrinterTarget]) -> Result<(), String> {
    let value = serde_json::to_value(printers).map_err(|e| e.to_string())?;
    store.set(PRINTERS_KEY, value);
    store.save().map_err(|e| e.to_string())
}

/// List saved printers.
#[tauri::command]
pub fn list_printers(app: AppHandle) -> Result<Vec<PrinterTarget>, String> {
    let store = store(&app)?;
    Ok(read_all(&store))
}

/// Upsert a printer by id and return the updated list.
#[tauri::command]
pub fn save_printer(app: AppHandle, target: PrinterTarget) -> Result<Vec<PrinterTarget>, String> {
    let store = store(&app)?;
    let mut printers = read_all(&store);
    if let Some(pos) = printers.iter().position(|p| p.id == target.id) {
        printers[pos] = target;
    } else {
        printers.push(target);
    }
    write_all(&store, &printers)?;
    Ok(printers)
}

/// Delete a printer by id and return the updated list.
#[tauri::command]
pub fn delete_printer(app: AppHandle, id: String) -> Result<Vec<PrinterTarget>, String> {
    let store = store(&app)?;
    let printers: Vec<PrinterTarget> = read_all(&store)
        .into_iter()
        .filter(|p| p.id != id)
        .collect();
    write_all(&store, &printers)?;
    Ok(printers)
}

/// Probe a printer target and return its connection info.
#[tauri::command]
pub fn test_printer(target: PrinterTarget) -> Result<ConnectionInfo, String> {
    let client = make_client(&target).map_err(|e| e.to_string())?;
    client.test_connection().map_err(|e| e.to_string())
}

/// Upload G-code to the printer and start the print; returns live status.
///
/// `filename` defaults to `tpt-vertex.gcode` when empty.
#[tauri::command]
pub fn send_to_printer(
    target: PrinterTarget,
    filename: String,
    gcode: String,
) -> Result<StatusSnapshot, String> {
    let client = make_client(&target).map_err(|e| e.to_string())?;
    let name = if filename.trim().is_empty() {
        "tpt-vertex.gcode".to_string()
    } else {
        filename
    };
    client.upload_gcode(&name, gcode.as_bytes()).map_err(|e| e.to_string())?;
    client.start_print(&name).map_err(|e| e.to_string())?;
    client.status().map_err(|e| e.to_string())
}

/// Fetch the current status snapshot for a printer target.
#[tauri::command]
pub fn printer_status(target: PrinterTarget) -> Result<StatusSnapshot, String> {
    let client = make_client(&target).map_err(|e| e.to_string())?;
    client.status().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_vertex_printer_link::ProtocolKind;

    fn sample() -> PrinterTarget {
        PrinterTarget::new("id1", "Test", ProtocolKind::OctoPrint, "http://localhost", Some("k".into()))
    }

    #[test]
    fn upsert_and_delete_round_trips() {
        // Exercise the pure persistence logic with an in-memory store-like
        // vector, mirroring the command bodies without Tauri.
        let mut printers: Vec<PrinterTarget> = Vec::new();
        let t = sample();
        if printers.iter().position(|p| p.id == t.id).is_none() {
            printers.push(t.clone());
        }
        assert_eq!(printers.len(), 1);
        printers.retain(|p| p.id != t.id);
        assert!(printers.is_empty());
    }
}
