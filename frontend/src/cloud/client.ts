/**
 * Tauri IPC wrapper for the cloud project hand-off stub (best-effort).
 *
 * Mirrors `printer/client.ts`: degrades gracefully when running in a plain
 * browser with no Tauri runtime. Field names are snake_case to match the Rust
 * `serde` serialization used by `tauri::command`.
 */
import { invoke } from "@tauri-apps/api/core";

/** True when running inside the Tauri desktop shell. */
export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/** A cloud project returned by `open_cloud_project`. */
export interface CloudProject {
  id: string;
  name: string;
  /** Raw project manifest JSON (feature tree, parameters, etc.). */
  manifest: unknown;
}

const NOT_TAURI = "Cloud project hand-off is only available in the desktop app.";

/**
 * Open a cloud-hosted project by id from the given API endpoint.
 *
 * Best-effort: the hosted platform is not deployed yet, so this cannot be
 * verified end-to-end.
 */
export async function openCloudProject(
  endpoint: string,
  projectId: string,
  apiKey?: string | null,
): Promise<CloudProject> {
  if (!isTauri()) throw new Error(NOT_TAURI);
  return invoke<CloudProject>("open_cloud_project", {
    endpoint,
    projectId,
    apiKey: apiKey ?? null,
  });
}
