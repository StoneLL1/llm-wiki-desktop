import { invoke } from "@tauri-apps/api/core";
import { create } from "zustand";

import type { ConfirmedAction } from "../types/backend";
import type { TaskProjectPersistenceReason } from "../types/task";
import type { WorkflowPersistenceMode } from "../types/workflow";
import type {
  AgentRoute,
  OpenedProject,
  ProjectAssessmentOperation,
  ProjectOpenAssessment,
  ProjectOpenIntent,
  OpenProjectResponse,
  ProjectSummary,
  ProjectSessionAuthority,
  ProjectTemplate,
  RecentProject,
  StartProjectOpenAssessmentResult,
} from "../types/project";
import { invalidateProjectScope } from "./projectScope";
import { resetProjectScopedStores } from "./resetProjectScope";

export interface CreateProjectPayload {
  rootPath: string;
  name: string;
  template?: ProjectTemplate;
}

interface ProjectState {
  currentProject: ProjectSummary;
  authority: ProjectSessionAuthority | null;
  recentProjects: RecentProject[];
  pendingAction: OpenProjectResponse["pendingAction"];
  assessmentOperationId: string | null;
  assessment: ProjectOpenAssessment | null;
  assessing: boolean;
  assessmentError: string | null;
  initializing: boolean;
  initialized: boolean;
  error: string | null;
  taskPersistence: WorkflowPersistenceMode | null;
  taskPersistenceReason: TaskProjectPersistenceReason | null;
  setCurrentProject: (project: ProjectSummary) => void;
  setAgentRoute: (projectId: string, rootPath: string, agentRoute: AgentRoute) => void;
  clearCurrentProject: () => void;
  showAssessedProjectSelection: () => void;
  setRecentProjects: (projects: RecentProject[]) => void;
  setPendingAction: (action: OpenProjectResponse["pendingAction"]) => void;
  setTaskPersistence: (
    projectId: string,
    rootPath: string,
    persistence: WorkflowPersistenceMode | null,
    reason: TaskProjectPersistenceReason | null,
  ) => void;
  loadRecentProjects: () => Promise<RecentProject[]>;
  removeRecentProject: (projectId: string, rootPath: string) => Promise<RecentProject[]>;
  createProject: (payload: CreateProjectPayload) => Promise<ProjectSummary>;
  openProject: (path: string) => Promise<OpenProjectResponse>;
  assessProject: (path: string) => Promise<ProjectOpenAssessment>;
  cancelProjectAssessment: () => Promise<void>;
  openAssessedProject: (assessmentId: string) => Promise<ProjectSummary>;
  relocateRecentProject: (
    assessmentId: string,
    previousProjectId: string,
    previousRootPath: string,
  ) => Promise<ProjectSummary>;
  resolveAmbiguousAssessedProject: (assessmentId: string) => Promise<ProjectSummary>;
  rememberAmbiguousProjectIntent: (
    assessmentId: string,
    intent: Exclude<ProjectOpenIntent, "open_as_markdown_vault">,
  ) => Promise<ProjectOpenAssessment>;
  clearAmbiguousProjectIntent: (assessmentId: string) => Promise<ProjectOpenAssessment>;
  refreshProjectAuthority: () => Promise<ProjectSessionAuthority>;
  assessCurrentProject: () => Promise<ProjectOpenAssessment>;
  trustAssessedProject: (assessmentId: string) => Promise<void>;
  revokeAssessedProjectTrust: (assessmentId: string) => Promise<ProjectOpenAssessment>;
  enableCompatibleFullFeatures: (
    assessmentId: string,
    template: ProjectTemplate,
    initializeGit: boolean,
  ) => Promise<void>;
  configureCompatibleLayout: (
    assessmentId: string,
    mapping: { wikiRoot?: string; sourceRoot?: string; exportRoot?: string },
  ) => Promise<void>;
  repairAssessedProject: (assessmentId: string) => Promise<void>;
  requestAssessedGitInitialization: (assessmentId: string) => Promise<void>;
  requestAssessedGitCheckpoint: (assessmentId: string) => Promise<void>;
  confirmPendingAction: () => Promise<ConfirmedAction | undefined>;
  cancelPendingAction: () => Promise<void>;
  bootstrap: () => Promise<void>;
}

