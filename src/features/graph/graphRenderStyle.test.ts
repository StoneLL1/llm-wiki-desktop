import { describe, expect, it } from "vitest";

import type { GraphRenderOptions } from "./graphRenderStyle";
import {
  GRAPH_SELECTED_COLOR,
  graphSearchMatches,
  hiddenReasonForNode,
  visibleNodeIdsForExport,
  visualForNode,
} from "./graphRenderStyle";
import { COMMUNITY_PALETTE, type GraphNode } from "../../types/graph";

describe("graphRenderStyle", () => {
  const baseNode: GraphNode = {
    id: "concepts/agents.md",
    path: "concepts/agents.md",
    label: "Agent Loops",
    type: "concept",
    tags: ["workflow", "llm"],
    starred: false,
    degree: 2,
  };

  function options(overrides: Partial<GraphRenderOptions> = {}): GraphRenderOptions {
    return {
      colorMode: "type",
      selectedNodeId: null,
      hoveredNodeId: null,
      focusedNodeId: null,
      search: "",
      typeFilter: new Set(),
      degreeThreshold: 0,
      neighborIds: new Set(),
      hoveredType: null,
      ...overrides,
    };
  }

  it("matches graph search against label, path, and tags case-insensitively", () => {
    expect(graphSearchMatches(baseNode, "agent")).toBe(true);
    expect(graphSearchMatches(baseNode, "CONCEPTS/AGENTS")).toBe(true);
    expect(graphSearchMatches(baseNode, "WORKFLOW")).toBe(true);
    expect(graphSearchMatches(baseNode, "missing")).toBe(false);
  });

  it("separates type, degree, and search hidden reasons", () => {
    expect(hiddenReasonForNode(baseNode, options({ typeFilter: new Set(["entity"]) }))).toBe("type");
    expect(hiddenReasonForNode(baseNode, options({ degreeThreshold: 3 }))).toBe("degree");
    expect(hiddenReasonForNode(baseNode, options({ search: "missing" }))).toBe("search");
    expect(hiddenReasonForNode(baseNode, options({ search: "workflow" }))).toBeNull();
  });

  it("highlights the selected node and dims visible non-neighbors", () => {
    const selected = visualForNode(baseNode, options({ selectedNodeId: baseNode.id }));
    const neighbor = visualForNode(
      { ...baseNode, id: "entity/claude.md", type: "entity" },
      options({ selectedNodeId: baseNode.id, neighborIds: new Set(["entity/claude.md"]) }),
    );
    const nonNeighbor = visualForNode(
      { ...baseNode, id: "source/paper.md", type: "source" },
      options({ selectedNodeId: baseNode.id, neighborIds: new Set(["entity/claude.md"]) }),
    );

    expect(selected.highlighted).toBe(true);
    expect(selected.borderColor).toBe(GRAPH_SELECTED_COLOR);
    expect(selected.sizeDelta).toBe(2);
    expect(neighbor.opacity).toBe(1);
    expect(nonNeighbor.hidden).toBe(false);
    expect(nonNeighbor.opacity).toBeLessThan(0.2);
  });

  it("uses hovered type as a temporary highlight without hiding other visible nodes", () => {
    const matching = visualForNode(baseNode, options({ hoveredType: "concept" }));
    const other = visualForNode(
      { ...baseNode, id: "entity/claude.md", type: "entity" },
      options({ hoveredType: "concept" }),
    );

    expect(matching.hidden).toBe(false);
    expect(matching.opacity).toBe(1);
    expect(other.hidden).toBe(false);
    expect(other.opacity).toBeLessThan(1);
  });

  it("colors community mode from supplied community ids", () => {
    const visual = visualForNode(
      { ...baseNode, id: "concepts/agents.md" },
      options({
        colorMode: "community",
        communityByNodeId: new Map([["concepts/agents.md", 3]]),
      }),
    );

    expect(visual.color).toBe(COMMUNITY_PALETTE[3 % COMMUNITY_PALETTE.length]);
  });

  it("returns export-visible ids by excluding any node with a hidden reason", () => {
    const nodes: GraphNode[] = [
      baseNode,
      { ...baseNode, id: "entity/claude.md", path: "entity/claude.md", label: "Claude", type: "entity", degree: 3 },
      { ...baseNode, id: "source/paper.md", path: "source/paper.md", label: "Paper", type: "source", degree: 1 },
    ];

    expect(
      visibleNodeIdsForExport(nodes, {
        typeFilter: new Set(["concept", "source"]),
        degreeThreshold: 2,
        search: "",
      }),
    ).toEqual(new Set([baseNode.id]));
  });
});
