import { useState } from "react";
import { Viewport } from "./components/Viewport";
import { FeatureTreePanel } from "./components/FeatureTreePanel";
import { PropertiesPanel } from "./components/PropertiesPanel";
import { AssemblyTreePanel } from "./components/AssemblyTreePanel";
import { Toolbar } from "./components/Toolbar";
import { StatusBar } from "./components/StatusBar";
import { useModelStore } from "./state/store";
import { useKeyboardShortcuts } from "./hooks/useKeyboardShortcuts";

export function App() {
  const [theme, setTheme] = useState<"light" | "dark">("dark");
  useKeyboardShortcuts();
  const featureCount = useModelStore((s) => s.features.length);
  const selected = useModelStore((s) => s.selectedFeatureId);

  return (
    <div className={`app ${theme}`}>
      <Toolbar onToggleTheme={() => setTheme((t) => (t === "dark" ? "light" : "dark"))} />
      <div className="workspace">
        <aside className="left-rail">
          <FeatureTreePanel />
          <AssemblyTreePanel />
        </aside>
        <main className="viewport-region">
          <Viewport />
        </main>
        <aside className="right-rail">
          <PropertiesPanel featureId={selected} />
        </aside>
      </div>
      <StatusBar featureCount={featureCount} />
    </div>
  );
}
