# Accessibility (a11y) — TPT Vertex

TPT Vertex aims to meet **WCAG 2.1 AA**. This document records the accessibility
pass on the frontend, what has been addressed, and outstanding follow-ups.

## Addressed

- **Landmarks**: the app uses `header` (toolbar), `main` (viewport), `aside`
  (panels), and `footer` (status bar) landmarks, each with an `aria-label`.
- **Skip link**: a "Skip to viewport" link is the first focusable element and
  jumps focus to `#main-viewport`.
- **Keyboard operability**:
  - All toolbar and dialog controls are native `<button>`/`<input>`/`<select>`.
  - The feature tree is an ARIA `listbox`; options are focusable (`tabIndex=0`),
    expose `aria-selected`, and respond to Enter/Space.
  - Undo/redo and other shortcuts are available via the keyboard
    (`useKeyboardShortcuts`).
- **Focus visibility**: a global `:focus-visible` outline meets WCAG 2.4.7.
- **Names & roles**: panels, dialogs (`role="dialog"` with `aria-label`), and the
  status bar (`role="status"`, `aria-live="polite"`) are labelled.
- **Forms**: parameter inputs use `<label>` wrapping the field; version-control
  inputs/selects carry `aria-label`s.

## Color & contrast

- Both light and dark themes use foreground/background pairs meeting AA contrast
  for body text. Accent-on-white and accent-on-dark combinations were chosen for
  ≥ 4.5:1 on text-sized elements.
- Diff badges convey meaning with **both** color and a text label
  (added/removed/modified), not color alone (WCAG 1.4.1).

## Testing

- Automated component tests assert labelled controls render and are operable.
- Manual pass recommended each release with:
  - Keyboard-only navigation (Tab/Shift+Tab/Enter/Escape).
  - A screen reader (NVDA/VoiceOver) smoke test of the main workflow.
  - `axe` DevTools on the built app.

## Follow-ups

- Roving `tabindex` and arrow-key navigation within the feature-tree listbox.
- `Escape` to close the version-control and sketch dialogs; focus trapping and
  restoration on dialog open/close.
- Respect `prefers-reduced-motion` for the skip-link and any future animations.
- Announce live collaboration presence changes via an `aria-live` region.
- 3D viewport: provide a non-visual summary (e.g. selected feature, dimensions)
  for assistive tech, since the canvas itself is not directly perceivable.
