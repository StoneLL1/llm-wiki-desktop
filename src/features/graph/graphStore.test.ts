import { beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

import { useGraphStore } from "../../stores/graphStore";

describe("graphStore", () => {
  beforeEach(() => invokeMock.mockReset());
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

  it("toggles type filters and clamps the degree threshold", () => {
    useGraphStore.getState().reset();
    expect(useGraphStore.getState().typeFilter.size).toBe(0);
    useGraphStore.getState().toggleTypeFilter("concept");
    expect(useGraphStore.getState().typeFilter.has("concept")).toBe(true);
    // Toggle off again returns to an empty filter set.
    useGraphStore.getState().toggleTypeFilter("concept");
    expect(useGraphStore.getState().typeFilter.has("concept")).toBe(false);

    useGraphStore.getState().setDegreeThreshold(5);
    expect(useGraphStore.getState().degreeThreshold).toBe(5);
    // Negative + fractional inputs are clamped to a non-negative integer.
    useGraphStore.getState().setDegreeThreshold(-3);
    expect(useGraphStore.getState().degreeThreshold).toBe(0);
    useGraphStore.getState().setDegreeThreshold(4.9);
    expect(useGraphStore.getState().degreeThreshold).toBe(4);
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

  it("runs a rebuild as a background task before loading its cache", async () => {
    const data = {
      nodes: [], edges: [], contentHash: "new", builtAt: "2026-06-21T00:00:00Z", layout: null,
    };
    invokeMock.mockImplementation((command: string) => {
      if (command === "build_graph") {
        return Promise.resolve({ id: "graph-task", status: "succeeded", error: null });
      }
      if (command === "get_graph") {
        return Promise.resolve({ data, cached: true, layoutStale: true });
      }
      return Promise.resolve(null);
    });

    await useGraphStore.getState().rebuild("project-1", "D:/wiki");

    expect(invokeMock).toHaveBeenNthCalledWith(1, "build_graph", {
      request: { projectId: "project-1", projectRootPath: "D:/wiki" },
    });
    expect(invokeMock).toHaveBeenNthCalledWith(2, "get_graph", {
      request: { projectId: "project-1", projectRootPath: "D:/wiki" },
    });
    expect(useGraphStore.getState().data).toEqual(data);
  });
});
