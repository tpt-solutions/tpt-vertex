import { useModelStore } from "../state/store";

export function PropertiesPanel({ featureId }: { featureId: string | null }) {
  const feature = useModelStore((s) => s.features.find((f) => f.id === featureId));
  const updateParam = useModelStore((s) => s.updateParam);

  if (!feature) {
    return (
      <section className="panel properties" aria-label="Properties">
        <h2 className="panel-title">Properties</h2>
        <p className="muted">No feature selected.</p>
      </section>
    );
  }

  return (
    <section className="panel properties" aria-label="Properties">
      <h2 className="panel-title">Properties</h2>
      <div className="prop-name">{feature.label}</div>
      {Object.entries(feature.params).map(([key, value]) => (
        <label key={key} className="prop-row">
          <span>{key}</span>
          <input
            type={typeof value === "number" ? "number" : "text"}
            value={value}
            onChange={(e) =>
              updateParam(
                feature.id,
                key,
                typeof value === "number" ? Number(e.target.value) : e.target.value,
              )
            }
          />
        </label>
      ))}
    </section>
  );
}
