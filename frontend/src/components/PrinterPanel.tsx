import { useEffect, useState } from "react";
import {
  deletePrinter,
  deletePrinterKey,
  discoveredPrinterToTarget,
  discoverPrinters,
  getPrinterKey,
  isTauri,
  listPrinters,
  savePrinter,
  setPrinterKey,
  testPrinter,
  type ConnectionInfo,
  type DiscoveredPrinter,
  type PrinterTarget,
  type ProtocolKind,
} from "../printer/client";
import { PrinterSetupWizard } from "./PrinterSetupWizard";

/**
 * Printer management panel (Phase 13): list saved printers, add/edit/remove
 * them, and probe their connection. "Send to Printer" lives in the slicer
 * panel, which holds the G-code.
 */
export function PrinterPanel({ onClose }: { onClose: () => void }) {
  const [printers, setPrinters] = useState<PrinterTarget[]>([]);
  const [editing, setEditing] = useState<PrinterTarget | null>(null);
  const [info, setInfo] = useState<Record<string, ConnectionInfo>>({});
  const [error, setError] = useState<string | null>(null);
  const [discovered, setDiscovered] = useState<DiscoveredPrinter[]>([]);
  const [discovering, setDiscovering] = useState(false);
  const [wizardOpen, setWizardOpen] = useState(false);

  const refresh = () => {
    listPrinters()
      .then(setPrinters)
      .catch((e) => setError(String(e)));
  };

  useEffect(refresh, []);

  const blank = (): PrinterTarget => ({
    id: crypto.randomUUID(),
    name: "",
    kind: "octoprint" as ProtocolKind,
    base_url: "http://",
    api_key: null,
  });

  const onSave = async () => {
    if (!editing) return;
    if (!editing.name.trim() || !editing.base_url.trim()) {
      setError("Name and base URL are required.");
      return;
    }
    try {
      // The API key lives only in the OS keychain; the saved target never
      // embeds it in printers.json.
      if (editing.api_key) {
        await setPrinterKey(editing.id, editing.api_key);
      }
      setPrinters(await savePrinter({ ...editing, api_key: null }));
      setEditing(null);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };

  const onDelete = async (id: string) => {
    try {
      await deletePrinterKey(id).catch(() => {});
      setPrinters(await deletePrinter(id));
    } catch (e) {
      setError(String(e));
    }
  };

  const onEdit = async (p: PrinterTarget) => {
    // Prefill the (masked) API key from the OS keychain for editing.
    const key = await getPrinterKey(p.id).catch(() => null);
    setEditing({ ...p, api_key: key });
  };

  const onTest = async (p: PrinterTarget) => {
    try {
      const ci = await testPrinter(p);
      setInfo((prev) => ({ ...prev, [p.id]: ci }));
    } catch (e) {
      setError(String(e));
    }
  };

  const onDiscover = async () => {
    setDiscovering(true);
    setError(null);
    try {
      const result = await discoverPrinters();
      setDiscovered(result.printers);
      if (result.printers.length === 0 && result.errors.length > 0) {
        setError(`No printers found (${result.errors.join("; ")})`);
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setDiscovering(false);
    }
  };

  const onAddDiscovered = (p: DiscoveredPrinter) => {
    setEditing(discoveredPrinterToTarget(p));
  };

  return (
    <>
      <div className="vc-backdrop" role="dialog" aria-label="Printers">
        <div className="vc-card printer-card">
          <header className="vc-header">
            <h3>Printers</h3>
            <div className="spacer" />
            <button onClick={onClose} aria-label="Close">
              Close
            </button>
          </header>

          {!isTauri() && (
            <p className="muted">
              Printer control is available in the desktop app. In the browser you
              can configure targets, but testing/sending requires Tauri.
            </p>
          )}
          {error && <p className="error">{error}</p>}

          <div className="printer-body">
            <section className="panel" aria-label="Saved printers">
              <h2 className="panel-title">Saved</h2>
              {printers.length === 0 && <p className="muted">No printers saved yet.</p>}
              <ul className="printer-list">
                {printers.map((p) => (
                  <li key={p.id} className="printer-row">
                    <div>
                      <strong>{p.name}</strong>{" "}
                      <span className="muted">({p.kind})</span>
                      {info[p.id] && (
                        <span className={info[p.id].connected ? "ok" : "error"}>
                          {" "}
                          — {info[p.id].connected ? "online" : "offline"}
                          {info[p.id].firmware ? ` · ${info[p.id].firmware}` : ""}
                        </span>
                      )}
                    </div>
                    <div className="row-actions">
                      <button onClick={() => onTest(p)}>Test</button>
                      <button onClick={() => onEdit(p)}>Edit</button>
                      <button onClick={() => onDelete(p.id)}>Delete</button>
                    </div>
                  </li>
                ))}
              </ul>
              <div className="row-actions">
                <button className="primary" onClick={() => setEditing(blank())}>
                  Add printer
                </button>
                {/* Guided alternative to the manual editor below. */}
                <button
                  onClick={() => setWizardOpen(true)}
                  aria-label="Find my printer"
                >
                  Find my printer…
                </button>
              </div>
            </section>

            <section className="panel" aria-label="Discovered printers">
              <h2 className="panel-title">Discover</h2>
              {!isTauri() && (
                <p className="muted">Discovery is available in the desktop app.</p>
              )}
              <button onClick={onDiscover} disabled={discovering || !isTauri()}>
                {discovering ? "Scanning…" : "Scan LAN"}
              </button>
              {discovered.length === 0 ? (
                <p className="muted">
                  {discovering ? "Listening for mDNS responses…" : "No printers found yet."}
                </p>
              ) : (
                <ul className="printer-list">
                  {discovered.map((p, i) => (
                    <li key={`${p.hostname}:${p.port}:${i}`} className="printer-row">
                      <div>
                        <strong>{p.name}</strong> <span className="muted">({p.protocol})</span>
                        <br />
                        <span className="muted">
                          {p.hostname} ({p.ip}:{p.port})
                        </span>
                      </div>
                      <div className="row-actions">
                        <button onClick={() => onAddDiscovered(p)}>Add</button>
                      </div>
                    </li>
                  ))}
                </ul>
              )}
            </section>

            {editing && (
              <section className="panel" aria-label="Printer editor">
                <h2 className="panel-title">Edit</h2>
                <label>
                  Name
                  <input
                    value={editing.name}
                    onChange={(e) => setEditing({ ...editing, name: e.target.value })}
                    aria-label="Printer name"
                  />
                </label>
                <label>
                  Protocol
                  <select
                    value={editing.kind}
                    onChange={(e) =>
                      setEditing({ ...editing, kind: e.target.value as ProtocolKind })
                    }
                    aria-label="Protocol"
                  >
                    <option value="octoprint">OctoPrint</option>
                    <option value="moonraker-compat">Moonraker (compat)</option>
                    <option value="esp3d">ESP3D</option>
                  </select>
                </label>
                <label>
                  Base URL
                  <input
                    value={editing.base_url}
                    onChange={(e) => setEditing({ ...editing, base_url: e.target.value })}
                    aria-label="Base URL"
                  />
                </label>
                <label>
                  API key
                  <input
                    type="password"
                    autoComplete="new-password"
                    value={editing.api_key ?? ""}
                    onChange={(e) =>
                      setEditing({
                        ...editing,
                        api_key: e.target.value === "" ? null : e.target.value,
                      })
                    }
                    aria-label="API key"
                  />
                </label>
                <div className="row-actions">
                  <button className="primary" onClick={onSave}>
                    Save
                  </button>
                  <button onClick={() => setEditing(null)}>Cancel</button>
                </div>
              </section>
            )}
          </div>
        </div>
      </div>

      {wizardOpen && (
        <PrinterSetupWizard
          onClose={() => setWizardOpen(false)}
          onSaved={() => {
            setWizardOpen(false);
            setError(null);
            refresh();
          }}
        />
      )}
    </>
  );
}
