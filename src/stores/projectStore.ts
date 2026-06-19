import { invoke } from "@tauri-apps/api/core";
import { create } from "zustand";

import type {
  OpenProjectResponse,
  ProjectSummary,
  ProjectTemplate,
  RecentProject,
} from "../types/project";

export interface CreateProjectPayload {
  rootPath: string;
  name: string;
  template?: ProjectTemplate;
}

interface ProjectState {
  currentProject: ProjectSummary;
  recentProjects: RecentProject[];
  pendingAction: OpenProjectResponse["pendingAction"];
  setCurrentProject: (project: ProjectSummary) => void;
  setRecentProjects: (projects: RecentProject[]) => void;
  setPendingAction: (action: OpenProjectResponse["pendingAction"]) => void;
  loadRecentProjects: () => Promise<RecentProject[]>;
  createProject: (payload: CreateProjectPayload) => Promise<ProjectSummary>;
  openProject: (path: string) => Promise<OpenProjectResponse>;
}

const hasTauri = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export const defaultProject: ProjectSummary = {
  projectId: "sample-agent-knowledge-base",
  name: "Agent Knowledge Base",
  rootPath: "D:/Users/Aletta/Documents/wiki/agent-llm",
  template: "general",
  wikiPageCount: 237,
  sourceCount: 18,
  taskCount: 2,
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
};

export const defaultRecentProjects: RecentProject[] = [
  {
    projectId: "sample-agent-knowledge-base",
    name: "Agent Knowledge Base",
    rootPath: "D:/Users/Aletta/Documents/wiki/agent-llm",
    template: "general",
    openedAt: "2026-06-19T00:00:00Z",
  },
];

export const useProjectStore = create<ProjectState>((set) => ({
  currentProject: defaultProject,
  recentProjects: defaultRecentProjects,
  pendingAction: undefined,
  setCurrentProject: (currentProject) => set({ currentProject }),
  setRecentProjects: (recentProjects) => set({ recentProjects }),
  setPendingAction: (pendingAction) => set({ pendingAction }),
  loadRecentProjects: async () => {
    if (!hasTauri()) {
      return [];
    }
    const projects = await invoke<RecentProject[]>("list_recent_projects");
    set({ recentProjects: projects });
    return projects;
  },
  createProject: async ({ rootPath, name, template }) => {
    const summary = await invoke<ProjectSummary>("create_project", {
      request: { rootPath, name, template: template ?? "general" },
    });
    set({ currentProject: summary, pendingAction: undefined });
    return summary;
  },
  openProject: async (path) => {
    if (!hasTauri()) {
      return { kind: "opened" as const, summary: undefined, pendingAction: undefined };
    }
    const response = await invoke<OpenProjectResponse>("open_project", { request: { path } });
    if (response.summary) {
      set({ currentProject: response.summary });
    }
    set({ pendingAction: response.pendingAction });
    return response;
  },
}));
