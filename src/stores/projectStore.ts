import { create } from "zustand";

export interface ProjectSummary {
  name: string;
  path: string;
  wikiPageCount: number;
  indexState: "indexed" | "stale" | "missing";
  agentRoute: "Agent" | "BYOK";
  byokProvider: string;
}

interface ProjectState {
  currentProject: ProjectSummary;
  setCurrentProject: (project: ProjectSummary) => void;
}

export const defaultProject: ProjectSummary = {
  name: "Agent Knowledge Base",
  path: "D:/Users/Aletta/Documents/wiki/agent-llm",
  wikiPageCount: 237,
  indexState: "indexed",
  agentRoute: "Agent",
  byokProvider: "Anthropic",
};

export const useProjectStore = create<ProjectState>((set) => ({
  currentProject: defaultProject,
  setCurrentProject: (currentProject) => set({ currentProject }),
}));
