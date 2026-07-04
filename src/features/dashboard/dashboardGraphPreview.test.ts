import { describe, expect, it } from "vitest";

import type { GraphData } from "../../types/graph";
import type { ProjectSummary } from "../../types/project";
import type { BackendTask } from "../../types/task";
import type { WikiTree } from "../../types/wiki";
import { buildDashboardGraphPreview } from "./dashboardGraphPreview";

describe("dashboardGraphPreview", () => {
  it("prefers loaded graph data over project summary counts", () => {
    const model = buildDashboardGraphPreview(project({ wikiPageCount: 99, graphState: "stale" }), graphData(), "ready", [], null);

    expect(model.nodeCount).toBe(3);
    expect(model.edgeCount).toBe(2);
    expect(model.graphState).toBe("cached");
    expect(model.previewNodes).toHaveLength(3);
  });

  it("falls back to project summary before graph view has loaded", () => {
    const model = buildDashboardGraphPreview(project({ wikiPageCount: 42, graphState: "missing" }), null, "idle", [], null);

    expect(model.nodeCount).toBe(42);
    expect(model.edgeCount).toBe(0);
    expect(model.pageCount).toBe(42);
    expect(model.graphState).toBe("missing");
  });

  it("reports active graph task without starting a task", () => {
    const model = buildDashboardGraphPreview(
      project(),
      null,
      "rebuilding",
      [task({ title: "Building graph cache", taskType: "graph_build", status: "running" })],
      null,
    );

    expect(model.activeTaskLabel).toBe("Building graph cache");
  });

  it("computes top page types from wiki tree pages", () => {
    const model = buildDashboardGraphPreview(project(), null, "idle", [], tree());

    expect(model.topTypes).toEqual([
      { type: "concept", count: 2 },
      { type: "source", count: 1 },
    ]);
  });

  it("returns deterministic mini preview coordinates", () => {
    const first = buildDashboardGraphPreview(project(), graphData(), "ready", [], null);
    const second = buildDashboardGraphPreview(project(), graphData(), "ready", [], null);

    expect(first.previewNodes).toEqual(second.previewNodes);
    expect(first.previewNodes.every((node) => node.x >= 0 && node.x <= 120)).toBe(true);
    expect(first.previewNodes.every((node) => node.y >= 0 && node.y <= 72)).toBe(true);
  });
});

function project(overrides: Partial<ProjectSummary> = {}): ProjectSummary {
  return {
    projectId: "sample",
    name: "Sample",
    rootPath: "D:/wiki",
    template: "general",
    wikiPageCount: 3,
    sourceCount: 0,
    taskCount: 0,
    indexState: "indexed",
    graphState: "cached",
    agentRoute: "agent",
    health: {
      isWikiProject: true,
      hasPurpose: true,
      hasSchema: true,
      hasAppState: true,
      hasObsidian: false,
      missingPaths: [],
    },
    ...overrides,
  };
}

function graphData(): GraphData {
  return {
    nodes: [
      { id: "a", path: "wiki/a.md", label: "Alpha", type: "concept", tags: [], starred: false, degree: 2 },
      { id: "b", path: "wiki/b.md", label: "Beta", type: "entity", tags: [], starred: false, degree: 1 },
      { id: "c", path: "wiki/c.md", label: "Gamma", type: "source", tags: [], starred: false, degree: 1 },
    ],
    edges: [
      { source: "a", target: "b", relation: "related", weight: 1 },
      { source: "a", target: "c", relation: "related", weight: 1 },
    ],
    contentHash: "hash",
    builtAt: "2026-07-04T00:00:00Z",
    layout: null,
  };
}

function task(overrides: Partial<BackendTask> = {}): BackendTask {
  return {
    id: "task-graph",
    taskType: "graph_build",
    projectId: "sample",
    title: "Task",
    status: "running",
    progress: null,
    startedAt: "2026-07-04T00:00:00Z",
    updatedAt: "2026-07-04T00:01:00Z",
    completedAt: null,
    cancellable: true,
    logPath: null,
    result: null,
    error: null,
    ...overrides,
  };
}

function tree(): WikiTree {
  return {
    root: { name: "wiki", kind: "folder", path: "wiki", starred: false, bookmarked: false, fileCount: 3, children: [] },
    totalPages: 3,
    pages: [
      page("wiki/a.md", "concept"),
      page("wiki/b.md", "concept"),
      page("wiki/c.md", "source"),
    ],
  };
}

function page(path: string, pageType: "concept" | "source") {
  return {
    path,
    title: path,
    pageType,
    tags: [],
    sources: [],
    aliases: [],
    created: null,
    updated: null,
    starred: false,
    bookmarked: false,
    wordCount: 1,
    fileSize: 1,
    modifiedTime: "2026-07-04T00:00:00Z",
    hash: path,
    wikilinks: [],
  };
}
