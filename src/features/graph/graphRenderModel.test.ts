import { describe, expect, it } from "vitest";

import { COMMUNITY_PALETTE, PAGE_TYPE_COLORS, type GraphData, type GraphNode } from "../../types/graph";
import type { WikiPageType } from "../../types/wiki";
import { buildRenderSnapshot, type RenderSnapshotInput } from "./graphRenderModel";
import { GRAPH_SELECTED_COLOR, visualForEdge, visualForNode } from "./graphRenderStyle";

// The render snapshot's job is to compute, ONCE per refresh, the things that
// the per-edge/per-node sigma reducers used to recompute on every call:
//   - GraphRenderOptions (incl. communityByNodeId Map)
//   - hiddenNodeIds Set (scan of all nodes against type/degree/search filters)
// Reducers then read the snapshot. This test file pins that contract and the
// visual semantics that must stay identical to the pre-snapshot code path.

function makeNode(overrides: Partial<GraphNode> = {}): GraphNode {
  return {
    id: "concepts/agents.md",
    path: "concepts/agents.md",
    label: "Agent Loops",
    type: "concept",
    tags: ["workflow", "llm"],
    starred: false,
    degree: 2,
    ...overrides,
  };
}

function makeData(nodes: GraphNode[], overrides: Partial<GraphData> = {}): GraphData {
  return {
    nodes,
    edges: [],
    contentHash: "hash-1",
    builtAt: "2026-07-04T00:00:00Z",
    layout: null,
    ...overrides,
  };
}

function makeInput(overrides: Partial<RenderSnapshotInput> = {}): RenderSnapshotInput {
  return {
    colorMode: "type",
    selectedNodeId: null,
    hoveredNodeId: null,
    focusedNodeId: null,
    search: "",
    typeFilter: new Set<WikiPageType>(),
    degreeThreshold: 0,
    neighborIds: new Set<string>(),
    hoveredType: null,
    ...overrides,
  };
}

