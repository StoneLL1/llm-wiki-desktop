import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import "../../i18n";
import { useGraphStore } from "../../stores/graphStore";
import { useNavigationStore } from "../../stores/navigationStore";
import { useProjectStore } from "../../stores/projectStore";
import { useTaskStore } from "../../stores/taskStore";
import type { ProjectSummary } from "../../types/project";
import type { BackendTask } from "../../types/task";
import { DashboardView } from "./DashboardView";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

describe("DashboardView", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    useGraphStore.getState().reset();
    useNavigationStore.getState().setActiveView("dashboard");
    useProjectStore.getState().setCurrentProject(project({ wikiPageCount: 42, graphState: "stale" }));
    useTaskStore.getState().setTasks([]);
  });

  it("renders graph preview counts from graph store data", () => {
    useGraphStore.setState({
      status: "ready",
      data: {
        nodes: [
          { id: "a", path: "wiki/a.md", label: "Alpha", type: "concept", tags: [], starred: false, degree: 1 },
          { id: "b", path: "wiki/b.md", label: "Beta", type: "entity", tags: [], starred: false, degree: 1 },
        ],
        edges: [{ source: "a", target: "b", relation: "related", weight: 1 }],
        contentHash: "hash",
        builtAt: "2026-07-04T00:00:00Z",
        layout: null,
      },
    });

    render(<DashboardView />);

    const section = screen.getByRole("region", { name: "Graph overview" });
    expect(section).toHaveTextContent("2 nodes");
    expect(section).toHaveTextContent("1 edges");
  });

  it("opens graph view from the dashboard graph panel", () => {
    render(<DashboardView />);

    fireEvent.click(screen.getByRole("button", { name: "Open Graph" }));

    expect(useNavigationStore.getState().activeView).toBe("graph");
  });

  it("shows graph build task state without invoking build_graph", () => {
    useTaskStore.getState().setTasks([
      task({ id: "graph-build", title: "Building graph preview", taskType: "graph_build", status: "running" }),
    ]);

    render(<DashboardView />);

    expect(screen.getAllByText("Building graph preview").length).toBeGreaterThan(0);
    expect(invokeMock).not.toHaveBeenCalledWith("build_graph", expect.anything());
  });

  it("surfaces graph store status in the graph overview", () => {
    useGraphStore.setState({ status: "rebuilding" });

    render(<DashboardView />);

    const section = screen.getByRole("region", { name: "Graph overview" });
    expect(section).toHaveTextContent("Rebuilding graph");
  });
});

function project(overrides: Partial<ProjectSummary> = {}): ProjectSummary {
  return {
    projectId: "sample",
    name: "Sample",
    rootPath: "D:/wiki",
    template: "general",
    wikiPageCount: 0,
    sourceCount: 0,
    taskCount: 0,
    indexState: "indexed",
    graphState: "missing",
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

function task(overrides: Partial<BackendTask> = {}): BackendTask {
  return {
    id: "task",
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
