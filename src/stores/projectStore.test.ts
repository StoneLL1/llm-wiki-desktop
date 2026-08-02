import { beforeEach, describe, expect, it, vi } from "vitest";

import type {
  OpenProjectResponse,
  ProjectOpenAssessment,
  ProjectSummary,
  RecentProject,
} from "../types/project";

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

const assessment: ProjectOpenAssessment = {
  assessmentId: "assessment-a",
  canonicalRootPath: recent.rootPath,
  canonicalIdentityKey: "identity-a",
  identityRevision: "revision-a",
  format: "markdown_vault",
  trust: "untrusted",
  filesystemAccess: "read_only",
  health: "healthy",
  layout: { markdownRoots: [{ path: ".", role: "mixed" }] },
  confidence: "high",
  markers: [],
  capabilities: ["read_markdown"],
  warnings: [],
  layoutWarnings: [],
  git: { isRepository: false, branch: null, head: null, hasChanges: false },
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
    assessmentOperationId: null,
    assessment: null,
    assessing: false,
    assessmentError: null,
    initializing: false,
    initialized: false,
    error: null,
  });
  useProjectStore.getState().setPendingAction(undefined);
});

describe("projectStore bootstrap", () => {
  it("keeps operation and completed assessment IDs in separate command scopes", async () => {
    invokeMock
      .mockResolvedValueOnce({ assessmentOperationId: "operation-a" })
      .mockResolvedValueOnce({ assessmentOperationId: "operation-a", status: "completed", assessment })
      .mockResolvedValueOnce(summary);

    const result = await useProjectStore.getState().assessProject(recent.rootPath);
    await useProjectStore.getState().openAssessedProject(result.assessmentId);

    expect(invokeMock.mock.calls).toEqual([
      ["start_project_open_assessment", { request: { path: recent.rootPath } }],
      ["get_project_open_assessment", { request: { assessmentOperationId: "operation-a" } }],
      ["open_assessed_project", { request: { assessmentId: "assessment-a" } }],
    ]);
  });

  it("cancels only with the operation ID and clears the completed snapshot", async () => {
    invokeMock
      .mockResolvedValueOnce({ assessmentOperationId: "operation-a" })
      .mockResolvedValueOnce({ assessmentOperationId: "operation-a", status: "completed", assessment })
      .mockResolvedValueOnce(undefined);

    await useProjectStore.getState().assessProject(recent.rootPath);
    await useProjectStore.getState().cancelProjectAssessment();

    expect(invokeMock).toHaveBeenLastCalledWith("cancel_project_open_assessment", {
      request: { assessmentOperationId: "operation-a" },
    });
    expect(useProjectStore.getState()).toMatchObject({ assessmentOperationId: null, assessment: null, assessing: false });
  });

  it("discards a delayed assessment when the active project changes", async () => {
    let resolveAssessment!: (value: {
      assessmentOperationId: string;
      status: "completed";
      assessment: ProjectOpenAssessment;
    }) => void;
    const delayedAssessment = new Promise<{
      assessmentOperationId: string;
      status: "completed";
      assessment: ProjectOpenAssessment;
    }>((resolve) => {
      resolveAssessment = resolve;
    });
    invokeMock.mockImplementation((command: string) => {
      if (command === "start_project_open_assessment") {
        return Promise.resolve({ assessmentOperationId: "operation-a" });
      }
      if (command === "get_project_open_assessment") return delayedAssessment;
      if (command === "cancel_project_open_assessment") return Promise.resolve(undefined);
      return Promise.reject(new Error(`Unexpected command: ${command}`));
    });
    const projectB = { ...summary, projectId: "project-b", rootPath: "D:/wiki/project-b" };

    const assessing = useProjectStore.getState().assessProject(recent.rootPath);
    await vi.waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("get_project_open_assessment", {
        request: { assessmentOperationId: "operation-a" },
      }),
    );
    useProjectStore.getState().setCurrentProject(projectB);
    resolveAssessment({
      assessmentOperationId: "operation-a",
      status: "completed",
      assessment,
    });

    await expect(assessing).rejects.toThrow("superseded");
    expect(invokeMock).toHaveBeenCalledWith("cancel_project_open_assessment", {
      request: { assessmentOperationId: "operation-a" },
    });
    expect(useProjectStore.getState()).toMatchObject({
      currentProject: projectB,
      assessment: null,
      assessing: false,
    });
  });

  it("does not let a delayed confirmation replace a newer project or its pending action", async () => {
    let resolveConfirmation!: (value: import("../types/backend").ConfirmedAction) => void;
    const confirmation = new Promise<import("../types/backend").ConfirmedAction>((resolve) => {
      resolveConfirmation = resolve;
    });
    const actionA = { id: "a", actionType: "delete_file" as const, title: "A", message: "A", riskLevel: "destructive" as const, affectedPaths: [], preview: null, expiresAt: null };
    const actionB = { ...actionA, id: "b", title: "B" };
    const projectB = { ...summary, projectId: "project-b", rootPath: "D:/wiki/project-b" };
    useProjectStore.getState().setCurrentProject(summary);
    useProjectStore.getState().setPendingAction(actionA);
    invokeMock.mockReturnValue(confirmation);

    const pending = useProjectStore.getState().confirmPendingAction();
    useProjectStore.getState().setCurrentProject(projectB);
    useProjectStore.getState().setPendingAction(actionB);
    resolveConfirmation({ action: actionA, status: "confirmed", checkpointExists: true, projectSummary: summary });
    await pending;

    expect(useProjectStore.getState()).toMatchObject({ currentProject: projectB, pendingAction: actionB });
  });

  it("cancels and hides a project-bound pending action when the project changes", async () => {
    const action = { id: "project-a-action", actionType: "create_git_checkpoint" as const, title: "Checkpoint", message: "Checkpoint", riskLevel: "high" as const, affectedPaths: ["note.md"], preview: null, expiresAt: null };
    const projectB = { ...summary, projectId: "project-b", rootPath: "D:/wiki/project-b" };
    useProjectStore.getState().setCurrentProject(summary);
    useProjectStore.getState().setPendingAction(action);
    invokeMock.mockResolvedValue(undefined);

    useProjectStore.getState().setCurrentProject(projectB);

    expect(useProjectStore.getState().pendingAction).toBeUndefined();
    expect(invokeMock).toHaveBeenCalledWith("confirm_pending_action", {
      request: { actionId: action.id, status: "cancelled" },
    });
  });

  it("cancels a delayed authority action returned after the project changed", async () => {
    let resolveAction!: (value: import("../types/backend").PendingAction) => void;
    const delayedAction = new Promise<import("../types/backend").PendingAction>((resolve) => {
      resolveAction = resolve;
    });
    const action = { id: "late-trust", actionType: "trust_compatible_project" as const, title: "Trust", message: "Trust", riskLevel: "high" as const, affectedPaths: [], preview: null, expiresAt: null };
    const projectB = { ...summary, projectId: "project-b", rootPath: "D:/wiki/project-b" };
    invokeMock.mockImplementation((command: string) => {
      if (command === "trust_project") return delayedAction;
      if (command === "confirm_pending_action") return Promise.resolve(undefined);
      return Promise.reject(new Error(`Unexpected command: ${command}`));
    });
    useProjectStore.getState().setCurrentProject(summary);

    const request = useProjectStore.getState().trustAssessedProject("assessment-a");
    useProjectStore.getState().setCurrentProject(projectB);
    resolveAction(action);
    await request;

    expect(useProjectStore.getState().pendingAction).toBeUndefined();
    expect(invokeMock).toHaveBeenCalledWith("confirm_pending_action", {
      request: { actionId: action.id, status: "cancelled" },
    });
  });

  it("forwards the explicit choice to leave Git initialization disabled", async () => {
    const action = {
      id: "enable-a",
      actionType: "enable_compatible_project" as const,
      title: "Enable",
      message: "Enable",
      riskLevel: "high" as const,
      affectedPaths: [".app/compat/purpose.md", ".app/compat/schema.md"],
      preview: null,
      expiresAt: null,
    };
    useProjectStore.setState({ currentProject: summary });
    invokeMock.mockResolvedValueOnce(action);

    await useProjectStore
      .getState()
      .enableCompatibleFullFeatures("assessment-a", "general", false);

    expect(invokeMock).toHaveBeenCalledWith("enable_compatible_full_features", {
      request: {
        assessmentId: "assessment-a",
        projectId: summary.projectId,
        projectRootPath: summary.rootPath,
        template: "general",
        initializeGit: false,
      },
    });
    expect(useProjectStore.getState().pendingAction).toEqual(action);
  });

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
