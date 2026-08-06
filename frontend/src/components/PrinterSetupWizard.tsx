import { useEffect, useState } from "react";
import {
  discoveredPrinterToTarget,
  discoverPrinters,
  isTauri,
  savePrinter,
  setPrinterKey,
  testPrinter,
  type ConnectionInfo,
  type DiscoveredPrinter,
  type PrinterTarget,
  type ProtocolKind,
} from "../printer/client";

/** Wizard stages, in order. */
type Step = "discover" | "details" | "test";

const STEPS: Step[] = ["discover", "details", "test"];
const STEP_TITLES: Record<Step, string> = {
  discover: "Discover",
  details: "Details",
  test: "Test",
};

/** An empty target, used for the manual-entry fallback. */
function blankTarget(): PrinterTarget {
  return {
    id: crypto.randomUUID(),
    name: "",
    kind: "octoprint" as ProtocolKind,
    base_url: "http://",
    api_key: null,
  };
}

/**
 * Guided "Find my printer" wizard (Phase 13).
 *
 * Walks through mDNS discovery (`discover_printers`), confirming/editing the
 * connection details, and a live connection test before saving the printer.
 * The manual-entry flow in `PrinterPanel` remains available; this is the
 * hand-held path for users who don't know their printer's URL.
 *
 * As everywhere else in the app, the API key is stored in the OS keychain via
 * `setPrinterKey` and never written into `printers.json`.
 */
