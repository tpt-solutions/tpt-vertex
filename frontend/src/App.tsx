import { useEffect, useState } from "react";
import { Viewport } from "./components/Viewport";
import { FeatureTreePanel } from "./components/FeatureTreePanel";
import { PropertiesPanel } from "./components/PropertiesPanel";
import { AssemblyTreePanel } from "./components/AssemblyTreePanel";
import { Toolbar } from "./components/Toolbar";
import { StatusBar } from "./components/StatusBar";
import { SketchEditor } from "./components/SketchEditor";
import { Onboarding } from "./components/Onboarding";
import { VersionControl } from "./components/VersionControl";
import { SlicerPanel } from "./components/SlicerPanel";
import { SimulationPanel } from "./components/SimulationPanel";
import { PrinterPanel } from "./components/PrinterPanel";
import { CapabilityStatusPanel } from "./components/CapabilityStatusPanel";
import { ExportPanel } from "./components/ExportPanel";
import { useModelStore } from "./state/store";
import { useSketchStore } from "./state/sketchStore";
import { useKeyboardShortcuts } from "./hooks/useKeyboardShortcuts";
import { loadKernel } from "./kernel";
import { getCollabClient } from "./collab";

export function App() {
  const [theme, setTheme] = useState<"light" | "dark">("dark");
  const [historyOpen, setHistoryOpen] = useState(false);
  const [slicerOpen, setSlicerOpen] = useState(false);
  const [simOpen, setSimOpen] = useState(false);
  const [printersOpen, setPrintersOpen] = useState(false);
  const [capabilitiesOpen, setCapabilitiesOpen] = useState(false);
  const [exportOpen, setExportOpen] = useState(false);
  useKeyboardShortcuts();
  const featureCount = useModelStore((s) => s.features.length);
  const selected = useModelStore((s) => s.selectedFeatureId);
  const openSketch = useSketchStore((s) => s.openEditor);
  const addFeature = useModelStore((s) => s.addFeature);

  // Kick off the optional WASM kernel load (used by buildMesh when present).
  useEffect(() => {
    loadKernel();
    const client = getCollabClient({ displayName: "You", room: "demo", token: "dev" });
    client.connect();
    return () => client.disconnect();
  }, []);

  const onAddFeature = (type: string) => {
    const id = `f${Date.now()}`;
    switch (type) {
      case "extrude":
        addFeature({ id, type, label: "Box", params: { x0: 0, y0: 0, x1: 40, y1: 40, height: 30 } });
        break;
      case "revolve":
        addFeature({
          id,
          type,
          label: "Revolve",
          params: { x0: 0, y0: 0, x1: 20, y1: 20, height: 30, angle: 360 },
        });
        break;
      case "fillet":
        addFeature({ id, type, label: "Fillet", params: { radius: 2 } });
        break;
      case "chamfer":
        addFeature({ id, type, label: "Chamfer", params: { distance: 2 } });
        break;
      case "boolean":
      default: {
        const op = type.startsWith("boolean-")
          ? (type.split("-")[1] as string)
          : "union";
        addFeature({
          id,
          type: "boolean",
          label: `${op[0].toUpperCase()}${op.slice(1)}`,
          params: { op, bx0: 0, by0: 0, bx1: 20, by1: 20, bh: 20 },
        });
        break;
      }
    }
  };

  return (
    <div className={`app ${theme}`}>
      <a href="#main-viewport" className="skip-link">
        Skip to viewport
      </a>
      <Toolbar
        onToggleTheme={() => setTheme((t) => (t === "dark" ? "light" : "dark"))}
        onOpenSketch={openSketch}
        onOpenHistory={() => setHistoryOpen(true)}
        onOpenSlicer={() => setSlicerOpen(true)}
        onOpenSimulation={() => setSimOpen(true)}
        onOpenPrinters={() => setPrintersOpen(true)}
        onOpenCapabilities={() => setCapabilitiesOpen(true)}
        onOpenExport={() => setExportOpen(true)}
        onAddFeature={onAddFeature}
      />
      <div className="workspace">
        <aside className="left-rail" aria-label="Model panels">
          <FeatureTreePanel />
          <AssemblyTreePanel />
        </aside>
        <main id="main-viewport" className="viewport-region" aria-label="3D viewport">
          <Viewport />
        </main>
        <aside className="right-rail" aria-label="Inspector">
          <PropertiesPanel featureId={selected} />
        </aside>
      </div>
      <StatusBar featureCount={featureCount} />
      <SketchEditor />
      {historyOpen && <VersionControl onClose={() => setHistoryOpen(false)} />}
      {slicerOpen && <SlicerPanel onClose={() => setSlicerOpen(false)} />}
      {simOpen && <SimulationPanel onClose={() => setSimOpen(false)} />}
      {printersOpen && <PrinterPanel onClose={() => setPrintersOpen(false)} />}
      {capabilitiesOpen && <CapabilityStatusPanel onClose={() => setCapabilitiesOpen(false)} />}
      {exportOpen && <ExportPanel onClose={() => setExportOpen(false)} />}
      <Onboarding />
    </div>
  );
}
