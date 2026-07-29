import type Graph from "graphology";
import type Sigma from "sigma";
import type { MouseCoords, SigmaNodeEventPayload, SigmaStageEventPayload } from "sigma/types";

interface GraphCanvasInteractionCallbacks {
  onClearSelection: () => void;
  onDragStart: (nodeId: string) => void;
  onDragEnd: (nodeId: string) => void;
  onDragStateChange: (dragging: boolean) => void;
}

const DRAG_THRESHOLD_SQUARED = 9;

/**
 * Bind the canvas interactions that mutate live graph coordinates.
 *
 * Node movement deliberately stays outside React/Zustand: graphology emits a
 * single node-attribute update per pointer move and sigma schedules the WebGL
 * repaint. React only observes the final persisted layout.
 */
export function bindGraphCanvasInteractions(
  renderer: Sigma,
  graph: Graph,
  callbacks: GraphCanvasInteractionCallbacks,
): () => void {
  const mouseCaptor = renderer.getMouseCaptor();
  const camera = renderer.getCamera();
  let draggedNodeId: string | null = null;
  let nodeMoved = false;
  let pointerOrigin: { x: number; y: number } | null = null;
  let suppressStageClick = false;
  let suppressStageClickTimer: ReturnType<typeof setTimeout> | null = null;

  const clearDrag = () => {
    const nodeId = draggedNodeId;
    const moved = nodeMoved;
    draggedNodeId = null;
    nodeMoved = false;
    pointerOrigin = null;
    camera.enable();
    callbacks.onDragStateChange(false);
    if (nodeId && moved) {
      suppressStageClick = true;
      if (suppressStageClickTimer) clearTimeout(suppressStageClickTimer);
      suppressStageClickTimer = setTimeout(() => {
        suppressStageClick = false;
        suppressStageClickTimer = null;
      }, 0);
      callbacks.onDragEnd(nodeId);
    }
  };

  const onClickStage = (_payload: SigmaStageEventPayload) => {
    if (suppressStageClick) {
      suppressStageClick = false;
      return;
    }
    if (!draggedNodeId) callbacks.onClearSelection();
  };

  const onDownNode = ({ node, event, preventSigmaDefault }: SigmaNodeEventPayload) => {
    draggedNodeId = node;
    nodeMoved = false;
    pointerOrigin = { x: event.x, y: event.y };

    event.preventSigmaDefault();
    preventSigmaDefault();
    camera.disable();
  };

  const onMove = (event: MouseCoords) => {
    if (!draggedNodeId || !graph.hasNode(draggedNodeId)) return;
    if (!nodeMoved) {
      if (!pointerOrigin || squaredDistance(pointerOrigin, event) < DRAG_THRESHOLD_SQUARED) {
        event.preventSigmaDefault();
        return;
      }
      // Keep graph-to-viewport normalization stable once dragging actually
      // begins, otherwise auto-rescale makes the node drift from the pointer.
      if (!renderer.getCustomBBox()) renderer.setCustomBBox(renderer.getBBox());
      const position = renderer.viewportToGraph(event);
      const current = graph.getNodeAttributes(draggedNodeId);
      if (coordinatesEqual(current, position)) return;
      nodeMoved = true;
      callbacks.onDragStart(draggedNodeId);
      callbacks.onDragStateChange(true);
    }
    const position = renderer.viewportToGraph(event);
    graph.mergeNodeAttributes(draggedNodeId, {
      x: finiteCoordinate(position.x),
      y: finiteCoordinate(position.y),
    });
  };

  renderer.on("clickStage", onClickStage);
  renderer.on("downNode", onDownNode);
  mouseCaptor.on("mousemovebody", onMove);
  mouseCaptor.on("mouseup", clearDrag);

  return () => {
    renderer.off("clickStage", onClickStage);
    renderer.off("downNode", onDownNode);
    mouseCaptor.off("mousemovebody", onMove);
    mouseCaptor.off("mouseup", clearDrag);
    if (draggedNodeId) clearDrag();
    if (suppressStageClickTimer) clearTimeout(suppressStageClickTimer);
  };
}

export function fitGraphToViewport(renderer: Sigma, refreshRenderer: () => void): void {
  renderer.setCustomBBox(null);
  refreshRenderer();
  void renderer.getCamera().animatedReset({ duration: 300 });
}

function finiteCoordinate(value: number): number {
  return Number.isFinite(value) ? value : 0;
}

function squaredDistance(start: { x: number; y: number }, end: { x: number; y: number }): number {
  const dx = end.x - start.x;
  const dy = end.y - start.y;
  return dx * dx + dy * dy;
}

function coordinatesEqual(current: Record<string, unknown>, next: { x: number; y: number }): boolean {
  return current.x === next.x && current.y === next.y;
}
