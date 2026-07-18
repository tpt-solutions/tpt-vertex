import { useModelStore } from "../state/store";

export function AssemblyTreePanel() {
  const assemblies = useModelStore((s) => s.assemblies);

  const render = (nodes: typeof assemblies) =>
    nodes.map((n) => (
      <li key={n.id}>
        <span className="assembly-node">{n.name}</span>
        {n.children.length > 0 && <ul>{render(n.children)}</ul>}
      </li>
    ));

  return (
    <section className="panel assembly-tree" aria-label="Assembly tree">
      <h2 className="panel-title">Assembly</h2>
      <ul className="assembly-root">{render(assemblies)}</ul>
    </section>
  );
}