describe("buildRenderSnapshot", () => {
  it("builds communityByNodeId once from layout.communities", () => {
    const data = makeData(
      [makeNode({ id: "a" }), makeNode({ id: "b" }), makeNode({ id: "c" })],
      {
        layout: {
          positions: { a: [0, 0], b: [1, 1], c: [2, 2] },
          communities: { a: 0, b: 3, c: 7 },
        },
      },
    );
    const snap = buildRenderSnapshot(data, makeInput({ colorMode: "community" }));

    expect(snap.options.communityByNodeId).toBeInstanceOf(Map);
    expect(snap.options.communityByNodeId?.get("a")).toBe(0);
    expect(snap.options.communityByNodeId?.get("b")).toBe(3);
    expect(snap.options.communityByNodeId?.get("c")).toBe(7);
  });

  it("returns an empty community map when layout is null", () => {
    const data = makeData([makeNode({ id: "a" })], { layout: null });
    const snap = buildRenderSnapshot(data, makeInput({ colorMode: "community" }));

    expect(snap.options.communityByNodeId).toBeInstanceOf(Map);
    expect(snap.options.communityByNodeId?.size).toBe(0);
  });

  it("computes hiddenNodeIds once by scanning nodes against type/degree/search filters", () => {
    const nodes = [
      makeNode({ id: "concept-a", type: "concept", degree: 5, tags: ["workflow"] }),
      makeNode({ id: "entity-b", type: "entity", degree: 1, tags: ["workflow"] }),
      makeNode({ id: "source-c", type: "source", degree: 10, label: "Paper", tags: [] }),
    ];
    const data = makeData(nodes);

    // typeFilter holds HIDDEN types (store semantics: unchecked legend types).
    // Hide "entity" → entity-b drops out, concept-a and source-c stay visible.
    const snap = buildRenderSnapshot(
      data,
      makeInput({ typeFilter: new Set<WikiPageType>(["entity"]) }),
    );

    expect(snap.hiddenNodeIds).toBeInstanceOf(Set);
    expect(snap.hiddenNodeIds.has("entity-b")).toBe(true);
    expect(snap.hiddenNodeIds.has("concept-a")).toBe(false);
    expect(snap.hiddenNodeIds.has("source-c")).toBe(false);
  });

  it("hides nodes below the degree threshold", () => {
    const nodes = [
      makeNode({ id: "hi", degree: 10 }),
      makeNode({ id: "lo", degree: 1 }),
    ];
    const data = makeData(nodes);
    const snap = buildRenderSnapshot(data, makeInput({ degreeThreshold: 3 }));

    expect(snap.hiddenNodeIds.has("lo")).toBe(true);
    expect(snap.hiddenNodeIds.has("hi")).toBe(false);
  });

  it("hides nodes that do not match the search query", () => {
    const nodes = [
      makeNode({ id: "a", path: "wiki/agents.md", label: "Agent Loops", tags: ["workflow"] }),
      makeNode({ id: "b", path: "wiki/transformer.md", label: "Transformer", tags: ["attention"] }),
    ];
    const data = makeData(nodes);
    // "agent" matches node a's label/path/tags; node b has no "agent" anywhere.
    const snap = buildRenderSnapshot(data, makeInput({ search: "agent" }));

    expect(snap.hiddenNodeIds.has("a")).toBe(false);
    expect(snap.hiddenNodeIds.has("b")).toBe(true);
  });

  it("produces an empty hidden set when no filter is active", () => {
    const data = makeData([makeNode({ id: "a" }), makeNode({ id: "b", degree: 0 })]);
    const snap = buildRenderSnapshot(data, makeInput());

    expect(snap.hiddenNodeIds.size).toBe(0);
  });

  it("exposes options whose visualForNode output matches the prior per-call path", () => {
    // Pin the parity contract: a node visual computed from snapshot.options
    // must equal the visual computed from an equivalent options object built
    // the old way. This is the regression guard that proves the snapshot does
    // not change rendering semantics.
    const node = makeNode({ id: "concepts/agents.md", type: "concept" });
    const data = makeData([node], {
      layout: { positions: { "concepts/agents.md": [0, 0] }, communities: { "concepts/agents.md": 2 } },
    });
    const snap = buildRenderSnapshot(
      data,
      makeInput({ colorMode: "community", selectedNodeId: "concepts/agents.md" }),
    );

    const visual = visualForNode(node, snap.options);

    expect(visual.hidden).toBe(false);
    expect(visual.highlighted).toBe(true);
    expect(visual.borderColor).toBe(GRAPH_SELECTED_COLOR);
    expect(visual.color).toBe(COMMUNITY_PALETTE[2 % COMMUNITY_PALETTE.length]);
  });

  it("colors type-mode nodes with PAGE_TYPE_COLORS via snapshot.options", () => {
    const node = makeNode({ id: "entity/x.md", type: "entity" });
    const data = makeData([node]);
    const snap = buildRenderSnapshot(data, makeInput({ colorMode: "type" }));

    const visual = visualForNode(node, snap.options);

    expect(visual.color).toBe(PAGE_TYPE_COLORS.entity);
  });

  it("feeds the precomputed hidden set into edge visibility (edge touching a hidden node is hidden)", () => {
    const nodes = [
      makeNode({ id: "visible", type: "concept", degree: 5 }),
      makeNode({ id: "hidden-by-degree", type: "concept", degree: 0 }),
    ];
    const data = makeData(nodes, {
      edges: [
        { source: "visible", target: "hidden-by-degree", relation: "related", weight: 1 },
      ],
    });
    const snap = buildRenderSnapshot(data, makeInput({ degreeThreshold: 1 }));

    const edge = data.edges[0];
    const visual = visualForEdge(edge, snap.options, snap.hiddenNodeIds);

    expect(visual.hidden).toBe(true);
  });

  it("keeps an edge visible when both endpoints survive the filters", () => {
    const nodes = [
      makeNode({ id: "a", type: "concept", degree: 5 }),
      makeNode({ id: "b", type: "concept", degree: 5 }),
    ];
    const data = makeData(nodes, {
      edges: [{ source: "a", target: "b", relation: "related", weight: 1 }],
    });
    const snap = buildRenderSnapshot(data, makeInput());

    const edge = data.edges[0];
    const visual = visualForEdge(edge, snap.options, snap.hiddenNodeIds);

    expect(visual.hidden).toBe(false);
  });

  it("dims non-neighbor visible nodes when a node is selected", () => {
    const nodes = [
      makeNode({ id: "sel", type: "concept", degree: 5 }),
      makeNode({ id: "neighbor", type: "concept", degree: 5 }),
      makeNode({ id: "far", type: "concept", degree: 5 }),
    ];
    const data = makeData(nodes, {
      edges: [
        { source: "sel", target: "neighbor", relation: "related", weight: 1 },
      ],
    });
    const snap = buildRenderSnapshot(
      data,
      makeInput({ selectedNodeId: "sel", neighborIds: new Set(["neighbor"]) }),
    );

    const selVisual = visualForNode(nodes[0], snap.options);
    const neighborVisual = visualForNode(nodes[1], snap.options);
    const farVisual = visualForNode(nodes[2], snap.options);

    expect(selVisual.highlighted).toBe(true);
    expect(neighborVisual.highlighted).toBe(true);
    expect(farVisual.opacity).toBeLessThan(0.2);
    expect(farVisual.highlighted).toBe(false);
  });

  it("highlights the edge between the selected node and a neighbor", () => {
    const nodes = [
      makeNode({ id: "sel", type: "concept", degree: 5 }),
      makeNode({ id: "neighbor", type: "concept", degree: 5 }),
      makeNode({ id: "far", type: "concept", degree: 5 }),
    ];
    const data = makeData(nodes, {
      edges: [
        { source: "sel", target: "neighbor", relation: "related", weight: 1 },
        { source: "neighbor", target: "far", relation: "related", weight: 1 },
      ],
    });
    const snap = buildRenderSnapshot(
      data,
      makeInput({ selectedNodeId: "sel", neighborIds: new Set(["neighbor"]) }),
    );

    const highlightEdge = visualForEdge(data.edges[0], snap.options, snap.hiddenNodeIds);
    const dimEdge = visualForEdge(data.edges[1], snap.options, snap.hiddenNodeIds);

    expect(highlightEdge.color).toBe(GRAPH_SELECTED_COLOR);
    expect(highlightEdge.opacity).toBe(1);
    expect(dimEdge.opacity).toBeLessThan(0.2);
  });

  it("applies hoveredType dimming to non-matching visible nodes without hiding them", () => {
    const nodes = [
      makeNode({ id: "concept-a", type: "concept", degree: 5 }),
      makeNode({ id: "entity-b", type: "entity", degree: 5 }),
    ];
    const data = makeData(nodes);
    const snap = buildRenderSnapshot(data, makeInput({ hoveredType: "concept" }));

    const matching = visualForNode(nodes[0], snap.options);
    const other = visualForNode(nodes[1], snap.options);

    expect(matching.hidden).toBe(false);
    expect(matching.opacity).toBe(1);
    expect(other.hidden).toBe(false);
    expect(other.opacity).toBeLessThan(1);
  });

  it("highlights the focused node with sizeDelta 1 and dims non-neighbors (parity with selectedNodeId)", () => {
    // Focus shares the `hasFocusRoot` branch with selection; pin it separately
    // so a future regression that breaks one but not the other is caught.
    const nodes = [
      makeNode({ id: "focus", type: "concept", degree: 5 }),
      makeNode({ id: "neighbor", type: "concept", degree: 5 }),
      makeNode({ id: "far", type: "concept", degree: 5 }),
    ];
    const data = makeData(nodes, {
      edges: [{ source: "focus", target: "neighbor", relation: "related", weight: 1 }],
    });
    const snap = buildRenderSnapshot(
      data,
      makeInput({ focusedNodeId: "focus", neighborIds: new Set(["neighbor"]) }),
    );

    const focusVisual = visualForNode(nodes[0], snap.options);
    const neighborVisual = visualForNode(nodes[1], snap.options);
    const farVisual = visualForNode(nodes[2], snap.options);

    expect(focusVisual.highlighted).toBe(true);
    expect(focusVisual.sizeDelta).toBe(1); // focused: 1 (selected: 2)
    expect(focusVisual.forceLabel).toBe(true);
    expect(neighborVisual.highlighted).toBe(true);
    expect(farVisual.highlighted).toBe(false);
    expect(farVisual.opacity).toBeLessThan(0.2);

    const edgeVisual = visualForEdge(data.edges[0], snap.options, snap.hiddenNodeIds);
    expect(edgeVisual.color).toBe(GRAPH_SELECTED_COLOR);
    expect(edgeVisual.opacity).toBe(1);
  });

  it("sets forceLabel on a node that matches the search query", () => {
    const nodes = [
      makeNode({ id: "a", path: "wiki/agents.md", label: "Agent Loops", tags: ["workflow"] }),
      makeNode({ id: "b", path: "wiki/other.md", label: "Other", tags: [] }),
    ];
    const data = makeData(nodes);
    const snap = buildRenderSnapshot(data, makeInput({ search: "agent" }));

    const hitVisual = visualForNode(nodes[0], snap.options);
    const missVisual = visualForNode(nodes[1], snap.options);

    expect(hitVisual.forceLabel).toBe(true);
    expect(missVisual.forceLabel).toBe(false);
    // search-miss is hidden, not just dimmed.
    expect(snap.hiddenNodeIds.has("b")).toBe(true);
  });

  it("returns full-opacity edges when no selection/focus/hover is active", () => {
    const nodes = [
      makeNode({ id: "a", type: "concept", degree: 5 }),
      makeNode({ id: "b", type: "concept", degree: 5 }),
    ];
    const data = makeData(nodes, {
      edges: [{ source: "a", target: "b", relation: "related", weight: 1 }],
    });
    const snap = buildRenderSnapshot(data, makeInput());

    const visual = visualForEdge(data.edges[0], snap.options, snap.hiddenNodeIds);

    expect(visual.hidden).toBe(false);
    expect(visual.opacity).toBe(1);
    expect(visual.color).toBe("#d4d4d4");
  });

  it("recomputes a fresh snapshot when filters change (snapshot is not memoized across calls)", () => {
    const nodes = [
      makeNode({ id: "a", type: "concept", degree: 5 }),
      makeNode({ id: "b", type: "entity", degree: 5 }),
    ];
    const data = makeData(nodes);

    const before = buildRenderSnapshot(data, makeInput());
    // typeFilter holds HIDDEN types (store semantics). Hide "entity" → b drops.
    const after = buildRenderSnapshot(
      data,
      makeInput({ typeFilter: new Set<WikiPageType>(["entity"]) }),
    );

    expect(before.hiddenNodeIds.size).toBe(0);
    expect(after.hiddenNodeIds.has("b")).toBe(true);
    expect(after.hiddenNodeIds.has("a")).toBe(false);
    // community map identity differs per call (rebuilt from layout each time).
    expect(after.options.communityByNodeId).not.toBe(before.options.communityByNodeId);
  });

  it("handles an empty graph (no nodes) without throwing", () => {
    const data = makeData([]);
    const snap = buildRenderSnapshot(data, makeInput());

    expect(snap.hiddenNodeIds.size).toBe(0);
    expect(snap.options.communityByNodeId?.size).toBe(0);
  });

  it("propagates colorMode/selectedNodeId/focusedNodeId/hoveredNodeId/search into options verbatim", () => {
    const data = makeData([makeNode({ id: "a" })]);
    const neighborIds = new Set(["a"]);
    const snap = buildRenderSnapshot(
      data,
      makeInput({
        colorMode: "plain",
        selectedNodeId: "a",
        focusedNodeId: "b",
        hoveredNodeId: "c",
        search: "agent",
        neighborIds,
        hoveredType: "concept",
        degreeThreshold: 2,
      }),
    );

    expect(snap.options.colorMode).toBe("plain");
    expect(snap.options.selectedNodeId).toBe("a");
    expect(snap.options.focusedNodeId).toBe("b");
    expect(snap.options.hoveredNodeId).toBe("c");
    expect(snap.options.search).toBe("agent");
    expect(snap.options.neighborIds).toBe(neighborIds);
    expect(snap.options.hoveredType).toBe("concept");
    expect(snap.options.degreeThreshold).toBe(2);
  });
});
