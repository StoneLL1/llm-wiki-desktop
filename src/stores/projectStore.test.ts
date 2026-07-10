import { beforeEach, describe, expect, it, vi } from "vitest";

import type { OpenProjectResponse, ProjectSummary, RecentProject } from "../types/project";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

import { defaultProject, useProjectStore } from "./projectStore";
import { captureProjectScope, isProjectScopeCurrent } from "./projectScope";

const recent: RecentProject = {
  projectId: "project-a",
  name: "Project A",
  rootPath: "D:/知识库/project-a",
  template: "general",
  openedAt: "2026-06-21T00:00:00Z",
  wikiPageCount: 2,
  sourceCount: 1,
  taskCount: 0,
  indexState: "indexed",
  graphState: "cached",
  missing: false,
};

const summary: ProjectSummary = {
  projectId: recent.projectId,
  name: recent.name,
  rootPath: recent.rootPath,
  template: recent.template,
  wikiPageCount: 2,
  sourceCount: 1,
  taskCount: 0,
  indexState: "indexed",
  graphState: "cached",
  agentRoute: "unconfigured",
  health: {
    isWikiProject: true,
    hasPurpose: true,
    hasSchema: true,
    hasAppState: true,
    hasObsidian: false,
    missingPaths: [],
  },
};

beforeEach(() => {
  invokeMock.mockReset();
  Object.defineProperty(window, "__TAURI_INTERNALS__", {
    value: {},
    configurable: true,
  });
  useProjectStore.setState({
    currentProject: defaultProject,
    recentProjects: [],
    pendingAction: undefined,
    initializing: false,
    initialized: false,
    error: null,
  });
});

describe("projectStore bootstrap", () => {
  it("ignores an agent route update for a project that is no longer active", () => {
    const projectA = summary;
    const projectB = {
      ...summary,
      projectId: "project-b",
      name: "Project B",
      rootPath: "D:/知识库/project-b",
    };

    useProjectStore.getState().setCurrentProject(projectB);
    useProjectStore
      .getState()
      .setAgentRoute(projectA.projectId, projectA.rootPath, "agent");

    expect(useProjectStore.getState().currentProject).toEqual(projectB);
  });

  it("does not invalidate in-flight work for a metadata-only update to the same project", () => {
    useProjectStore.getState().setCurrentProject(summary);
    const scope = captureProjectScope();

    useProjectStore.getState().setCurrentProject({ ...summary, agentRoute: "agent" });

    expect(isProjectScopeCurrent(scope)).toBe(true);
  });

  it("opens the most recent project so the backend registers its context before rendering it", async () => {
    const opened: OpenProjectResponse = { kind: "opened", summary };
    invokeMock.mockResolvedValueOnce([recent]).mockResolvedValueOnce(opened);

    await useProjectStore.getState().bootstrap();

    expect(invokeMock.mock.calls).toEqual([
      ["list_recent_projects"],
      ["open_project", { request: { path: recent.rootPath } }],
    ]);
    expect(useProjectStore.getState().currentProject).toEqual(summary);
    expect(useProjectStore.getState().initialized).toBe(true);
  });

  it("skips missing recents during automatic bootstrap", async () => {
    const missing: RecentProject = {
      ...recent,
      projectId: "missing-project",
      name: "Missing Project",
      rootPath: "D:/知识库/missing-project",
      wikiPageCount: 0,
      sourceCount: 0,
      indexState: "missing",
      graphState: "missing",
      missing: true,
    };
    const opened: OpenProjectResponse = { kind: "opened", summary };
    invokeMock.mockResolvedValueOnce([missing, recent]).mockResolvedValueOnce(opened);

    await useProjectStore.getState().bootstrap();

    expect(invokeMock.mock.calls).toEqual([
      ["list_recent_projects"],
      ["open_project", { request: { path: recent.rootPath } }],
    ]);
    expect(useProjectStore.getState().currentProject).toEqual(summary);
    expect(useProjectStore.getState().error).toBeNull();
  });

  it("never lets a delayed automatic reopen overwrite an explicit project selection", async () => {
    let resolveAutomatic!: (value: OpenProjectResponse) => void;
    const automatic = new Promise<OpenProjectResponse>((resolve) => {
      resolveAutomatic = resolve;
    });
    const selected = {
      ...summary,
      projectId: "project-b",
      name: "Project B",
      rootPath: "D:/知识库/project-b",
    };
    invokeMock.mockImplementation((command: string, args?: { request?: { path?: string } }) => {
      if (command === "list_recent_projects") return Promise.resolve([recent]);
      if (command === "open_project" && args?.request?.path === recent.rootPath) return automatic;
      if (command === "open_project" && args?.request?.path === selected.rootPath) {
        return Promise.resolve({ kind: "opened", summary: selected });
      }
      return Promise.reject(new Error(`Unexpected command: ${command}`));
    });

    const bootstrapping = useProjectStore.getState().bootstrap();
    await vi.waitFor(() => expect(invokeMock).toHaveBeenCalledTimes(2));
    await useProjectStore.getState().openProject(selected.rootPath);
    resolveAutomatic({ kind: "opened", summary });
    await bootstrapping;

    expect(useProjectStore.getState().currentProject).toEqual(selected);
  });
});
