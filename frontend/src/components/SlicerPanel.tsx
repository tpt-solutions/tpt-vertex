import { useEffect, useMemo, useState } from "react";
import { useModelStore } from "../state/store";
import {
  sliceModel,
  DEFAULT_SLICE_SETTINGS,
  type SliceSettings,
  type SliceResult,
} from "../geometry/slicer";
import {
  listPrinters,
  sendToPrinter,
  type PrinterTarget,
} from "../printer/client";

/**
 * Minimal slicer settings + layer-preview panel (Phase 10). Reads the current
 * parametric model, runs the browser-side slicer preview, and shows a top-down
 * layer view with print estimates and a G-code download.
 */
export function SlicerPanel({ onClose }: { onClose: () => void }) {
  const features = useModelStore((s) => s.features);
  const [settings, setSettings] = useState<SliceSettings>(DEFAULT_SLICE_SETTINGS);
  const [result, setResult] = useState<SliceResult | null>(null);
  const [layerIndex, setLayerIndex] = useState(0);
  const [printers, setPrinters] = useState<PrinterTarget[]>([]);
  const [selectedPrinter, setSelectedPrinter] = useState("");
  const [sendStatus, setSendStatus] = useState<string | null>(null);
  const [sendError, setSendError] = useState<string | null>(null);

  const update = <K extends keyof SliceSettings>(key: K, value: SliceSettings[K]) =>
    setSettings((s) => ({ ...s, [key]: value }));

  const doSlice = () => {
    const r = sliceModel(features, settings);
    setResult(r);
    setLayerIndex(0);
  };

  const refreshPrinters = () => {
    listPrinters()
      .then((list) => {
        setPrinters(list);
        if (list.length && !selectedPrinter) setSelectedPrinter(list[0].id);
      })
      .catch(() => setPrinters([]));
  };

  const onSend = async () => {
    if (!result || !selectedPrinter) return;
    const target = printers.find((p) => p.id === selectedPrinter);
    if (!target) return;
    setSendError(null);
    setSendStatus(null);
    try {
      const s = await sendToPrinter(target, "tpt-vertex.gcode", result.gcode);
      const pct = s.progress ? ` · ${Math.round(s.progress.completion * 100)}%` : "";
      setSendStatus(`State: ${s.state}${pct}`);
    } catch (e) {
      setSendError(String(e));
    }
  };

  useEffect(refreshPrinters, []); // eslint-disable-line react-hooks/exhaustive-deps

  const layer = result?.layers[Math.min(layerIndex, result.layers.length - 1)];

  const viewBox = useMemo(() => {
    if (!result) return "0 0 100 100";
    const [w, d] = result.size;
    const pad = Math.max(w, d) * 0.1 + 1;
    return `${-w / 2 - pad} ${-d / 2 - pad} ${w + pad * 2} ${d + pad * 2}`;
  }, [result]);

  const downloadGcode = () => {
    if (!result) return;
    const blob = new Blob([result.gcode], { type: "text/plain" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = "tpt-vertex.gcode";
    a.click();
    URL.revokeObjectURL(url);
  };

  return (
    <div className="vc-backdrop" role="dialog" aria-label="Slicer">
      <div className="vc-card slicer-card">
        <header className="vc-header">
          <h3>Slicer</h3>
          <div className="spacer" />
          <button onClick={onClose} aria-label="Close">
            Close
          </button>
        </header>

        <div className="slicer-body">
          <section className="panel slicer-settings" aria-label="Slice settings">
            <h2 className="panel-title">Settings</h2>
            <label>
              Layer height (mm)
              <input
                type="number"
                step="0.05"
                min="0.05"
                value={settings.layerHeight}
                onChange={(e) => update("layerHeight", Number(e.target.value))}
                aria-label="Layer height"
              />
            </label>
            <label>
              First layer height (mm)
              <input
                type="number"
                step="0.05"
                min="0.05"
                value={settings.firstLayerHeight}
                onChange={(e) => update("firstLayerHeight", Number(e.target.value))}
                aria-label="First layer height"
              />
            </label>
            <label>
              Walls
              <input
                type="number"
                step="1"
                min="1"
                value={settings.wallCount}
                onChange={(e) => update("wallCount", Math.max(1, Math.round(Number(e.target.value))))}
                aria-label="Wall count"
              />
            </label>
            <label>
              Infill density (%)
              <input
                type="number"
                step="5"
                min="0"
                max="100"
                value={Math.round(settings.infillDensity * 100)}
                onChange={(e) => update("infillDensity", Number(e.target.value) / 100)}
                aria-label="Infill density"
              />
            </label>
            <label>
              Nozzle (mm)
              <input
                type="number"
                step="0.1"
                min="0.1"
                value={settings.nozzleDiameter}
                onChange={(e) => update("nozzleDiameter", Number(e.target.value))}
                aria-label="Nozzle diameter"
              />
            </label>
            <label>
              Material
              <select
                value={settings.material}
                onChange={(e) => update("material", e.target.value)}
                aria-label="Material"
              >
                <option value="PLA">PLA</option>
                <option value="ABS">ABS</option>
                <option value="PETG">PETG</option>
                <option value="TPU">TPU</option>
              </select>
            </label>
            <button className="primary" onClick={doSlice}>
              Slice
            </button>
          </section>

          <section className="panel slicer-preview" aria-label="Layer preview">
            <h2 className="panel-title">Preview</h2>
            {!result && <p className="muted">Adjust settings and press Slice.</p>}
            {result && layer && (
              <>
                <svg
                  className="layer-svg"
                  viewBox={viewBox}
                  role="img"
                  aria-label={`Layer ${layerIndex + 1} of ${result.layerCount}`}
                >
                  <g transform="scale(1,-1)">
                    {layer.infill.map((s, i) => (
                      <line
                        key={`i${i}`}
                        x1={s[0]}
                        y1={s[1]}
                        x2={s[2]}
                        y2={s[3]}
                        className="infill-line"
                      />
                    ))}
                    {layer.perimeters.map((loop, i) => (
                      <polygon
                        key={`p${i}`}
                        points={loop.map((p) => `${p[0]},${p[1]}`).join(" ")}
                        className="perimeter-loop"
                      />
                    ))}
                  </g>
                </svg>
                <label className="layer-slider">
                  Layer {layerIndex + 1} / {result.layerCount} (Z=
                  {layer.z.toFixed(2)}mm)
                  <input
                    type="range"
                    min={0}
                    max={result.layerCount - 1}
                    value={layerIndex}
                    onChange={(e) => setLayerIndex(Number(e.target.value))}
                    aria-label="Layer selector"
                  />
                </label>
              </>
            )}
          </section>

          <section className="panel slicer-stats" aria-label="Print estimates">
            <h2 className="panel-title">Estimates</h2>
            {result ? (
              <ul className="stats-list">
                <li>
                  <span>Layers</span>
                  <span className="mono">{result.layerCount}</span>
                </li>
                <li>
                  <span>Filament</span>
                  <span className="mono">
                    {(result.estimatedFilamentMm / 1000).toFixed(2)} m
                  </span>
                </li>
                <li>
                  <span>Print time</span>
                  <span className="mono">
                    {Math.floor(result.estimatedTimeS / 3600)}h{" "}
                    {Math.floor((result.estimatedTimeS % 3600) / 60)}m
                  </span>
                </li>
                <li>
                  <span>Size</span>
                  <span className="mono">
                    {result.size.map((v) => v.toFixed(0)).join(" × ")} mm
                  </span>
                </li>
              </ul>
            ) : (
              <p className="muted">No slice yet.</p>
            )}
            <button className="primary" disabled={!result} onClick={downloadGcode}>
              Download G-code
            </button>
            <h2 className="panel-title">Send to printer</h2>
            {printers.length === 0 ? (
              <p className="muted">No printers. Add one in the Printers panel.</p>
            ) : (
              <label>
                Printer
                <select
                  value={selectedPrinter}
                  onChange={(e) => setSelectedPrinter(e.target.value)}
                  aria-label="Target printer"
                >
                  {printers.map((p) => (
                    <option key={p.id} value={p.id}>
                      {p.name}
                    </option>
                  ))}
                </select>
              </label>
            )}
            <button
              className="primary"
              disabled={!result || printers.length === 0}
              onClick={onSend}
            >
              Send to Printer
            </button>
            {sendStatus && <p className="mono">{sendStatus}</p>}
            {sendError && <p className="error">{sendError}</p>}
          </section>
        </div>
      </div>
    </div>
  );
}
