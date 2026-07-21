/**
 * Tauri IPC wrapper for printer connectivity (Phase 13).
 *
 * This is the first place the web frontend talks to `@tauri-apps/api`: the
 * desktop shell exposes `tpt-vertex-printer-link` through the commands defined
 * in `desktop/src-tauri/src/printer.rs`. All calls degrade gracefully when the
 * app is running in a plain browser (no Tauri runtime), so the panel can show a
 * helpful hint instead of crashing.
 *
 * Field names are snake_case to match the Rust `serde` serialization used by
 * `tauri::command`.
 */
import { invoke } from "@tauri-apps/api/core";

/** True when running inside the Tauri desktop shell. */
export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export type ProtocolKind = "esp3d" | "octoprint" | "moonraker-compat";

export interface PrinterTarget {
  id: string;
  name: string;
  kind: ProtocolKind;
  base_url: string;
  api_key: string | null;
}

export interface ConnectionInfo {
  protocol: ProtocolKind;
  host: string;
  connected: boolean;
  firmware: string | null;
}

export interface Temperature {
  tool: number;
  tool_target: number;
  bed: number;
  bed_target: number;
}

export interface JobProgress {
  completion: number;
  file: string | null;
  time_left_s: number | null;
}

export type PrinterState =
  | "Disconnected"
  | "Idle"
  | "Printing"
  | "Paused"
  | "Completed"
  | "Error";

export interface StatusSnapshot {
  state: PrinterState;
  temps: Temperature;
  progress: JobProgress | null;
  firmware: string | null;
}

const NOT_TAURI = "Printer control is only available in the desktop app.";

export async function listPrinters(): Promise<PrinterTarget[]> {
  if (!isTauri()) return [];
  return invoke<PrinterTarget[]>("list_printers");
}

export async function savePrinter(target: PrinterTarget): Promise<PrinterTarget[]> {
  if (!isTauri()) throw new Error(NOT_TAURI);
  return invoke<PrinterTarget[]>("save_printer", { target });
}

export async function deletePrinter(id: string): Promise<PrinterTarget[]> {
  if (!isTauri()) throw new Error(NOT_TAURI);
  return invoke<PrinterTarget[]>("delete_printer", { id });
}

export async function testPrinter(target: PrinterTarget): Promise<ConnectionInfo> {
  if (!isTauri()) throw new Error(NOT_TAURI);
  return invoke<ConnectionInfo>("test_printer", { target });
}

export async function printerStatus(target: PrinterTarget): Promise<StatusSnapshot> {
  if (!isTauri()) throw new Error(NOT_TAURI);
  return invoke<StatusSnapshot>("printer_status", { target });
}

export async function sendToPrinter(
  target: PrinterTarget,
  filename: string,
  gcode: string,
): Promise<StatusSnapshot> {
  if (!isTauri()) throw new Error(NOT_TAURI);
  return invoke<StatusSnapshot>("send_to_printer", { target, filename, gcode });
}
