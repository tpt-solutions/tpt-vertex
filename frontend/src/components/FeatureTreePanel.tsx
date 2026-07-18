import { useModelStore } from "../state/store";

export function FeatureTreePanel() {
  const features = useModelStore((s) => s.features);
  const selected = useModelStore((s) => s.selectedFeatureId);
  const hovered = useModelStore((s) => s.hoveredFeatureId);
  const setSelected = useModelStore((s) => s.setSelected);
  const setHovered = useModelStore((s) => s.setHovered);

  return (
    <section className="panel feature-tree" aria-label="Feature tree">
      <h2 className="panel-title">Feature Tree</h2>
      <ul role="listbox" aria-label="Features" aria-activedescendant={selected ?? undefined}>
        {features.map((f) => (
          <li
            key={f.id}
            id={f.id}
            role="option"
            tabIndex={0}
            aria-selected={f.id === selected}
            className={(f.id === selected ? "selected " : "") + (f.id === hovered ? "hovered" : "")}
            onClick={() => setSelected(f.id)}
            onKeyDown={(e) => {
              if (e.key === "Enter" || e.key === " ") {
                e.preventDefault();
                setSelected(f.id);
              }
            }}
            onMouseEnter={() => setHovered(f.id)}
            onMouseLeave={() => setHovered(null)}
          >
            <span className={`badge ${f.type}`}>{f.type}</span>
            {f.label}
          </li>
        ))}
      </ul>
    </section>
  );
}
