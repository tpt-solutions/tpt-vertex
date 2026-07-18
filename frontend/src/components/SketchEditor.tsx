import { useRef } from "react";
import { useSketchStore, type SketchPoint } from "../state/sketchStore";

function toLocal(el: HTMLCanvasElement, e: React.PointerEvent<HTMLCanvasElement>): SketchPoint {
  const rect = el.getBoundingClientRect();
  return { x: e.clientX - rect.left, y: e.clientY - rect.top };
}

export function SketchEditor() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const open = useSketchStore((s) => s.open);
  const tool = useSketchStore((s) => s.tool);
  const entities = useSketchStore((s) => s.entities);
  const draft = useSketchStore((s) => s.draft);
  const setTool = useSketchStore((s) => s.setTool);
  const beginPoint = useSketchStore((s) => s.beginPoint);
  const updateDraft = useSketchStore((s) => s.updateDraft);
  const commitDraft = useSketchStore((s) => s.commitDraft);
  const closeEditor = useSketchStore((s) => s.closeEditor);
  const clear = useSketchStore((s) => s.clear);

  if (!open) return null;

  return (
    <div className="sketch-editor" role="dialog" aria-label="Sketch editor">
      <div className="sketch-toolbar">
        <span className="sketch-title">Sketch</span>
        <button className={tool === "select" ? "active" : ""} onClick={() => setTool("select")}>
          Select
        </button>
        <button className={tool === "line" ? "active" : ""} onClick={() => setTool("line")}>
          Line
        </button>
        <button className={tool === "circle" ? "active" : ""} onClick={() => setTool("circle")}>
          Circle
        </button>
        <button onClick={clear}>Clear</button>
        <button className="close" onClick={closeEditor}>
          Done
        </button>
      </div>
      <canvas
        ref={canvasRef}
        width={640}
        height={420}
        className="sketch-canvas"
        onPointerDown={(e) => beginPoint(toLocal(e.currentTarget, e))}
        onPointerMove={(e) => updateDraft(toLocal(e.currentTarget, e))}
        onPointerUp={() => commitDraft()}
      ></canvas>
      <SketchPreview canvasRef={canvasRef} entities={entities} draft={draft} />
    </div>
  );
}

function SketchPreview({
  canvasRef,
  entities,
  draft,
}: {
  canvasRef: React.RefObject<HTMLCanvasElement>;
  entities: ReturnType<typeof useSketchStore.getState>["entities"];
  draft: ReturnType<typeof useSketchStore.getState>["draft"];
}) {
  const canvas = canvasRef.current;
  if (!canvas) return null;
  const ctx = canvas.getContext("2d");
  if (!ctx) return null;

  ctx.clearRect(0, 0, canvas.width, canvas.height);
  ctx.strokeStyle = "#4f9dff";
  ctx.lineWidth = 2;

  for (const ent of entities) {
    drawEntity(ctx, ent.points, ent.kind);
  }
  if (draft) {
    ctx.strokeStyle = "#ffd166";
    drawEntity(ctx, draft, draft[0] === draft[1] ? "line" : "line");
  }
  return null;
}

function drawEntity(
  ctx: CanvasRenderingContext2D,
  pts: [SketchPoint, SketchPoint],
  kind: "line" | "circle",
) {
  ctx.beginPath();
  if (kind === "circle") {
    const r = Math.hypot(pts[1].x - pts[0].x, pts[1].y - pts[0].y);
    ctx.arc(pts[0].x, pts[0].y, r, 0, Math.PI * 2);
  } else {
    ctx.moveTo(pts[0].x, pts[0].y);
    ctx.lineTo(pts[1].x, pts[1].y);
  }
  ctx.stroke();
}