export function PrinterSetupWizard({
  onClose,
  onSaved,
}: {
  onClose: () => void;
  onSaved?: (target: PrinterTarget) => void;
}) {
  const [step, setStep] = useState<Step>("discover");
  const [scanning, setScanning] = useState(false);
  const [scanned, setScanned] = useState(false);
  const [found, setFound] = useState<DiscoveredPrinter[]>([]);
  const [discoveryErrors, setDiscoveryErrors] = useState<string[]>([]);
  const [selected, setSelected] = useState<number | null>(null);
  const [target, setTarget] = useState<PrinterTarget>(blankTarget);
  const [apiKey, setApiKey] = useState("");
  const [testing, setTesting] = useState(false);
  const [conn, setConn] = useState<ConnectionInfo | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const scan = async () => {
    setScanning(true);
    setError(null);
    setDiscoveryErrors([]);
    try {
      const result = await discoverPrinters();
      setFound(result.printers);
      setDiscoveryErrors(result.errors);
      setSelected(result.printers.length > 0 ? 0 : null);
      if (result.printers.length > 0) {
        setTarget(discoveredPrinterToTarget(result.printers[0]));
      }
    } catch (e) {
      // A hard IPC failure still leaves the manual fallback usable.
      setDiscoveryErrors([String(e)]);
    } finally {
      setScanned(true);
      setScanning(false);
    }
  };

  // Kick off a scan as soon as the wizard opens (desktop only — in the browser
  // there is no mDNS backend, so we show a notice instead).
  useEffect(() => {
    if (isTauri()) void scan();
    else setScanned(true);
  }, []);

  const pick = (i: number) => {
    setSelected(i);
    setTarget(discoveredPrinterToTarget(found[i]));
    setConn(null);
  };

  const manualEntry = () => {
    setSelected(null);
    setTarget(blankTarget());
    setConn(null);
    setError(null);
    setStep("details");
  };

  /** Target as sent to the backend for probing: key included, since it isn't in the keychain yet. */
  const probeTarget = (): PrinterTarget => ({
    ...target,
    name: target.name.trim(),
    base_url: target.base_url.trim(),
    api_key: apiKey.trim() === "" ? null : apiKey.trim(),
  });

  const runTest = async () => {
    setTesting(true);
    setError(null);
    setConn(null);
    try {
      setConn(await testPrinter(probeTarget()));
    } catch (e) {
      setError(String(e));
    } finally {
      setTesting(false);
    }
  };

  const detailsValid = target.name.trim() !== "" && target.base_url.trim() !== "";

  const next = () => {
    setError(null);
    if (step === "discover") {
      if (selected === null) {
        setError("Select a discovered printer, or choose manual entry.");
        return;
      }
      setStep("details");
    } else if (step === "details") {
      if (!detailsValid) {
        setError("Name and base URL are required.");
        return;
      }
      setStep("test");
      void runTest();
    }
  };

  const back = () => {
    setError(null);
    // Any earlier probe result is stale once the details can change again.
    if (step === "test") {
      setConn(null);
      setStep("details");
    } else if (step === "details") setStep("discover");
  };

  const finish = async () => {
    if (!detailsValid) {
      setError("Name and base URL are required.");
      return;
    }
    setSaving(true);
    setError(null);
    try {
      const saved = probeTarget();
      // Secret goes to the OS keychain; the persisted target never carries it.
      if (saved.api_key) await setPrinterKey(saved.id, saved.api_key);
      const stripped: PrinterTarget = { ...saved, api_key: null };
      await savePrinter(stripped);
      onSaved?.(stripped);
      onClose();
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="vc-backdrop" role="dialog" aria-label="Printer setup wizard">
      <div className="vc-card printer-card">
        <header className="vc-header">
          <h3>Find my printer</h3>
          <div className="spacer" />
          {STEPS.map((s, i) => (
            <span
              key={s}
              // Reuses the capability-badge styling: the active step is the bold
              // accent pill, the others stay flat.
              className={s === step ? "badge status-badge status-wip" : "badge"}
              aria-current={s === step ? "step" : undefined}
            >
              {i + 1}. {STEP_TITLES[s]}
            </span>
          ))}
          <button onClick={onClose} aria-label="Close">
            Close
          </button>
        </header>

        {error && (
          <p className="error" role="alert">
            {error}
          </p>
        )}

        {step === "discover" && (
          <section className="panel" aria-label="Discover printers">
            <h2 className="panel-title">Step 1 — Discover</h2>
            {!isTauri() ? (
              <p className="muted">
                Automatic discovery needs the desktop app — your browser can&apos;t
                listen for mDNS announcements. You can still enter your printer&apos;s
                details by hand below.
              </p>
            ) : (
              <p className="muted">
                {scanning
                  ? "Listening for mDNS responses on your network…"
                  : `Found ${found.length} printer${found.length === 1 ? "" : "s"}.`}
              </p>
            )}

            {found.length > 0 && (
              <ul className="printer-list">
                {found.map((p, i) => (
                  <li
                    key={`${p.hostname}:${p.port}:${i}`}
                    className={i === selected ? "printer-row selected" : "printer-row"}
                  >
                    <div>
                      <strong>{p.name}</strong> <span className="muted">({p.protocol})</span>
                      <br />
                      <span className="muted">
                        {p.ip}:{p.port} · {p.hostname}
                      </span>
                    </div>
                    <div className="row-actions">
                      <button
                        onClick={() => pick(i)}
                        aria-pressed={i === selected}
                        aria-label={`Select ${p.name}`}
                      >
                        {i === selected ? "Selected" : "Select"}
                      </button>
                    </div>
                  </li>
                ))}
              </ul>
            )}

            {scanned && !scanning && found.length === 0 && isTauri() && (
              <p className="muted">
                No printers answered. Check that the printer is powered on and on the
                same network, then scan again — or enter its details manually.
              </p>
            )}

            {discoveryErrors.length > 0 && (
              <p className="muted">Discovery notes: {discoveryErrors.join("; ")}</p>
            )}

            <div className="row-actions">
              <button onClick={() => void scan()} disabled={scanning || !isTauri()}>
                {scanning ? "Scanning…" : scanned ? "Scan again" : "Scan LAN"}
              </button>
              <button onClick={manualEntry}>Enter details manually</button>
            </div>
          </section>
        )}

        {step === "details" && (
          <section className="panel" aria-label="Printer details">
            <h2 className="panel-title">Step 2 — Details</h2>
            <p className="muted">
              Confirm how TPT Vertex should reach this printer. The API key is stored in
              your OS keychain, not in the project files.
            </p>
            <label>
              Name
              <input
                value={target.name}
                onChange={(e) => setTarget({ ...target, name: e.target.value })}
                aria-label="Printer name"
              />
            </label>
            <label>
              Protocol
              <select
                value={target.kind}
                onChange={(e) =>
                  setTarget({ ...target, kind: e.target.value as ProtocolKind })
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
                value={target.base_url}
                onChange={(e) => setTarget({ ...target, base_url: e.target.value })}
                aria-label="Base URL"
              />
            </label>
            <label>
              API key (optional)
              <input
                type="password"
                autoComplete="new-password"
                value={apiKey}
                onChange={(e) => setApiKey(e.target.value)}
                aria-label="API key"
              />
            </label>
          </section>
        )}

        {step === "test" && (
          <section className="panel" aria-label="Test connection">
            <h2 className="panel-title">Step 3 — Test</h2>
            <p className="muted">
              {target.name} · {target.base_url} ({target.kind})
            </p>
            {testing && <p className="muted">Contacting printer…</p>}
            {!testing && conn && (
              <p className={conn.connected ? "ok" : "error"}>
                {conn.connected ? "Connected" : "Not responding"} — {conn.host} ·{" "}
                {conn.protocol}
                {conn.firmware ? ` · ${conn.firmware}` : ""}
              </p>
            )}
            {!testing && !conn && !error && (
              <p className="muted">Run the test to confirm the connection.</p>
            )}
            <div className="row-actions">
              <button onClick={() => void runTest()} disabled={testing}>
                {testing ? "Testing…" : conn || error ? "Test again" : "Test"}
              </button>
            </div>
          </section>
        )}

        <div className="row-actions">
          <button onClick={back} disabled={step === "discover"}>
            Back
          </button>
          {step === "test" ? (
            <button className="primary" onClick={() => void finish()} disabled={saving}>
              {saving ? "Saving…" : conn?.connected ? "Finish" : "Save anyway"}
            </button>
          ) : (
            <button
              className="primary"
              onClick={next}
              disabled={step === "details" && !detailsValid}
            >
              Next
            </button>
          )}
          <button onClick={onClose}>Cancel</button>
        </div>
      </div>
    </div>
  );
}