const hasTauri = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

let selectionEpoch = 0;
let assessmentEpoch = 0;
let pendingActionProjectKey: string | null = null;
let pendingActionId: string | null = null;

export const defaultProject: ProjectSummary = {
  projectId: "",
  name: "",
  rootPath: "",
  template: "general",
  wikiPageCount: 0,
  sourceCount: 0,
  taskCount: 0,
  indexState: "missing",
  graphState: "missing",
  agentRoute: "unconfigured",
  health: {
    isWikiProject: false,
    hasPurpose: false,
    hasSchema: false,
    hasAppState: false,
    hasObsidian: false,
    missingPaths: [],
  },
};

export const defaultRecentProjects: RecentProject[] = [];

function errorMessage(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error) {
    const message = (error as { message: unknown }).message;
    if (typeof message === "string") return message;
  }
  return String(error);
}

function cancelAssessmentOperation(operationId: string | null): void {
  if (!operationId || !hasTauri()) return;
  void invoke("cancel_project_open_assessment", {
    request: { assessmentOperationId: operationId },
  }).catch(() => undefined);
}

function projectKey(project: Pick<ProjectSummary, "projectId" | "rootPath">): string {
  return `${project.projectId}\u0000${project.rootPath}`;
}

function abandonPendingAction(action: OpenProjectResponse["pendingAction"]): void {
  if (!action) {
    pendingActionProjectKey = null;
    pendingActionId = null;
    return;
  }
  if (pendingActionId === action.id) {
    pendingActionProjectKey = null;
    pendingActionId = null;
  }
  if (!hasTauri()) return;
  void invoke<ConfirmedAction>("confirm_pending_action", {
    request: { actionId: action.id, status: "cancelled" },
  }).catch(() => undefined);
}

function bindPendingAction(
  action: OpenProjectResponse["pendingAction"],
  project: Pick<ProjectSummary, "projectId" | "rootPath">,
): void {
  pendingActionProjectKey = action ? projectKey(project) : null;
  pendingActionId = action?.id ?? null;
}

async function refreshTaskPersistence(
  project: Pick<ProjectSummary, "projectId" | "rootPath">,
): Promise<void> {
  const current = useProjectStore.getState().currentProject;
  if (
    !project.projectId ||
    !project.rootPath ||
    current.projectId !== project.projectId ||
    current.rootPath !== project.rootPath
  ) {
    return;
  }
  const { recoverTasksForProject } = await import("./taskStore");
  await recoverTasksForProject(project.projectId, project.rootPath);
}

function startProjectInventory(project: Pick<ProjectSummary, "projectId" | "rootPath">): void {
  if (!hasTauri() || !project.projectId || !project.rootPath) return;
  // Promise.resolve keeps this fire-and-forget path safe for embedded/test
  // environments whose invoke shim does not implement every background command.
  void Promise.resolve(invoke("start_project_inventory", {
    request: { projectId: project.projectId, projectRootPath: project.rootPath },
  })).catch(() => {
    const current = useProjectStore.getState().currentProject;
    if (current.projectId === project.projectId && current.rootPath === project.rootPath) {
      useProjectStore.setState({
        currentProject: { ...current, inventoryState: "failed" },
      });
    }
  });
}

