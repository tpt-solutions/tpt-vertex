export function StatusBar({ featureCount }: { featureCount: number }) {
  return (
    <footer className="status-bar">
      <span>features: {featureCount}</span>
      <span className="spacer" />
      <span>ready</span>
    </footer>
  );
}
