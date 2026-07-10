import { invoke } from "@tauri-apps/api/core";
import { create } from "zustand";

import type { ConfirmedAction } from "../types/backend";
import type {
  AgentRoute,
  OpenProjectResponse,
  ProjectSummary,
  ProjectTemplate,
  RecentProject,
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
  recentProjects: RecentProject[];
  pendingAction: OpenProjectResponse["pendingAction"];
  initializing: boolean;
  initialized: boolean;
  error: string | null;
  setCurrentProject: (project: ProjectSummary) => void;
  setAgentRoute: (projectId: string, rootPath: string, agentRoute: AgentRoute) => void;
  clearCurrentProject: () => void;
  setRecentProjects: (projects: RecentProject[]) => void;
  setPendingAction: (action: OpenProjectResponse["pendingAction"]) => void;
  loadRecentProjects: () => Promise<RecentProject[]>;
  createProject: (payload: CreateProjectPayload) => Promise<ProjectSummary>;
  openProject: (path: string) => Promise<OpenProjectResponse>;
  confirmPendingAction: () => Promise<ConfirmedAction | undefined>;
  cancelPendingAction: () => Promise<void>;
  bootstrap: () => Promise<void>;
}

const hasTauri = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

let selectionEpoch = 0;

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

export const useProjectStore = create<ProjectState>((set, get) => ({
  currentProject: defaultProject,
  recentProjects: defaultRecentProjects,
  pendingAction: undefined,
  initializing: false,
  initialized: false,
  error: null,
  setCurrentProject: (currentProject) => {
    const previous = get().currentProject;
    const changedProject =
      previous.projectId !== currentProject.projectId || previous.rootPath !== currentProject.rootPath;
    if (changedProject) {
      selectionEpoch += 1;
      invalidateProjectScope();
      resetProjectScopedStores();
    }
    set({ currentProject });
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
    selectionEpoch += 1;
    invalidateProjectScope();
    resetProjectScopedStores();
    set({ currentProject: defaultProject, pendingAction: undefined });
  },
  setRecentProjects: (recentProjects) => set({ recentProjects }),
  setPendingAction: (pendingAction) => set({ pendingAction }),
  loadRecentProjects: async () => {
    if (!hasTauri()) {
      set({ recentProjects: [] });
      return [];
    }
    const projects = await invoke<RecentProject[]>("list_recent_projects");
    set({ recentProjects: projects, error: null });
    return projects;
  },
  createProject: async ({ rootPath, name, template }) => {
    const epoch = ++selectionEpoch;
    const summary = await invoke<ProjectSummary>("create_project", {
      request: { rootPath, name, template: template ?? "general" },
    });
    if (epoch === selectionEpoch) {
      invalidateProjectScope();
      resetProjectScopedStores();
      set({ currentProject: summary, pendingAction: undefined, error: null });
    }
    return summary;
  },
  openProject: async (path) => {
    if (!hasTauri()) {
      return { kind: "opened" as const, summary: undefined, pendingAction: undefined };
    }
    const epoch = ++selectionEpoch;
    const response = await invoke<OpenProjectResponse>("open_project", { request: { path } });
    if (epoch !== selectionEpoch) {
      return response;
    }
    if (response.summary) {
      invalidateProjectScope();
      resetProjectScopedStores();
      set({ currentProject: response.summary, error: null });
    }
    set({ pendingAction: response.pendingAction });
    return response;
  },
  confirmPendingAction: async () => {
    const action = get().pendingAction;
    if (!action) {
      return undefined;
    }
    const requestEpoch = selectionEpoch;
    const requestProject = get().currentProject;
    if (!hasTauri()) {
      if (
        requestEpoch === selectionEpoch &&
        get().pendingAction?.id === action.id
      ) {
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
      set({
        currentProject: confirmed.projectSummary ?? state.currentProject,
        pendingAction: undefined,
      });
    }
    return confirmed;
  },
  cancelPendingAction: async () => {
    const action = get().pendingAction;
    const requestEpoch = selectionEpoch;
    const requestProject = get().currentProject;
    if (action && hasTauri()) {
      await invoke<ConfirmedAction>("confirm_pending_action", {
        request: { actionId: action.id, status: "cancelled" },
      });
    }
    const state = get();
    if (
      requestEpoch === selectionEpoch &&
      state.currentProject.projectId === requestProject.projectId &&
      state.currentProject.rootPath === requestProject.rootPath &&
      state.pendingAction?.id === action?.id
    ) {
      set({ pendingAction: undefined });
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
      const last = recentProjects.find((project) => !project.missing);
      if (last && bootstrapEpoch === selectionEpoch) {
        await get().openProject(last.rootPath);
      }
      set({ initializing: false, initialized: true });
    } catch (error) {
      set({
        currentProject: defaultProject,
        initializing: false,
        initialized: true,
        error: errorMessage(error),
      });
    }
  },
}));
