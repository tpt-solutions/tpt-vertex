//! Desktop printer management + control commands (Phase 13).
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! Bridges the web frontend to `tpt-vertex-printer-link`. Saved printer
//! connection configs (`PrinterTarget`s) are persisted in `printers.json` via
//! `tauri-plugin-store`; the API key is **never** written to that JSON file —
//! it is stored in the OS keychain (keyed by `target.id`) and rehydrated on
//! demand by the live-control commands. Legacy plaintext keys found in
//! `printers.json` are migrated into the keychain on read.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_plugin_store::{Store, StoreExt};
use tpt_vertex_printer_link::{
    make_client, ConnectionInfo, DiscoveryResult, Keychain, PrinterTarget, StatusSnapshot,
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

fn store(app: &AppHandle) -> Result<Arc<Store<tauri::Wry>>, String> {
    app.store(STORE_FILE).map_err(|e| e.to_string())
}

fn read_all(store: &Store<tauri::Wry>) -> Vec<PrinterTarget> {
    let mut printers: Vec<PrinterTarget> = store
        .get(PRINTERS_KEY)
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();
    // Migrate any legacy plaintext `api_key` found in the JSON store into the
    // OS keychain, stripping it from the persisted copy (never store secrets
    // in plaintext JSON).
    let mut migrated = false;
    for p in &mut printers {
        if let Some(key) = p.api_key.take() {
            if !key.is_empty() {
                let _ = Keychain::new().set_key(&p.id, &key);
                migrated = true;
            }
        }
    }
    if migrated {
        let _ = write_all(store, &printers);
    }
    printers
}

/// Rehydrate a target's `api_key` from the OS keychain when it is absent, so
/// live-control commands (which persist targets without the secret) can still
/// authenticate. Targets arriving from the frontend without a key look up the
/// stored credential by `id`.
fn rehydrate(mut target: PrinterTarget) -> PrinterTarget {
    let missing = target.api_key.as_deref().map(|k| k.is_empty()).unwrap_or(true);
    if missing {
        if let Ok(Some(key)) = Keychain::new().get_key(&target.id) {
            target.api_key = Some(key);
        }
    }
    target
}

fn write_all(store: &Store<tauri::Wry>, printers: &[PrinterTarget]) -> Result<(), String> {
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
///
/// The API key (if any) is stored in the OS keychain keyed by `target.id` and
/// is **never** persisted to `printers.json`; the saved target always has its
/// `api_key` stripped.
#[tauri::command]
pub fn save_printer(app: AppHandle, target: PrinterTarget) -> Result<Vec<PrinterTarget>, String> {
    let store = store(&app)?;
    let mut target = target;
    if let Some(key) = target.api_key.take() {
        if !key.is_empty() {
            Keychain::new()
                .set_key(&target.id, &key)
                .map_err(|e| e.to_string())?;
        }
    }
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
    // Drop the API key from the OS keychain too.
    let _ = Keychain::new().delete_key(&id);
    Ok(printers)
}

/// Probe a printer target and return its connection info.
#[tauri::command]
pub fn test_printer(target: PrinterTarget) -> Result<ConnectionInfo, String> {
    let client = make_client(&rehydrate(target)).map_err(|e| e.to_string())?;
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
    let client = make_client(&rehydrate(target)).map_err(|e| e.to_string())?;
    let name = if filename.trim().is_empty() {
        "tpt-vertex.gcode".to_string()
    } else {
        filename
    };
    client
        .upload_gcode(&name, gcode.as_bytes())
        .map_err(|e| e.to_string())?;
    client.start_print(&name).map_err(|e| e.to_string())?;
    client.status().map_err(|e| e.to_string())
}

/// Fetch the current status snapshot for a printer target.
#[tauri::command]
pub fn printer_status(target: PrinterTarget) -> Result<StatusSnapshot, String> {
    let client = make_client(&rehydrate(target)).map_err(|e| e.to_string())?;
    client.status().map_err(|e| e.to_string())
}

/// Scan the local network for printers via mDNS.
#[tauri::command]
pub fn discover_printers() -> Result<DiscoveryResult, String> {
    Ok(tpt_vertex_printer_link::discovery::scan())
}

/// Store a printer's API key in the OS keychain.
#[tauri::command]
pub fn set_printer_key(printer_id: String, api_key: String) -> Result<(), String> {
    let kc = Keychain::new();
    kc.set_key(&printer_id, &api_key)
}

/// Retrieve a printer's API key from the OS keychain.
#[tauri::command]
pub fn get_printer_key(printer_id: String) -> Result<Option<String>, String> {
    let kc = Keychain::new();
    kc.get_key(&printer_id)
}

/// Delete a printer's API key from the OS keychain.
#[tauri::command]
pub fn delete_printer_key(printer_id: String) -> Result<(), String> {
    let kc = Keychain::new();
    kc.delete_key(&printer_id)
}

/// Stream G-code to a printer layer-by-layer.
///
/// Splits the G-code on `"; LAYER"` boundaries and sends each layer
/// individually, polling the printer status between layers.
#[tauri::command]
pub fn stream_gcode(target: PrinterTarget, gcode: String) -> Result<usize, String> {
    use tpt_vertex_printer_link::stream::{GCodeStreamer, StreamConfig};
    let client = make_client(&rehydrate(target)).map_err(|e| e.to_string())?;
    let streamer = GCodeStreamer::new(client.as_ref(), StreamConfig::default());
    streamer.stream_full(&gcode).map_err(|e| e.to_string())
}

/// Push a single already-sliced layer's G-code to the printer.
///
/// Meant to be called once per layer as the frontend slicer produces it, so
/// the printer starts receiving (and, on firmware that executes streamed
/// G-code directly rather than requiring a stored file, printing) before the
/// rest of the model has finished slicing — instead of waiting for the whole
/// file to be uploaded and then starting the print.
#[tauri::command]
pub fn stream_gcode_layer(target: PrinterTarget, layer_gcode: String) -> Result<(), String> {
    use tpt_vertex_printer_link::stream::{GCodeStreamer, StreamConfig};
    let client = make_client(&rehydrate(target)).map_err(|e| e.to_string())?;
    let streamer = GCodeStreamer::new(client.as_ref(), StreamConfig::default());
    streamer.send_layer(&layer_gcode).map_err(|e| e.to_string())
}

/// Cancel the active print on a printer.
#[tauri::command]
pub fn cancel_print(target: PrinterTarget) -> Result<(), String> {
    let client = make_client(&rehydrate(target)).map_err(|e| e.to_string())?;
    client.cancel().map_err(|e| e.to_string())
}

/// Pause the active print on a printer.
#[tauri::command]
pub fn pause_print(target: PrinterTarget) -> Result<(), String> {
    let client = make_client(&rehydrate(target)).map_err(|e| e.to_string())?;
    client.pause().map_err(|e| e.to_string())
}

/// Resume a paused print.
#[tauri::command]
pub fn resume_print(target: PrinterTarget) -> Result<(), String> {
    let client = make_client(&rehydrate(target)).map_err(|e| e.to_string())?;
    client.resume().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_vertex_printer_link::ProtocolKind;

    fn sample() -> PrinterTarget {
        PrinterTarget::new(
            "id1",
            "Test",
            ProtocolKind::OctoPrint,
            "http://localhost",
            Some("k".into()),
        )
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
