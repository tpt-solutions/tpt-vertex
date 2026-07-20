import { useState } from "react";
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
import { useModelStore } from "./state/store";
import { useSketchStore } from "./state/sketchStore";
import { useKeyboardShortcuts } from "./hooks/useKeyboardShortcuts";

export function App() {
  const [theme, setTheme] = useState<"light" | "dark">("dark");
  const [historyOpen, setHistoryOpen] = useState(false);
  const [slicerOpen, setSlicerOpen] = useState(false);
  useKeyboardShortcuts();
  const featureCount = useModelStore((s) => s.features.length);
  const selected = useModelStore((s) => s.selectedFeatureId);
  const openSketch = useSketchStore((s) => s.openEditor);

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
      <Onboarding />
    </div>
  );
}