export const useProjectStore = create<ProjectState>((set, get) => ({
  currentProject: defaultProject,
  authority: null,
  recentProjects: defaultRecentProjects,
  pendingAction: undefined,
  assessmentOperationId: null,
  assessment: null,
  assessing: false,
  assessmentError: null,
  initializing: false,
  initialized: false,
  error: null,
  taskPersistence: null,
  taskPersistenceReason: null,
  setCurrentProject: (currentProject) => {
    const previous = get().currentProject;
    const changedProject =
      previous.projectId !== currentProject.projectId || previous.rootPath !== currentProject.rootPath;
    if (changedProject) {
      abandonPendingAction(get().pendingAction);
      assessmentEpoch += 1;
      cancelAssessmentOperation(get().assessmentOperationId);
      selectionEpoch += 1;
      invalidateProjectScope();
      resetProjectScopedStores();
    }
    set({
      currentProject,
      authority: changedProject ? null : get().authority,
      pendingAction: changedProject ? undefined : get().pendingAction,
      assessmentOperationId: null,
      assessment: null,
      assessing: false,
      assessmentError: null,
      taskPersistence: changedProject ? null : get().taskPersistence,
      taskPersistenceReason: changedProject ? null : get().taskPersistenceReason,
    });
  },
  setAgentRoute: (projectId, rootPath, agentRoute) =>
    set((state) => {
      if (
        state.currentProject.projectId !== projectId ||
        state.currentProject.rootPath !== rootPath
      ) {
        return state;
      }
      return {
        currentProject: { ...state.currentProject, agentRoute },
      };
    }),
  clearCurrentProject: () => {
    abandonPendingAction(get().pendingAction);
    cancelAssessmentOperation(get().assessmentOperationId);
    selectionEpoch += 1;
    invalidateProjectScope();
    resetProjectScopedStores();
    assessmentEpoch += 1;
    set({
      currentProject: defaultProject,
      authority: null,
      pendingAction: undefined,
      assessmentOperationId: null,
      assessment: null,
      assessing: false,
      assessmentError: null,
      taskPersistence: null,
      taskPersistenceReason: null,
    });
  },
  showAssessedProjectSelection: () => {
    const assessment = get().assessment;
    const assessmentOperationId = get().assessmentOperationId;
    if (!assessment) {
      get().clearCurrentProject();
      return;
    }
    abandonPendingAction(get().pendingAction);
    // The completed assessment remains valid for its short backend TTL. Keep
    // it while releasing the active project so the no-project workspace can
    // present its explicit next decision instead of forcing another scan.
    selectionEpoch += 1;
    invalidateProjectScope();
    resetProjectScopedStores();
    set({
      currentProject: defaultProject,
      authority: null,
      pendingAction: undefined,
      assessmentOperationId,
      assessment,
      assessing: false,
      assessmentError: null,
      taskPersistence: null,
      taskPersistenceReason: null,
    });
  },
  setRecentProjects: (recentProjects) => set({ recentProjects }),
  setPendingAction: (pendingAction) => {
    bindPendingAction(pendingAction, get().currentProject);
    set({ pendingAction });
  },
  setTaskPersistence: (projectId, rootPath, taskPersistence, taskPersistenceReason) =>
    set((state) => {
      if (
        state.currentProject.projectId !== projectId ||
        state.currentProject.rootPath !== rootPath
      ) {
        return state;
      }
      return { taskPersistence, taskPersistenceReason };
    }),
  loadRecentProjects: async () => {
    if (!hasTauri()) {
      set({ recentProjects: [] });
      return [];
    }
    const projects = await invoke<RecentProject[]>("list_recent_projects");
    set({ recentProjects: projects, error: null });
    return projects;
  },
  removeRecentProject: async (projectId, rootPath) => {
    const projects = await invoke<RecentProject[]>("remove_recent_project", {
      request: { projectId, rootPath },
    });
    set({ recentProjects: projects, error: null });
    return projects;
  },
  createProject: async ({ rootPath, name, template }) => {
    abandonPendingAction(get().pendingAction);
    assessmentEpoch += 1;
    cancelAssessmentOperation(get().assessmentOperationId);
    set({ pendingAction: undefined, assessmentOperationId: null, assessment: null, assessing: false, assessmentError: null });
    const epoch = ++selectionEpoch;
    const opened = await invoke<OpenedProject>("create_project", {
      request: { rootPath, name, template: template ?? "general" },
    });
    if (epoch === selectionEpoch) {
      invalidateProjectScope();
      resetProjectScopedStores();
      set({
        currentProject: opened.summary,
        authority: opened.authority,
        pendingAction: undefined,
        error: null,
        taskPersistence: null,
        taskPersistenceReason: null,
      });
    }
    return opened.summary;
  },
  openProject: async (path) => {
    if (!hasTauri()) {
      return { kind: "opened" as const, summary: undefined, authority: undefined, pendingAction: undefined };
    }
    assessmentEpoch += 1;
    abandonPendingAction(get().pendingAction);
    cancelAssessmentOperation(get().assessmentOperationId);
    set({ pendingAction: undefined, assessmentOperationId: null, assessment: null, assessing: false, assessmentError: null });
    const epoch = ++selectionEpoch;
    const response = await invoke<OpenProjectResponse>("open_project", { request: { path } });
    if (epoch !== selectionEpoch) {
      return response;
    }
    if (response.summary) {
      invalidateProjectScope();
      resetProjectScopedStores();
      set({
        currentProject: response.summary,
        authority: response.authority ?? null,
        error: null,
        taskPersistence: null,
        taskPersistenceReason: null,
      });
    }
    bindPendingAction(response.pendingAction, response.summary ?? get().currentProject);
    set({ pendingAction: response.pendingAction });
    return response;
  },
  assessProject: async (path) => {
    if (!hasTauri()) {
      throw new Error("Project assessment requires the desktop runtime.");
    }
    const requestEpoch = ++assessmentEpoch;
    const previousOperationId = get().assessmentOperationId;
    if (previousOperationId) {
      cancelAssessmentOperation(previousOperationId);
    }
    set({
      assessmentOperationId: null,
      assessment: null,
      assessing: true,
      assessmentError: null,
    });
    try {
      const started = await invoke<StartProjectOpenAssessmentResult>(
        "start_project_open_assessment",
        { request: { path } },
      );
      if (requestEpoch !== assessmentEpoch) {
        void invoke("cancel_project_open_assessment", {
          request: { assessmentOperationId: started.assessmentOperationId },
        }).catch(() => undefined);
        throw new Error("Project assessment was superseded.");
      }
      set({ assessmentOperationId: started.assessmentOperationId });
      for (;;) {
        const operation = await invoke<ProjectAssessmentOperation>(
          "get_project_open_assessment",
          {
            request: { assessmentOperationId: started.assessmentOperationId },
          },
        );
        if (requestEpoch !== assessmentEpoch) {
          throw new Error("Project assessment was superseded.");
        }
        if (operation.status === "running") {
          await new Promise((resolve) => window.setTimeout(resolve, 40));
          continue;
        }
        if (operation.status === "failed" || !operation.assessment) {
          throw new Error(operation.error?.message ?? "Project assessment failed.");
        }
        set({
          assessment: operation.assessment,
          assessing: false,
          assessmentError: null,
        });
        return operation.assessment;
      }
    } catch (error) {
      if (requestEpoch === assessmentEpoch) {
        set({ assessing: false, assessmentError: errorMessage(error) });
      }
      throw error;
    }
  },
  cancelProjectAssessment: async () => {
    const operationId = get().assessmentOperationId;
    assessmentEpoch += 1;
    set({
      assessmentOperationId: null,
      assessment: null,
      assessing: false,
      assessmentError: null,
      taskPersistence: null,
      taskPersistenceReason: null,
    });
    if (operationId && hasTauri()) {
      await invoke("cancel_project_open_assessment", {
        request: { assessmentOperationId: operationId },
      }).catch(() => undefined);
    }
  },
  openAssessedProject: async (assessmentId) => {
    abandonPendingAction(get().pendingAction);
    set({ pendingAction: undefined });
    const requestEpoch = ++selectionEpoch;
    const opened = await invoke<OpenedProject>("open_assessed_project", {
      request: { assessmentId },
    });
    if (requestEpoch === selectionEpoch) {
      assessmentEpoch += 1;
      invalidateProjectScope();
      resetProjectScopedStores();
      set({
        currentProject: opened.summary,
        authority: opened.authority,
        pendingAction: undefined,
        assessmentOperationId: null,
        assessment: null,
        assessing: false,
        assessmentError: null,
        error: null,
      });
      startProjectInventory(opened.summary);
    }
    return opened.summary;
  },
  relocateRecentProject: async (assessmentId, previousProjectId, previousRootPath) => {
    abandonPendingAction(get().pendingAction);
    set({ pendingAction: undefined });
    const requestEpoch = ++selectionEpoch;
    const opened = await invoke<OpenedProject>("relocate_recent_project", {
      request: { assessmentId, previousProjectId, previousRootPath },
    });
    if (requestEpoch === selectionEpoch) {
      assessmentEpoch += 1;
      invalidateProjectScope();
      resetProjectScopedStores();
      set({
        currentProject: opened.summary,
        authority: opened.authority,
        pendingAction: undefined,
        assessmentOperationId: null,
        assessment: null,
        assessing: false,
        assessmentError: null,
        error: null,
      });
      startProjectInventory(opened.summary);
    }
    // The relocation command has already committed the global recent-file
    // update. Refresh it best-effort so the switcher does not keep rendering
    // the old missing path; failure here must not turn a successful open into
    // a failed relocation for the user.
    await get().loadRecentProjects().catch(() => undefined);
    return opened.summary;
  },
  resolveAmbiguousAssessedProject: async (assessmentId) => {
    abandonPendingAction(get().pendingAction);
    set({ pendingAction: undefined });
    const requestEpoch = ++selectionEpoch;
    const opened = await invoke<OpenedProject>("resolve_ambiguous_assessed_project", {
      request: { assessmentId, intent: "open_as_markdown_vault" },
    });
    if (requestEpoch === selectionEpoch) {
      assessmentEpoch += 1;
      invalidateProjectScope();
      resetProjectScopedStores();
      set({
        currentProject: opened.summary,
        authority: opened.authority,
        pendingAction: undefined,
        assessmentOperationId: null,
        assessment: null,
        assessing: false,
        assessmentError: null,
        error: null,
      });
      startProjectInventory(opened.summary);
    }
    return opened.summary;
  },
  rememberAmbiguousProjectIntent: async (assessmentId, intent) => {
    const assessment = await invoke<ProjectOpenAssessment>(
      "remember_ambiguous_project_intent",
      { request: { assessmentId, intent } },
    );
    if (get().assessment?.assessmentId === assessmentId) {
      set({ assessment });
    }
    return assessment;
  },
  clearAmbiguousProjectIntent: async (assessmentId) => {
    const assessment = await invoke<ProjectOpenAssessment>(
      "clear_ambiguous_project_intent",
      { request: { assessmentId } },
    );
    if (get().assessment?.assessmentId === assessmentId) {
      set({ assessment });
    }
    return assessment;
  },
  refreshProjectAuthority: async () => {
    const project = get().currentProject;
    if (!project.projectId || !project.rootPath) {
      throw new Error("No knowledge base is open.");
    }
    if (!hasTauri()) {
      throw new Error("Project authority requires the desktop runtime.");
    }
    const epoch = selectionEpoch;
    const authority = await invoke<ProjectSessionAuthority>(
      "get_project_session_authority",
      { request: { projectId: project.projectId, projectRootPath: project.rootPath } },
    );
    if (
      epoch === selectionEpoch &&
      get().currentProject.projectId === project.projectId &&
      get().currentProject.rootPath === project.rootPath
    ) {
      set({ authority });
    }
    return authority;
  },
  assessCurrentProject: async () => {
    const project = get().currentProject;
    if (!project.projectId || !project.rootPath) {
      throw new Error("No knowledge base is open.");
    }
    return get().assessProject(project.rootPath);
  },
  trustAssessedProject: async (assessmentId) => {
    const project = get().currentProject;
    const action = await invoke<NonNullable<OpenProjectResponse["pendingAction"]>>(
      "trust_project",
      {
      request: {
        assessmentId,
        projectId: project.projectId,
        projectRootPath: project.rootPath,
      },
      },
    );
    if (
      get().currentProject.projectId === project.projectId &&
      get().currentProject.rootPath === project.rootPath
    ) {
      bindPendingAction(action, project);
      set({ pendingAction: action });
    } else {
      abandonPendingAction(action);
    }
  },
  revokeAssessedProjectTrust: async (assessmentId) => {
    const project = get().currentProject;
    const requestEpoch = assessmentEpoch;
    const assessment = await invoke<ProjectOpenAssessment>("revoke_project_trust", {
      request: {
        assessmentId,
        projectId: project.projectId,
        projectRootPath: project.rootPath,
      },
    });
    if (
      requestEpoch === assessmentEpoch &&
      get().currentProject.projectId === project.projectId &&
      get().currentProject.rootPath === project.rootPath
    ) {
      set({ assessment });
      await get().refreshProjectAuthority();
      await refreshTaskPersistence(project);
    }
    return assessment;
  },
  enableCompatibleFullFeatures: async (assessmentId, template, initializeGit) => {
    const project = get().currentProject;
    const action = await invoke<NonNullable<OpenProjectResponse["pendingAction"]>>(
      "enable_compatible_full_features",
      {
        request: {
          assessmentId,
          projectId: project.projectId,
          projectRootPath: project.rootPath,
          template,
          initializeGit,
        },
      },
    );
    if (
      get().currentProject.projectId === project.projectId &&
      get().currentProject.rootPath === project.rootPath
    ) {
      bindPendingAction(action, project);
      set({ pendingAction: action });
    } else {
      abandonPendingAction(action);
    }
  },
  configureCompatibleLayout: async (assessmentId, mapping) => {
    const project = get().currentProject;
    const action = await invoke<NonNullable<OpenProjectResponse["pendingAction"]>>(
      "configure_compatible_layout",
      {
        request: {
          assessmentId,
          projectId: project.projectId,
          projectRootPath: project.rootPath,
          ...mapping,
        },
      },
    );
    if (
      get().currentProject.projectId === project.projectId &&
      get().currentProject.rootPath === project.rootPath
    ) {
      bindPendingAction(action, project);
      set({ pendingAction: action });
    } else {
      abandonPendingAction(action);
    }
  },
  repairAssessedProject: async (assessmentId) => {
    const project = get().currentProject;
    const action = await invoke<NonNullable<OpenProjectResponse["pendingAction"]>>(
      "prepare_assessed_project_repair",
      {
        request: {
          assessmentId,
          projectId: project.projectId,
          projectRootPath: project.rootPath,
        },
      },
    );
    if (
      get().currentProject.projectId === project.projectId &&
      get().currentProject.rootPath === project.rootPath
    ) {
      bindPendingAction(action, project);
      set({ pendingAction: action });
    } else {
      abandonPendingAction(action);
    }
  },
  requestAssessedGitInitialization: async (assessmentId) => {
    const project = get().currentProject;
    const action = await invoke<NonNullable<OpenProjectResponse["pendingAction"]>>(
      "initialize_git_repository",
      {
        request: {
          assessmentId,
          projectId: project.projectId,
          projectRootPath: project.rootPath,
        },
      },
    );
    if (
      get().currentProject.projectId === project.projectId &&
      get().currentProject.rootPath === project.rootPath
    ) {
      bindPendingAction(action, project);
      set({ pendingAction: action });
    } else {
      abandonPendingAction(action);
    }
  },
  requestAssessedGitCheckpoint: async (assessmentId) => {
    const project = get().currentProject;
    const action = await invoke<NonNullable<OpenProjectResponse["pendingAction"]>>(
      "request_assessed_git_checkpoint",
      {
        request: {
          assessmentId,
          projectId: project.projectId,
          projectRootPath: project.rootPath,
        },
      },
    );
    if (
      get().currentProject.projectId === project.projectId &&
      get().currentProject.rootPath === project.rootPath
    ) {
      bindPendingAction(action, project);
      set({ pendingAction: action });
    } else {
      abandonPendingAction(action);
    }
  },
  confirmPendingAction: async () => {
    const action = get().pendingAction;
    if (!action) {
      return undefined;
    }
    if (pendingActionProjectKey === null) {
      bindPendingAction(action, get().currentProject);
    } else if (pendingActionProjectKey !== projectKey(get().currentProject)) {
      abandonPendingAction(action);
      set({ pendingAction: undefined });
      return undefined;
    }
    const requestEpoch = selectionEpoch;
    const requestProject = get().currentProject;
    if (!hasTauri()) {
      if (
        requestEpoch === selectionEpoch &&
        get().pendingAction?.id === action.id
      ) {
        pendingActionProjectKey = null;
        pendingActionId = null;
        set({ pendingAction: undefined });
      }
      return { action, status: "confirmed", checkpointExists: false, projectSummary: null };
    }
    const confirmed = await invoke<ConfirmedAction>("confirm_pending_action", {
      request: { actionId: action.id, status: "confirmed" },
    });
    const state = get();
    if (
      requestEpoch === selectionEpoch &&
      state.currentProject.projectId === requestProject.projectId &&
      state.currentProject.rootPath === requestProject.rootPath &&
      state.pendingAction?.id === action.id
    ) {
      pendingActionProjectKey = null;
      pendingActionId = null;
      set({
        currentProject: confirmed.projectSummary ?? state.currentProject,
        pendingAction: undefined,
      });
      if (
        action.actionType === "trust_compatible_project" ||
        action.actionType === "enable_compatible_project" ||
        action.actionType === "repair_project" ||
        action.actionType === "initialize_git_repository" ||
        action.actionType === "create_git_checkpoint"
      ) {
        await get().refreshProjectAuthority();
        await refreshTaskPersistence(confirmed.projectSummary ?? requestProject);
        if (action.actionType === "repair_project") {
          startProjectInventory(confirmed.projectSummary ?? requestProject);
        }
      }
    }
    return confirmed;
  },
  cancelPendingAction: async () => {
    const action = get().pendingAction;
    const requestEpoch = selectionEpoch;
    const requestProject = get().currentProject;
    if (action && pendingActionProjectKey === null) {
      bindPendingAction(action, requestProject);
    } else if (action && pendingActionProjectKey !== projectKey(requestProject)) {
      abandonPendingAction(action);
      set({ pendingAction: undefined });
      return;
    }
    try {
      if (action && hasTauri()) {
        await invoke<ConfirmedAction>("confirm_pending_action", {
          request: { actionId: action.id, status: "cancelled" },
        });
      }
    } finally {
      const state = get();
      if (
        requestEpoch === selectionEpoch &&
        state.currentProject.projectId === requestProject.projectId &&
        state.currentProject.rootPath === requestProject.rootPath &&
        state.pendingAction?.id === action?.id
      ) {
        pendingActionProjectKey = null;
        pendingActionId = null;
        set({ pendingAction: undefined });
      }
    }
  },
  bootstrap: async () => {
    if (get().initialized || get().initializing) return;
    const bootstrapEpoch = selectionEpoch;
    set({ initializing: true, error: null });
    if (!hasTauri()) {
      set({ initializing: false, initialized: true, recentProjects: [] });
      return;
    }
    try {
      const recentProjects = await get().loadRecentProjects();
      const last = recentProjects[0];
      if (last?.missing) {
        set({
          error: `The most recently used knowledge base is unavailable: ${last.rootPath}`,
        });
      } else if (last && bootstrapEpoch === selectionEpoch) {
        const assessment = await get().assessProject(last.rootPath);
        const canAutoOpen =
          assessment.health !== "unreadable" &&
          !["ambiguous_markdown", "ordinary_materials", "unknown"].includes(assessment.format);
        if (canAutoOpen && bootstrapEpoch === selectionEpoch) {
          await get().openAssessedProject(assessment.assessmentId);
        }
      }
      set({ initializing: false, initialized: true });
    } catch (error) {
      set({
        currentProject: defaultProject,
        initializing: false,
        initialized: true,
        error: errorMessage(error),
        taskPersistence: null,
        taskPersistenceReason: null,
      });
    }
  },
}));
