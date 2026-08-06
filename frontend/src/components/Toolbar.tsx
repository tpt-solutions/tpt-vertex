import { useModelStore } from "../state/store";

export function Toolbar({
  onToggleTheme,
  onOpenSketch,
  onOpenHistory,
  onOpenSlicer,
  onOpenSimulation,
  onOpenPrinters,
  onOpenCapabilities,
  onOpenExport,
  onAddFeature,
}: {
  onToggleTheme: () => void;
  onOpenSketch: () => void;
  onOpenHistory: () => void;
  onOpenSlicer: () => void;
  onOpenSimulation: () => void;
  onOpenPrinters: () => void;
  onOpenCapabilities: () => void;
  onOpenExport: () => void;
  onAddFeature: (type: string) => void;
}) {
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
        <span className="tool-sep" />
        <button onClick={() => onAddFeature("extrude")} title="Add an extruded box">
          Add Box
        </button>
        <button onClick={() => onAddFeature("revolve")} title="Add a revolve">
          Revolve
        </button>
        <button onClick={() => onAddFeature("fillet")} title="Fillet edges">
          Fillet
        </button>
        <button onClick={() => onAddFeature("chamfer")} title="Chamfer edges">
          Chamfer
        </button>
        <button onClick={() => onAddFeature("boolean")} title="Boolean union with a box">
          Union
        </button>
        <button
          onClick={() => onAddFeature("boolean-subtract")}
          title="Boolean subtract a box"
        >
          Subtract
        </button>
        <button
          onClick={() => onAddFeature("boolean-intersect")}
          title="Boolean intersect with a box"
        >
          Intersect
        </button>
        <span className="tool-sep" />
        <button onClick={onOpenSketch} title="Open sketch editor">
          Sketch
        </button>
        <button onClick={onOpenHistory} title="Version control &amp; history">
          History
        </button>
        <button onClick={onOpenSlicer} title="Slice for 3D printing">
          Slice
        </button>
        <button onClick={onOpenExport} title="Export CAD formats">
          Export
        </button>
        <button onClick={onOpenPrinters} title="Manage printers &amp; send to print">
          Printers
        </button>
        <button onClick={onOpenSimulation} title="Run simulation &amp; motion study">
          Simulate
        </button>
        <button onClick={onOpenCapabilities} title="What's real vs. placeholder">
          Capabilities
        </button>
        <button onClick={onToggleTheme} title="Toggle theme">
          Theme
        </button>
      </div>
    </header>
  );
}
