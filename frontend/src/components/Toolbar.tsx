import { useModelStore } from "../state/store";

export function Toolbar({ onToggleTheme }: { onToggleTheme: () => void }) {
  const undo = useModelStore((s) => s.undo);
  const redo = useModelStore((s) => s.redo);

  return (
    <header className="toolbar">
      <div className="brand">TPT Vertex</div>
      <div className="tools">
        <button onClick={undo} title="Undo (Ctrl+Z)">
          Undo
        </button>
        <button onClick={redo} title="Redo (Ctrl+Shift+Z)">
          Redo
        </button>
        <button onClick={onToggleTheme} title="Toggle theme">
          Theme
        </button>
      </div>
    </header>
  );
}
