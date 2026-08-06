import { useState } from "react";
import { useModelStore } from "../state/store";
import { isTauri } from "../printer/client";
import { invoke } from "@tauri-apps/api/core";
import type { FeatureNode } from "../state/types";

/**
 * Export panel (Phase 13): drives the real `tpt-vertex-manufacturing` exporters
 * through the desktop Tauri commands (`export_step_text`, `export_stl_ascii`,
 * `export_stl_binary`, `export_obj`, `export_gltf`, `export_drawing`,
 * `export_bom`). In a plain browser build the commands are unavailable, so the
 * panel explains that exporting requires the desktop app.
 */

interface ModelSpec {
  rect: [number, number, number, number];
  height: number;
}

function modelSpecFromFeatures(features: FeatureNode[]): ModelSpec {
  const sketch = features.find((f) => f.type === "sketch");
  const extrude = features.find((f) => f.type === "extrude");
  const x0 = Number(sketch?.params.x0 ?? 0);
  const y0 = Number(sketch?.params.y0 ?? 0);
  const x1 = Number(sketch?.params.x1 ?? Number(extrude?.params.x1 ?? 40));
  const y1 = Number(sketch?.params.y1 ?? Number(extrude?.params.y1 ?? 40));
  const height = Number(extrude?.params.height ?? 30);
  return { rect: [x0, y0, x1, y1], height };
}

type Format =
  | "step"
  | "stl-ascii"
  | "stl-binary"
  | "obj"
  | "gltf"
  | "drawing"
  | "bom";

const FORMATS: { id: Format; label: string; ext: string; binary: boolean }[] = [
  { id: "step", label: "STEP", ext: "step", binary: false },
  { id: "stl-ascii", label: "STL (ASCII)", ext: "stl", binary: false },
  { id: "stl-binary", label: "STL (Binary)", ext: "stl", binary: true },
  { id: "obj", label: "OBJ", ext: "obj", binary: false },
  { id: "gltf", label: "glTF", ext: "gltf", binary: false },
  { id: "drawing", label: "Drawing (SVG)", ext: "svg", binary: false },
  { id: "bom", label: "BOM (Markdown)", ext: "md", binary: false },
];

function download(filename: string, data: BlobPart, mime: string) {
  const blob = new Blob([data], { type: mime });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  a.click();
  URL.revokeObjectURL(url);
}

export function ExportPanel({ onClose }: { onClose: () => void }) {
  const features = useModelStore((s) => s.features);
  const [status, setStatus] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const spec = modelSpecFromFeatures(features);

  const onExport = async (fmt: Format) => {
    setError(null);
    setStatus(null);
    if (!isTauri()) {
      setError("Exporting requires the TPT Vertex desktop app.");
      return;
    }
    try {
      if (fmt === "step") {
        const text: string = await invoke("export_step_text", { spec, name: "TPTVertex" });
        download("model.step", text, "application/step");
      } else if (fmt === "stl-ascii") {
        const text: string = await invoke("export_stl_ascii", { spec });
        download("model.stl", text, "model/stl");
      } else if (fmt === "stl-binary") {
        const bytes: number[] = await invoke("export_stl_binary", { spec });
        download("model.stl", new Uint8Array(bytes), "model/stl");
      } else if (fmt === "obj") {
        const text: string = await invoke("export_obj", { spec });
        download("model.obj", text, "model/obj");
      } else if (fmt === "gltf") {
        const out = (await invoke("export_gltf", { spec })) as {
          json: string;
          bin: number[];
        };
        download("model.gltf", out.json, "model/gltf+json");
        download("model.bin", new Uint8Array(out.bin), "application/octet-stream");
      } else if (fmt === "drawing") {
        const text: string = await invoke("export_drawing", { spec });
        download("model.svg", text, "image/svg+xml");
      } else if (fmt === "bom") {
        const text: string = await invoke("export_bom", { spec });
        download("bom.md", text, "text/markdown");
      }
      setStatus(`Exported ${fmt}.`);
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <div className="vc-backdrop" role="dialog" aria-label="Export">
      <div className="vc-card export-card">
        <header className="vc-header">
          <h3>Export</h3>
          <div className="spacer" />
          <button onClick={onClose} aria-label="Close">
            Close
          </button>
        </header>
        <p className="muted">
          Exports run offline through the embedded geometry kernel (STEP, STL, OBJ, glTF,
          drawing, BOM).
        </p>
        <div className="export-formats">
          {FORMATS.map((f) => (
            <button key={f.id} className="primary" onClick={() => onExport(f.id)}>
              {f.label}
            </button>
          ))}
        </div>
        {status && <p className="mono">{status}</p>}
        {error && <p className="error">{error}</p>}
      </div>
    </div>
  );
}
