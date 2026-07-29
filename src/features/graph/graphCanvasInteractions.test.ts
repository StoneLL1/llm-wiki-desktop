import Graph from "graphology";
import type Sigma from "sigma";
import type { MouseCoords, SigmaNodeEventPayload, SigmaStageEventPayload } from "sigma/types";
import { describe, expect, it, vi } from "vitest";

import { bindGraphCanvasInteractions, fitGraphToViewport } from "./graphCanvasInteractions";

type SigmaHandler = (payload: never) => void;
type MouseHandler = (payload: never) => void;

function createRendererHarness() {
  const sigmaHandlers = new Map<string, SigmaHandler>();
  const mouseHandlers = new Map<string, MouseHandler>();
  const camera = {
    disable: vi.fn(),
    enable: vi.fn(),
    animatedReset: vi.fn(async () => {}),
  };
  const renderer = {
    on: vi.fn((event: string, handler: SigmaHandler) => {
      sigmaHandlers.set(event, handler);
    }),
    off: vi.fn((event: string) => {
      sigmaHandlers.delete(event);
    }),
    getMouseCaptor: () => ({
      on: (event: string, handler: MouseHandler) => mouseHandlers.set(event, handler),
      off: (event: string) => mouseHandlers.delete(event),
    }),
    getCamera: () => camera,
    getCustomBBox: vi.fn(() => null),
    getBBox: vi.fn(() => ({ x: [0, 1], y: [0, 1] })),
    setCustomBBox: vi.fn(),
    viewportToGraph: vi.fn(({ x, y }: { x: number; y: number }) => ({ x: x / 10, y: y / 10 })),
  };

  return {
    renderer: renderer as unknown as Sigma,
    camera,
    emitSigma: (event: string, payload: unknown) => sigmaHandlers.get(event)?.(payload as never),
    emitMouse: (event: string, payload: unknown) => mouseHandlers.get(event)?.(payload as never),
    hasSigmaHandler: (event: string) => sigmaHandlers.has(event),
    hasMouseHandler: (event: string) => mouseHandlers.has(event),
    setCustomBBox: renderer.setCustomBBox,
  };
}

function pointerEvent(x = 0, y = 0): MouseCoords {
  return {
    x,
    y,
    sigmaDefaultPrevented: false,
    original: new MouseEvent("mousemove"),
    preventSigmaDefault: vi.fn(),
  };
}

function nodeEvent(node: string): SigmaNodeEventPayload {
  const event = pointerEvent();
  return {
    node,
    event,
    preventSigmaDefault: vi.fn(),
  };
}

