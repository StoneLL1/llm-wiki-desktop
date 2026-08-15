import { beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.hoisted(() => vi.fn());
const waitForTaskTerminalMock = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("../../lib/waitForTaskTerminal", () => ({ waitForTaskTerminal: waitForTaskTerminalMock }));

import { useGraphStore } from "../../stores/graphStore";
import { invalidateProjectScope } from "../../stores/projectScope";
import { useTaskStore } from "../../stores/taskStore";
import type { GraphData } from "../../types/graph";
import type { BackendTask } from "../../types/task";
import { invalidateProjectResources } from "../../stores/projectScope";

const graphData = (overrides: Partial<GraphData> = {}): GraphData => ({
  nodes: [{ id: "wiki/a.md", path: "wiki/a.md", label: "A", type: "concept", tags: [], starred: false, degree: 1 }],
  edges: [],
  contentHash: "hash-a",
  builtAt: "2026-07-04T00:00:00Z",
  layout: null,
  ...overrides,
});

const task = (overrides: Partial<BackendTask> = {}): BackendTask => ({
  id: "graph-task",
  taskType: "graph_build",
  projectId: "project-1",
  title: "Build graph",
  status: "running",
  progress: null,
  startedAt: "2026-07-04T00:00:00Z",
  updatedAt: "2026-07-04T00:00:00Z",
  completedAt: null,
  cancellable: true,
  logPath: null,
  result: null,
  error: null,
  ...overrides,
});

describe("graphStore", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    waitForTaskTerminalMock.mockReset();
    useGraphStore.getState().reset();
    useTaskStore.setState({ tasks: [], logs: {}, drawerOpen: false, selectedTaskId: null, runningCount: 0 });
    invalidateProjectScope();
  });
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
    waitForTaskTerminalMock.mockResolvedValue({ id: "graph-task", status: "succeeded", error: null });

    await useGraphStore.getState().rebuild("project-1", "D:/wiki");

    expect(invokeMock).toHaveBeenNthCalledWith(1, "build_graph", {
      request: { projectId: "project-1", projectRootPath: "D:/wiki" },
    });
    expect(invokeMock).toHaveBeenNthCalledWith(2, "get_graph", {
      request: { projectId: "project-1", projectRootPath: "D:/wiki" },
    });
    expect(useGraphStore.getState().data).toEqual(data);
  });

  it("loads graph and maps empty data to ready-empty", async () => {
    const emptyData = graphData({ nodes: [], edges: [] });
    invokeMock.mockResolvedValue({ data: emptyData, cached: true, layoutStale: false });

    await useGraphStore.getState().load("project-1", "D:/wiki");

    expect(useGraphStore.getState().status).toBe("ready-empty");
    expect(useGraphStore.getState().data).toEqual(emptyData);
  });

  it("single-flights ensures and retains presentation when the content hash is unchanged", async () => {
    const first = graphData();
    invokeMock.mockResolvedValue({ data: first, cached: true, layoutStale: false });
    await Promise.all(Array.from({ length: 20 }, () =>
      useGraphStore.getState().ensureGraph("project-1", "D:/wiki")));
    expect(invokeMock).toHaveBeenCalledTimes(1);
    useGraphStore.getState().setSelectedNode("wiki/a.md");
    useGraphStore.getState().setSearch("kept");

    await Promise.all(Array.from({ length: 20 }, () =>
      useGraphStore.getState().ensureGraph("project-1", "D:/wiki")));
    expect(invokeMock).toHaveBeenCalledTimes(1);

    invokeMock.mockResolvedValueOnce({
      data: graphData({ contentHash: first.contentHash }),
      cached: true,
      layoutStale: false,
    });
    await useGraphStore.getState().load("project-1", "D:/wiki");

    expect(useGraphStore.getState().data).toBe(first);
    expect(useGraphStore.getState().selectedNodeId).toBe("wiki/a.md");
    expect(useGraphStore.getState().search).toBe("kept");
  });

  it("drops incompatible node focus when refreshed topology changes", async () => {
    invokeMock
      .mockResolvedValueOnce({ data: graphData(), cached: true, layoutStale: false })
      .mockResolvedValueOnce({
        data: graphData({
          contentHash: "hash-b",
          nodes: [{ id: "wiki/b.md", path: "wiki/b.md", label: "B", type: "entity", tags: [], starred: false, degree: 0 }],
        }),
        cached: true,
        layoutStale: false,
      });
    await useGraphStore.getState().ensureGraph("project-1", "D:/wiki");
    useGraphStore.getState().setSelectedNode("wiki/a.md");
    invalidateProjectResources(
      { projectId: "project-1", rootPath: "D:/wiki" },
      ["graph"],
    );
    await useGraphStore.getState().ensureGraph("project-1", "D:/wiki");

    expect(useGraphStore.getState().data?.contentHash).toBe("hash-b");
    expect(useGraphStore.getState().selectedNodeId).toBeNull();
  });

  it("uses waitForTaskTerminal instead of polling get_task during rebuild", async () => {
    const started = task({ status: "running" });
    const completed = task({ status: "succeeded", completedAt: "2026-07-04T00:01:00Z" });
    const data = graphData();
    waitForTaskTerminalMock.mockResolvedValue(completed);
    invokeMock.mockImplementation((command: string) => {
      if (command === "build_graph") return Promise.resolve(started);
      if (command === "get_graph") return Promise.resolve({ data, cached: false, layoutStale: true });
      return Promise.resolve(null);
    });

    await useGraphStore.getState().rebuild("project-1", "D:/wiki");

    expect(waitForTaskTerminalMock).toHaveBeenCalledWith(started, {
      projectId: "project-1",
      projectRootPath: "D:/wiki",
    });
    expect(invokeMock).not.toHaveBeenCalledWith("get_task", expect.anything());
    expect(useTaskStore.getState().tasks).toContainEqual(completed);
  });

  it("keeps previous data visible when rebuild is cancelled", async () => {
    const existing = graphData({ contentHash: "existing" });
    useGraphStore.setState({ data: existing, status: "ready", cached: true, layoutStale: false });
    const started = task({ status: "running" });
    const cancelled = task({ status: "cancelled", completedAt: "2026-07-04T00:01:00Z" });
    invokeMock.mockResolvedValue(started);
    waitForTaskTerminalMock.mockResolvedValue(cancelled);

    await useGraphStore.getState().rebuild("project-1", "D:/wiki");

    expect(useGraphStore.getState().data).toEqual(existing);
    expect(useGraphStore.getState().status).toBe("ready");
    expect(useGraphStore.getState().error).toBe("Graph build was cancelled.");
  });

  it("keeps previous data visible in build UI state when rebuild fails", async () => {
    const existing = graphData({ contentHash: "existing" });
    useGraphStore.setState({ data: existing, status: "ready", cached: true, layoutStale: false });
    const started = task({ status: "running" });
    const failed = task({
      status: "failed",
      completedAt: "2026-07-04T00:01:00Z",
      error: {
        code: "GRAPH_BUILD_FAILED",
        message: "build failed",
        details: null,
        recoverable: true,
        userActionRequired: false,
      },
    });
    invokeMock.mockResolvedValue(started);
    waitForTaskTerminalMock.mockResolvedValue(failed);

    await useGraphStore.getState().rebuild("project-1", "D:/wiki");

    const state = useGraphStore.getState();
    expect(state.data).toEqual(existing);
    expect(state.status).toBe("ready");
    expect(state.buildUi.phase).toBe("failed");
    expect(state.buildUi.error).toContain("build failed");
  });

  it("does not start a second rebuild while one is active", async () => {
    const existing = graphData({ contentHash: "existing" });
    useGraphStore.setState({ data: existing, status: "ready", cached: true, layoutStale: false });
    const started = task({ status: "running" });
    let resolveTerminal!: (value: BackendTask) => void;
    invokeMock.mockImplementation((command: string) => {
      if (command === "build_graph") return Promise.resolve(started);
      if (command === "get_graph") return Promise.resolve({ data: existing, cached: true, layoutStale: false });
      return Promise.resolve(null);
    });
    waitForTaskTerminalMock.mockReturnValue(new Promise<BackendTask>((resolve) => {
      resolveTerminal = resolve;
    }));

    const first = useGraphStore.getState().rebuild("project-1", "D:/wiki");
    const second = useGraphStore.getState().rebuild("project-1", "D:/wiki");

    expect(invokeMock).toHaveBeenCalledTimes(1);
    resolveTerminal(task({ status: "succeeded", completedAt: "2026-07-04T00:01:00Z" }));
    await Promise.all([first, second]);
    expect(useGraphStore.getState()).toMatchObject({
      status: "ready",
      activeBuildPromise: null,
    });
    expect(useGraphStore.getState().data).toEqual(existing);
  });

  it("runs one follow-up read when invalidated repeatedly during a build", async () => {
    const existing = graphData({ contentHash: "existing" });
    useGraphStore.setState({
      data: existing,
      status: "ready",
      projectKey: "project-1\0D:/wiki",
    });
    const started = task({ status: "running" });
    let resolveTerminal!: (value: BackendTask) => void;
    invokeMock.mockImplementation((command: string) => {
      if (command === "build_graph") return Promise.resolve(started);
      if (command === "get_graph") {
        return Promise.resolve({ data: graphData({ contentHash: "after" }), cached: true, layoutStale: false });
      }
      return Promise.resolve(null);
    });
    waitForTaskTerminalMock.mockReturnValue(new Promise<BackendTask>((resolve) => {
      resolveTerminal = resolve;
    }));

    const rebuilding = useGraphStore.getState().rebuild("project-1", "D:/wiki");
    await vi.waitFor(() => expect(useGraphStore.getState().activeBuildPromise).not.toBeNull());
    invalidateProjectResources({ projectId: "project-1", rootPath: "D:/wiki" }, ["graph"], true);
    invalidateProjectResources({ projectId: "project-1", rootPath: "D:/wiki" }, ["graph"], true);
    resolveTerminal(task({ status: "succeeded", completedAt: "2026-07-04T00:01:00Z" }));
    await rebuilding;
    await vi.waitFor(() => expect(useGraphStore.getState().data?.contentHash).toBe("after"));

    expect(invokeMock.mock.calls.filter(([command]) => command === "build_graph")).toHaveLength(1);
    expect(invokeMock.mock.calls.filter(([command]) => command === "get_graph")).toHaveLength(2);
  });

  it("clears a dirty build follow-up when the project store resets", async () => {
    const terminalResolvers: Array<(value: BackendTask) => void> = [];
    waitForTaskTerminalMock.mockImplementation(() => new Promise<BackendTask>((resolve) => {
      terminalResolvers.push(resolve);
    }));
    invokeMock.mockImplementation((command: string) => {
      if (command === "build_graph") return Promise.resolve(task());
      if (command === "get_graph") {
        return Promise.resolve({ data: graphData(), cached: true, layoutStale: false });
      }
      return Promise.resolve(null);
    });

    const projectA = useGraphStore.getState().rebuild("project-a", "D:/a");
    await vi.waitFor(() => expect(terminalResolvers).toHaveLength(1));
    invalidateProjectResources({ projectId: "project-a", rootPath: "D:/a" }, ["graph"]);
    useGraphStore.getState().reset();
    terminalResolvers[0]!(task({ status: "succeeded" }));
    await projectA;

    const projectB = useGraphStore.getState().rebuild("project-b", "D:/b");
    await vi.waitFor(() => expect(terminalResolvers).toHaveLength(2));
    terminalResolvers[1]!(task({ status: "succeeded", projectId: "project-b" }));
    await projectB;

    expect(invokeMock.mock.calls.filter(([command]) => command === "get_graph")).toHaveLength(1);
  });

  it("ignores terminal task results from a previous project scope", async () => {
    const existing = graphData({ contentHash: "project-a" });
    const projectB = graphData({ contentHash: "project-b", nodes: [{ id: "wiki/b.md", path: "wiki/b.md", label: "B", type: "entity", tags: [], starred: false, degree: 0 }] });
    useGraphStore.setState({ data: existing, status: "ready" });
    const started = task({ id: "task-a", status: "running" });
    let resolveTerminal!: (value: BackendTask) => void;
    waitForTaskTerminalMock.mockReturnValue(new Promise<BackendTask>((resolve) => {
      resolveTerminal = resolve;
    }));
    invokeMock.mockImplementation((command: string, args: { request?: { projectId?: string } }) => {
      if (command === "build_graph") return Promise.resolve(started);
      if (command === "get_graph" && args.request?.projectId === "project-b") {
        return Promise.resolve({ data: projectB, cached: true, layoutStale: false });
      }
      if (command === "get_graph") {
        return Promise.resolve({ data: existing, cached: true, layoutStale: false });
      }
      return Promise.resolve(null);
    });

    const rebuild = useGraphStore.getState().rebuild("project-a", "D:/wiki-a");
    invalidateProjectScope();
    await useGraphStore.getState().load("project-b", "D:/wiki-b");
    resolveTerminal(task({ id: "task-a", status: "succeeded", completedAt: "2026-07-04T00:01:00Z" }));
    await rebuild;

    expect(useGraphStore.getState().data).toEqual(projectB);
    expect(useGraphStore.getState().status).toBe("ready");
  });
});
