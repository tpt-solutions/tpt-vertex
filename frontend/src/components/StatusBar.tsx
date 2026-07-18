export function StatusBar({ featureCount }: { featureCount: number }) {
  return (
    <footer className="status-bar" role="status" aria-live="polite">
      <span>features: {featureCount}</span>
      <span className="spacer" />
      <span>ready</span>
    </footer>
  );
}
