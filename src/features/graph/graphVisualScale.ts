export const GRAPH_VISUAL_SCALE = {
  minNodeSize: 2.4,
  maxNodeSize: 6.6,
  nodeDegreeFactor: 0.9,
  minEdgeSize: 0.42,
  minRenderedEdgeThickness: 0.42,
  maxEdgeSize: 1.15,
  edgeWeightFactor: 0.2,
  selectedSizeDelta: 1.1,
  hoveredSizeDelta: 0.65,
  highlightedEdgeSize: 1.2,
} as const;

export function nodeSizeForDegree(degree: number): number {
  const safeDegree = Number.isFinite(degree) ? Math.max(0, degree) : 0;
  return Math.min(
    GRAPH_VISUAL_SCALE.maxNodeSize,
    GRAPH_VISUAL_SCALE.minNodeSize + Math.sqrt(safeDegree) * GRAPH_VISUAL_SCALE.nodeDegreeFactor,
  );
}

export function edgeSizeForWeight(weight: number): number {
  const safeWeight = Number.isFinite(weight) ? Math.max(0, weight) : 0;
  return Math.min(
    GRAPH_VISUAL_SCALE.maxEdgeSize,
    GRAPH_VISUAL_SCALE.minEdgeSize + Math.sqrt(safeWeight) * GRAPH_VISUAL_SCALE.edgeWeightFactor,
  );
}
