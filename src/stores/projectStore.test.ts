import { beforeEach, describe, expect, it, vi } from "vitest";

import type {
  OpenedProject,
  ProjectOpenAssessment,
  ProjectSessionAuthority,
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

const authority: ProjectSessionAuthority = {
  projectId: summary.projectId,
  canonicalRootPath: recent.rootPath,
  canonicalIdentityKey: assessment.canonicalIdentityKey,
  identityRevision: assessment.identityRevision,
  authorityRevision: "authority-a",
  format: assessment.format,
  trust: assessment.trust,
  filesystemAccess: assessment.filesystemAccess,
  health: assessment.health,
  layout: assessment.layout,
  confidence: assessment.confidence,
  capabilities: assessment.capabilities,
  warnings: assessment.warnings,
  layoutWarnings: assessment.layoutWarnings,
  git: assessment.git,
};

const openedAssessed: OpenedProject = { summary, authority };

beforeEach(() => {
  invokeMock.mockReset();
  Object.defineProperty(window, "__TAURI_INTERNALS__", {
    value: {},
    configurable: true,
  });
  useProjectStore.setState({
    currentProject: defaultProject,
    authority: null,
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
    invokeMock.mockImplementation((command: string) => {
      if (command === "start_project_open_assessment") {
        return Promise.resolve({ assessmentOperationId: "operation-a" });
      }
      if (command === "get_project_open_assessment") {
        return Promise.resolve({ assessmentOperationId: "operation-a", status: "completed", assessment });
      }
      if (command === "open_assessed_project") return Promise.resolve(openedAssessed);
      if (command === "start_project_inventory") return Promise.resolve(undefined);
      return Promise.reject(new Error(`Unexpected command: ${command}`));
    });

    const result = await useProjectStore.getState().assessProject(recent.rootPath);
    await useProjectStore.getState().openAssessedProject(result.assessmentId);

    expect(invokeMock.mock.calls).toEqual([
      ["start_project_open_assessment", { request: { path: recent.rootPath } }],
      ["get_project_open_assessment", { request: { assessmentOperationId: "operation-a" } }],
      ["open_assessed_project", { request: { assessmentId: "assessment-a" } }],
      ["start_project_inventory", {
        request: { projectId: summary.projectId, projectRootPath: summary.rootPath },
      }],
    ]);
    expect(useProjectStore.getState().authority).toEqual(authority);
  });

  it("refreshes the authority snapshot after an explicit project change", async () => {
    useProjectStore.getState().setCurrentProject(summary);
    const refreshed = { ...authority, trust: "trusted" as const, authorityRevision: "authority-b" };
    invokeMock.mockResolvedValueOnce(refreshed);

    await expect(useProjectStore.getState().refreshProjectAuthority()).resolves.toEqual(refreshed);
    expect(invokeMock).toHaveBeenCalledWith("get_project_session_authority", {
      request: { projectId: summary.projectId, projectRootPath: summary.rootPath },
    });
    expect(useProjectStore.getState().authority).toEqual(refreshed);
  });

  it("opens an ambiguous Markdown folder only through the explicit typed choice", async () => {
    useProjectStore.setState({ assessment });
    invokeMock.mockResolvedValueOnce(openedAssessed);

    await expect(
      useProjectStore.getState().resolveAmbiguousAssessedProject(assessment.assessmentId),
    ).resolves.toEqual(summary);

    expect(invokeMock).toHaveBeenCalledWith("resolve_ambiguous_assessed_project", {
      request: { assessmentId: assessment.assessmentId, intent: "open_as_markdown_vault" },
    });
    expect(useProjectStore.getState().authority).toEqual(authority);
  });

  it("remembers the create-from-materials choice without opening the folder", async () => {
    const remembered = {
      ...assessment,
      rememberedOpenIntent: "create_from_materials" as const,
    };
    useProjectStore.setState({ assessment });
    invokeMock.mockResolvedValueOnce(remembered);

    await expect(
      useProjectStore
        .getState()
        .rememberAmbiguousProjectIntent(assessment.assessmentId, "create_from_materials"),
    ).resolves.toEqual(remembered);

    expect(invokeMock).toHaveBeenCalledWith("remember_ambiguous_project_intent", {
      request: { assessmentId: assessment.assessmentId, intent: "create_from_materials" },
    });
    expect(useProjectStore.getState().currentProject).toEqual(defaultProject);
    expect(useProjectStore.getState().assessment).toEqual(remembered);
  });

  it("clears a remembered ambiguous-folder choice without opening the folder", async () => {
    const remembered = {
      ...assessment,
      rememberedOpenIntent: "create_from_materials" as const,
    };
    const cleared = { ...assessment, rememberedOpenIntent: undefined };
    useProjectStore.setState({ assessment: remembered });
    invokeMock.mockResolvedValueOnce(cleared);

    await expect(
      useProjectStore.getState().clearAmbiguousProjectIntent(assessment.assessmentId),
    ).resolves.toEqual(cleared);

    expect(invokeMock).toHaveBeenCalledWith("clear_ambiguous_project_intent", {
      request: { assessmentId: assessment.assessmentId },
    });
    expect(useProjectStore.getState().currentProject).toEqual(defaultProject);
    expect(useProjectStore.getState().assessment).toEqual(cleared);
  });

  it("removes a recent-project entry without changing the current project", async () => {
    const remaining = [{ ...recent, projectId: "project-b", rootPath: "D:/knowledge/project-b" }];
    useProjectStore.setState({ recentProjects: [recent, ...remaining] });
    invokeMock.mockResolvedValueOnce(remaining);

    await expect(
      useProjectStore.getState().removeRecentProject(recent.projectId, recent.rootPath),
    ).resolves.toEqual(remaining);

    expect(invokeMock).toHaveBeenCalledWith("remove_recent_project", {
      request: { projectId: recent.projectId, rootPath: recent.rootPath },
    });
    expect(useProjectStore.getState().currentProject).toEqual(defaultProject);
    expect(useProjectStore.getState().recentProjects).toEqual(remaining);
  });

  it("opens a relocated project only through the backend identity-verified command", async () => {
    const relocatedRoot = "D:/knowledge/relocated-project-a";
    const relocatedSummary = { ...summary, rootPath: relocatedRoot };
    const relocatedAuthority = { ...authority, canonicalRootPath: relocatedRoot };
    const relocatedRecent = { ...recent, rootPath: relocatedRoot, missing: false };
    invokeMock.mockImplementation((command: string) => {
      if (command === "relocate_recent_project") {
        return Promise.resolve({ summary: relocatedSummary, authority: relocatedAuthority });
      }
      if (command === "start_project_inventory") return Promise.resolve(undefined);
      if (command === "list_recent_projects") return Promise.resolve([relocatedRecent]);
      return Promise.reject(new Error(`Unexpected command: ${command}`));
    });

    await expect(
      useProjectStore
        .getState()
        .relocateRecentProject(assessment.assessmentId, recent.projectId, recent.rootPath),
    ).resolves.toEqual(relocatedSummary);

    expect(invokeMock).toHaveBeenCalledWith("relocate_recent_project", {
      request: {
        assessmentId: assessment.assessmentId,
        previousProjectId: recent.projectId,
        previousRootPath: recent.rootPath,
      },
    });
    expect(useProjectStore.getState()).toMatchObject({
      currentProject: relocatedSummary,
      authority: relocatedAuthority,
      assessment: null,
      recentProjects: [relocatedRecent],
    });
  });

  it("releases the current project while preserving a completed assessment for its decision screen", () => {
    useProjectStore.setState({
      currentProject: summary,
      authority,
      assessmentOperationId: "operation-a",
      assessment,
    });

    useProjectStore.getState().showAssessedProjectSelection();

    expect(useProjectStore.getState()).toMatchObject({
      currentProject: defaultProject,
      authority: null,
      assessmentOperationId: "operation-a",
      assessment,
      assessing: false,
      assessmentError: null,
    });
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

  it("reassesses the most recent project before registering its open context", async () => {
    invokeMock.mockImplementation((command: string) => {
      if (command === "list_recent_projects") return Promise.resolve([recent]);
      if (command === "start_project_open_assessment") {
        return Promise.resolve({ assessmentOperationId: "operation-a" });
      }
      if (command === "get_project_open_assessment") {
        return Promise.resolve({ assessmentOperationId: "operation-a", status: "completed", assessment });
      }
      if (command === "open_assessed_project") return Promise.resolve(openedAssessed);
      if (command === "start_project_inventory") return Promise.resolve(undefined);
      return Promise.reject(new Error(`Unexpected command: ${command}`));
    });

    await useProjectStore.getState().bootstrap();

    expect(invokeMock.mock.calls).toEqual([
      ["list_recent_projects"],
      ["start_project_open_assessment", { request: { path: recent.rootPath } }],
      ["get_project_open_assessment", { request: { assessmentOperationId: "operation-a" } }],
      ["open_assessed_project", { request: { assessmentId: assessment.assessmentId } }],
      ["start_project_inventory", {
        request: { projectId: summary.projectId, projectRootPath: summary.rootPath },
      }],
    ]);
    expect(useProjectStore.getState().currentProject).toEqual(summary);
    expect(useProjectStore.getState().initialized).toBe(true);
  });

  it("keeps the workspace empty when the latest recent project is missing", async () => {
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
    invokeMock.mockResolvedValueOnce([missing, recent]);

    await useProjectStore.getState().bootstrap();

    expect(invokeMock.mock.calls).toEqual([["list_recent_projects"]]);
    expect(useProjectStore.getState().currentProject).toEqual(defaultProject);
    expect(useProjectStore.getState().error).toContain(missing.rootPath);
  });

  it("never lets a delayed automatic reopen overwrite an explicit project selection", async () => {
    let resolveAutomatic!: (value: OpenedProject) => void;
    const automatic = new Promise<OpenedProject>((resolve) => {
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
      if (command === "start_project_open_assessment" && args?.request?.path === recent.rootPath) {
        return Promise.resolve({ assessmentOperationId: "operation-a" });
      }
      if (command === "get_project_open_assessment") {
        return Promise.resolve({ assessmentOperationId: "operation-a", status: "completed", assessment });
      }
      if (command === "open_assessed_project") return automatic;
      return Promise.reject(new Error(`Unexpected command: ${command}`));
    });

    const bootstrapping = useProjectStore.getState().bootstrap();
    await vi.waitFor(() => expect(invokeMock).toHaveBeenCalledWith("open_assessed_project", {
      request: { assessmentId: assessment.assessmentId },
    }));
    useProjectStore.getState().setCurrentProject(selected);
    resolveAutomatic(openedAssessed);
    await bootstrapping;

    expect(useProjectStore.getState().currentProject).toEqual(selected);
  });
});
