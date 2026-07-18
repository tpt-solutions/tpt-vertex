import "@testing-library/jest-dom";

// jsdom does not implement ResizeObserver, which @react-three/fiber's <Canvas>
// (via react-use-measure) requires. Provide a no-op polyfill so components that
// mount the R3F canvas can render in tests.
if (typeof globalThis.ResizeObserver === "undefined") {
  class ResizeObserverStub {
    observe() {}
    unobserve() {}
    disconnect() {}
  }
  globalThis.ResizeObserver = ResizeObserverStub as unknown as typeof globalThis.ResizeObserver;
}

// jsdom does not implement the canvas 2D context. Provide a no-op stub so
// components that draw to a <canvas> (e.g. SketchPreview) can render in tests
// without emitting "Not implemented: HTMLCanvasElement.prototype.getContext".
if (typeof HTMLCanvasElement !== "undefined") {
  const noop = () => {};
  const stub = {
    canvas: undefined,
    clearRect: noop,
    beginPath: noop,
    moveTo: noop,
    lineTo: noop,
    arc: noop,
    stroke: noop,
    fill: noop,
    closePath: noop,
    save: noop,
    restore: noop,
    translate: noop,
    scale: noop,
    fillRect: noop,
    strokeRect: noop,
    fillText: noop,
    measureText: () => ({ width: 0 }),
    setTransform: noop,
  };
  HTMLCanvasElement.prototype.getContext = (() =>
    stub) as unknown as typeof HTMLCanvasElement.prototype.getContext;
}
