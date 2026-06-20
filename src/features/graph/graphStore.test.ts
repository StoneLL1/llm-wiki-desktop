import { describe, expect, it } from "vitest";

import { useGraphStore } from "../../stores/graphStore";

describe("graphStore", () => {
  it("starts with sensible defaults", () => {
    useGraphStore.getState().reset();
    const state = useGraphStore.getState();
    expect(state.status).toBe("idle");
    expect(state.colorMode).toBe("type");
    expect(state.data).toBeNull();
    expect(state.selectedNodeId).toBeNull();
    expect(state.search).toBe("");
  });

  it("updates color mode, selection, and search synchronously", () => {
    useGraphStore.getState().reset();
    useGraphStore.getState().setColorMode("community");
    useGraphStore.getState().setSelectedNode("wiki/a.md");
    useGraphStore.getState().setSearch("agent");

    const state = useGraphStore.getState();
    expect(state.colorMode).toBe("community");
    expect(state.selectedNodeId).toBe("wiki/a.md");
    expect(state.search).toBe("agent");
  });

  it("accepts externally-injected cached data (layout reuse path)", () => {
    useGraphStore.getState().reset();
    useGraphStore.setState({
      data: {
        nodes: [{ id: "wiki/a.md", path: "wiki/a.md", label: "A", type: "concept", tags: [], starred: false, degree: 2 }],
        edges: [{ source: "wiki/a.md", target: "wiki/b.md", relation: "related", weight: 1 }],
        contentHash: "hash-1",
        builtAt: "2026-06-20T00:00:00Z",
        layout: { positions: { "wiki/a.md": [0.1, 0.2] }, communities: { "wiki/a.md": 0 } },
      },
      cached: true,
      layoutStale: false,
      status: "ready",
    });

    const state = useGraphStore.getState();
    expect(state.status).toBe("ready");
    expect(state.data?.nodes).toHaveLength(1);
    expect(state.data?.layout?.positions["wiki/a.md"]).toEqual([0.1, 0.2]);
    expect(state.cached).toBe(true);
  });
});
