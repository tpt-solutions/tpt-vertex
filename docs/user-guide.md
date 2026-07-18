# TPT Vertex User Guide

TPT Vertex is a modern, cross-platform, parametric 3D CAD tool — the "Figma for
Hardware." It runs in the browser via WebGPU and as a lightweight desktop app,
with real-time multi-user collaboration and Git-like version control built in.

## Getting started

### In the browser

TPT Vertex renders with WebGPU. Use a recent Chromium-based browser or another
browser with WebGPU enabled. If WebGPU is unavailable, the app warns you and
falls back where possible.

Open the app, and you'll see:

- **Toolbar** (top): Undo/Redo, Sketch, History (version control), and Theme.
- **Left panels**: the Feature Tree and the Assembly outliner.
- **Viewport** (center): the 3D scene. Orbit, pan, and zoom with the mouse.
- **Inspector** (right): parameters for the selected feature.
- **Status bar** (bottom): live model info.

A short onboarding tour runs on first launch.

### On the desktop

The desktop app (built with Tauri) wraps the same interface and runs the
geometry kernel locally for offline-first editing, with native file open/save.
See [desktop/README.md](../desktop/README.md) to build it.

## Sketching

Click **Sketch** to open the 2D editor. Choose a tool (line, circle) and draw on
the plane. Sketches are the basis for 3D features like extrude and revolve.

## Features & the feature tree

The **Feature Tree** is your model's history: each entry (sketch, extrude,
revolve, boolean…) is a parametric operation. Selecting a feature shows its
parameters in the Inspector; changing a value rebuilds the model live.

The feature tree is the *source of truth* — everything downstream (rendering,
export, versioning, collaboration) derives from it.

## Assemblies

Combine multiple parts into an **assembly** and relate them with mates
(coincident, offset, axis-aligned). The Assembly panel outlines the structure.

## Version control

Click **History** to open version control:

- **Commit** your current state with a message.
- **Branch** to explore changes in isolation, and **checkout** to switch.
- **Merge** another branch; if both branches changed the same feature, a
  **conflict** appears and you choose which side to keep per feature.
- The **timeline** lists commits newest-first with branch tags; selecting a
  commit shows a **diff** of what changed, down to individual parameters.

## Collaboration

Multiple people can edit the same model at once. Changes merge conflict-free via
CRDTs, and you'll see collaborators' cursors and selections (presence). Access is
controlled per document: viewers can look, editors can change, owners can manage
sharing.

## Exporting & manufacturing

Export your model to standard formats:

- **STL** (binary/ASCII) for 3D printing.
- **OBJ** and **glTF** for visualization/interchange.
- **STEP** (AP203/214) for CAD interoperability; STEP can also be imported.
- **2D drawings** (orthographic SVG) and a **bill of materials** for assemblies.

Custom formats and tools can be added via the
[plugin interface](plugin-api.md).

## Keyboard shortcuts

- **Ctrl/Cmd+Z** — undo
- **Ctrl/Cmd+Shift+Z** — redo

## Troubleshooting

- **Blank viewport / WebGPU warning**: update your browser or enable WebGPU.
- **Import failed**: ensure the file is a supported format; very large or unusual
  STEP files may not fully reconstruct in v1 (faceted geometry is recovered).
