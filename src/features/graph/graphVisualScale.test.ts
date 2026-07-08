import { describe, expect, it } from "vitest";

import {
  edgeSizeForWeight,
  GRAPH_VISUAL_SCALE,
  nodeSizeForDegree,
} from "./graphVisualScale";

describe("graphVisualScale", () => {
  it("keeps low-degree wiki nodes compact while preserving degree ranking", () => {
    expect(nodeSizeForDegree(0)).toBeCloseTo(2.4);
    expect(nodeSizeForDegree(1)).toBeLessThan(3.6);
    expect(nodeSizeForDegree(8)).toBeGreaterThan(nodeSizeForDegree(1));
    expect(nodeSizeForDegree(80)).toBeLessThanOrEqual(6.6);
  });

  it("keeps default weighted edges visible without overpowering nodes", () => {
    expect(edgeSizeForWeight(0)).toBeCloseTo(0.42);
    expect(edgeSizeForWeight(1)).toBeGreaterThanOrEqual(0.6);
    expect(edgeSizeForWeight(1)).toBeLessThan(0.75);
    expect(edgeSizeForWeight(10)).toBeGreaterThan(edgeSizeForWeight(1));
    expect(edgeSizeForWeight(50)).toBeLessThanOrEqual(1.15);
    expect(GRAPH_VISUAL_SCALE.minRenderedEdgeThickness).toBe(GRAPH_VISUAL_SCALE.minEdgeSize);
  });

  it("keeps focus growth modest instead of making selected circles balloon", () => {
    expect(GRAPH_VISUAL_SCALE.selectedSizeDelta).toBeLessThanOrEqual(1.2);
    expect(GRAPH_VISUAL_SCALE.hoveredSizeDelta).toBeLessThanOrEqual(0.7);
    expect(GRAPH_VISUAL_SCALE.highlightedEdgeSize).toBeLessThanOrEqual(1.25);
  });

  it("falls back to minimum sizes for malformed numeric inputs", () => {
    expect(nodeSizeForDegree(Number.NaN)).toBe(GRAPH_VISUAL_SCALE.minNodeSize);
    expect(nodeSizeForDegree(Number.POSITIVE_INFINITY)).toBe(GRAPH_VISUAL_SCALE.minNodeSize);
    expect(edgeSizeForWeight(Number.NaN)).toBe(GRAPH_VISUAL_SCALE.minEdgeSize);
    expect(edgeSizeForWeight(Number.NEGATIVE_INFINITY)).toBe(GRAPH_VISUAL_SCALE.minEdgeSize);
  });
});
