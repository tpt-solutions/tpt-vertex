import {
  CAPABILITIES,
  STATUS_META,
  STATUS_ORDER,
  capabilitiesByPhase,
  type CapabilityStatus,
} from "../state/capabilityStatus";

/**
 * Capability status / transparency panel (Phase 13): renders the
 * `capabilityStatus` manifest so users can see, at a glance, which subsystems
 * are real and which are placeholders or still being wired up. Mirrors the
 * `PrinterPanel`/`SimulationPanel` modal layout.
 */
export function CapabilityStatusPanel({ onClose }: { onClose: () => void }) {
  const byPhase = capabilitiesByPhase();
  const phases = Object.keys(byPhase)
    .map(Number)
    .sort((a, b) => a - b);

  const counts = STATUS_ORDER.map((status) => ({
    status,
    count: CAPABILITIES.filter((c) => c.status === status).length,
  }));

  return (
    <div className="vc-backdrop" role="dialog" aria-modal="true" aria-label="Capability status">
      <div className="vc-card capability-card">
        <header className="vc-header">
          <h3>Capability Status</h3>
          <div className="spacer" />
          <button onClick={onClose} aria-label="Close">
            Close
          </button>
        </header>

        <p className="muted">
          An honest map of what TPT Vertex actually does today. Anything not marked <em>real</em>{" "}
          should not be trusted for production work.
        </p>

        <ul className="capability-legend" aria-label="Status legend">
          {counts.map(({ status, count }) => (
            <li key={status}>
              <StatusBadge status={status} />
              <span className="muted">
                {STATUS_META[status].description} ({count})
              </span>
            </li>
          ))}
        </ul>

        <div className="capability-body">
          {phases.map((phase) => (
            <section key={phase} className="panel" aria-label={`Phase ${phase} capabilities`}>
              <h2 className="panel-title">Phase {phase}</h2>
              <ul className="capability-list">
                {byPhase[phase].map((cap) => (
                  <li key={cap.id} className="capability-row" title={cap.notes}>
                    <StatusBadge status={cap.status} label={cap.label} />
                    <div className="capability-text">
                      <strong>{cap.label}</strong>
                      <span className="muted capability-notes">{cap.notes}</span>
                    </div>
                  </li>
                ))}
              </ul>
            </section>
          ))}
        </div>

        <p className="muted">
          Feature-tree entries backed by a placeholder subsystem (booleans, fillet/chamfer) carry
          the same badge next to the feature name.
        </p>
      </div>
    </div>
  );
}

/** Small colour-coded status pill, also used for feature-tree badges. */
function StatusBadge({ status, label }: { status: CapabilityStatus; label?: string }) {
  const meta = STATUS_META[status];
  return (
    <span
      className={`badge status-badge status-${status}`}
      aria-label={label ? `${label}: ${meta.label}` : meta.label}
    >
      {meta.label}
    </span>
  );
}