describe("bindGraphCanvasInteractions", () => {
  it("clears selection when the user clicks the empty stage", () => {
    const graph = new Graph();
    const harness = createRendererHarness();
    const onClearSelection = vi.fn();
    const dispose = bindGraphCanvasInteractions(harness.renderer, graph, {
      onClearSelection,
      onDragStart: vi.fn(),
      onDragEnd: vi.fn(),
      onDragStateChange: vi.fn(),
    });

    harness.emitSigma("clickStage", {
      event: pointerEvent(),
      preventSigmaDefault: vi.fn(),
    } satisfies SigmaStageEventPayload);

    expect(onClearSelection).toHaveBeenCalledOnce();
    dispose();
  });

  it("moves one node without React state churn and commits once on pointer release", () => {
    const graph = new Graph();
    graph.addNode("a", { x: 0, y: 0 });
    const harness = createRendererHarness();
    const onDragStart = vi.fn();
    const onDragEnd = vi.fn();
    const onDragStateChange = vi.fn();
    const dispose = bindGraphCanvasInteractions(harness.renderer, graph, {
      onClearSelection: vi.fn(),
      onDragStart,
      onDragEnd,
      onDragStateChange,
    });

    const down = nodeEvent("a");
    harness.emitSigma("downNode", down);
    harness.emitMouse("mousemovebody", pointerEvent(2, 2));
    expect(onDragStart).not.toHaveBeenCalled();
    harness.emitMouse("mousemovebody", pointerEvent(20, 30));
    harness.emitMouse("mousemovebody", pointerEvent(40, 50));
    harness.emitMouse("mouseup", pointerEvent(40, 50));

    expect(graph.getNodeAttribute("a", "x")).toBe(4);
    expect(graph.getNodeAttribute("a", "y")).toBe(5);
    expect(harness.setCustomBBox).toHaveBeenCalledOnce();
    expect(harness.camera.disable).toHaveBeenCalledOnce();
    expect(harness.camera.enable).toHaveBeenCalledOnce();
    expect(onDragStart).toHaveBeenCalledOnce();
    expect(onDragEnd).toHaveBeenCalledOnce();
    expect(onDragEnd).toHaveBeenCalledWith("a");
    expect(onDragStateChange.mock.calls).toEqual([[true], [false]]);
    expect(down.event.preventSigmaDefault).toHaveBeenCalledOnce();
    expect(down.preventSigmaDefault).toHaveBeenCalledOnce();
    dispose();
  });

  it("does not commit pointer jitter below the drag threshold", () => {
    const graph = new Graph();
    graph.addNode("a", { x: 0, y: 0 });
    const harness = createRendererHarness();
    const onDragStart = vi.fn();
    const onDragEnd = vi.fn();
    bindGraphCanvasInteractions(harness.renderer, graph, {
      onClearSelection: vi.fn(),
      onDragStart,
      onDragEnd,
      onDragStateChange: vi.fn(),
    });

    harness.emitSigma("downNode", nodeEvent("a"));
    harness.emitMouse("mousemovebody", pointerEvent(2, 2));
    harness.emitMouse("mouseup", pointerEvent(2, 2));

    expect(onDragStart).not.toHaveBeenCalled();
    expect(onDragEnd).not.toHaveBeenCalled();
    expect(graph.getNodeAttribute("a", "x")).toBe(0);
    expect(graph.getNodeAttribute("a", "y")).toBe(0);
  });

  it("suppresses only the stage click synthesized immediately after a drag", () => {
    const graph = new Graph();
    graph.addNode("a", { x: 0, y: 0 });
    const harness = createRendererHarness();
    const onClearSelection = vi.fn();
    bindGraphCanvasInteractions(harness.renderer, graph, {
      onClearSelection,
      onDragStart: vi.fn(),
      onDragEnd: vi.fn(),
      onDragStateChange: vi.fn(),
    });

    harness.emitSigma("downNode", nodeEvent("a"));
    harness.emitMouse("mousemovebody", pointerEvent(20, 30));
    harness.emitMouse("mouseup", pointerEvent(20, 30));
    harness.emitSigma("clickStage", {
      event: pointerEvent(),
      preventSigmaDefault: vi.fn(),
    } satisfies SigmaStageEventPayload);
    harness.emitSigma("clickStage", {
      event: pointerEvent(),
      preventSigmaDefault: vi.fn(),
    } satisfies SigmaStageEventPayload);

    expect(onClearSelection).toHaveBeenCalledOnce();
  });

  it("releases the fixed drag bounds before fitting the live graph", () => {
    const harness = createRendererHarness();
    const refreshRenderer = vi.fn();

    fitGraphToViewport(harness.renderer, refreshRenderer);

    expect(harness.setCustomBBox).toHaveBeenCalledWith(null);
    expect(refreshRenderer).toHaveBeenCalledOnce();
    expect(harness.camera.animatedReset).toHaveBeenCalledWith({ duration: 300 });
  });

  it("removes every listener and restores the camera when disposed mid-drag", () => {
    const graph = new Graph();
    graph.addNode("a", { x: 0, y: 0 });
    const harness = createRendererHarness();
    const dispose = bindGraphCanvasInteractions(harness.renderer, graph, {
      onClearSelection: vi.fn(),
      onDragStart: vi.fn(),
      onDragEnd: vi.fn(),
      onDragStateChange: vi.fn(),
    });

    harness.emitSigma("downNode", nodeEvent("a"));
    dispose();

    expect(harness.camera.enable).toHaveBeenCalledOnce();
    expect(harness.hasSigmaHandler("clickStage")).toBe(false);
    expect(harness.hasSigmaHandler("downNode")).toBe(false);
    expect(harness.hasMouseHandler("mousemovebody")).toBe(false);
    expect(harness.hasMouseHandler("mouseup")).toBe(false);
  });
});
