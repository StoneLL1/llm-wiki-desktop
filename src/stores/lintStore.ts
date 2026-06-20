import { invoke } from "@tauri-apps/api/core";
import { create } from "zustand";

import type {
  ApplyLintFixRequest,
  DeepLintReport,
  GetDeepLintReportRequest,
  LintFixConfirmRequest,
  LintFixOutcome,
  LintIssue,
  LintReport,
  LintRoutePreference,
  StartDeepLintRequest,
} from "../types/lint";
import type { AgentKind } from "../types/agent";
import type { LlmProviderKind } from "../types/llm";

const hasTauri = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

function errorMessage(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error) {
    const message = (error as { message: unknown }).message;
    if (typeof message === "string") return message;
  }
  return String(error);
}

export interface LintState {
  localReport: LintReport | null;
  deepTaskId: string | null;
  deepReport: DeepLintReport | null;
  loadingLocal: boolean;
  runningDeep: boolean;
  error: string | null;
  selectedIssueId: string | null;
  /** Per-issue fix status keyed by issue id. */
  fixStatus: Record<string, "idle" | "applying" | "applied" | "error">;
  /** Inline high-risk confirm surfaced when a fix returns needs_confirmation. */
  fixConfirm: LintFixConfirmRequest | null;

  runLocalLint: (projectId: string, rootPath: string) => Promise<void>;
  startDeepLint: (
    projectId: string,
    rootPath: string,
    route: LintRoutePreference,
    agent?: AgentKind | null,
    provider?: LlmProviderKind | null,
  ) => Promise<string | null>;
  clearDeepTask: () => void;
  loadDeepReport: (request: GetDeepLintReportRequest) => Promise<void>;
  selectIssue: (issueId: string | null) => void;
  applyFix: (
    projectId: string,
    rootPath: string,
    issue: LintIssue,
  ) => Promise<LintFixOutcome | null>;
  confirmHighRisk: (
    projectId: string,
    rootPath: string,
    expectedHash: string,
  ) => Promise<LintFixOutcome | null>;
  cancelHighRisk: () => void;
  reset: () => void;
}

const initial = {
  localReport: null as LintReport | null,
  deepTaskId: null as string | null,
  deepReport: null as DeepLintReport | null,
  loadingLocal: false,
  runningDeep: false,
  error: null as string | null,
  selectedIssueId: null as string | null,
  fixStatus: {} as LintState["fixStatus"],
  fixConfirm: null as LintFixConfirmRequest | null,
};

export const useLintStore = create<LintState>((set, get) => ({
  ...initial,

  runLocalLint: async (projectId, rootPath) => {
    if (!hasTauri()) return;
    set({ loadingLocal: true, error: null, fixStatus: {} });
    try {
      const report = await invoke<LintReport>("run_local_lint", {
        request: { projectId, projectRootPath: rootPath },
      });
      set({ localReport: report, loadingLocal: false });
    } catch (error) {
      set({ loadingLocal: false, error: errorMessage(error) });
    }
  },

  startDeepLint: async (projectId, rootPath, route, agent, provider) => {
    if (!hasTauri()) return null;
    set({ error: null });
    try {
      const request: StartDeepLintRequest = {
        projectId,
        projectRootPath: rootPath,
        route,
        agent: agent ?? null,
        provider: provider ?? null,
      };
      const task = await invoke<{ id: string }>("start_deep_lint", { request });
      set({ deepTaskId: task.id, runningDeep: true, deepReport: null });
      return task.id;
    } catch (error) {
      set({ error: errorMessage(error) });
      return null;
    }
  },

  clearDeepTask: () => set({ deepTaskId: null, runningDeep: false }),

  loadDeepReport: async (request) => {
    if (!hasTauri()) return;
    try {
      const report = await invoke<DeepLintReport>("get_deep_lint_report", { request });
      set({ deepReport: report, runningDeep: false });
    } catch (error) {
      set({ runningDeep: false, error: errorMessage(error) });
    }
  },

  selectIssue: (issueId) => set({ selectedIssueId: issueId }),

  applyFix: async (projectId, rootPath, issue) => {
    if (!hasTauri()) return null;
    set((state) => ({
      fixStatus: { ...state.fixStatus, [issue.id]: "applying" },
      fixConfirm: null,
      error: null,
    }));
    const request: ApplyLintFixRequest = {
      projectId,
      projectRootPath: rootPath,
      issue,
      confirmHighRisk: false,
      expectedHash: null,
    };
    try {
      const outcome = await invoke<LintFixOutcome>("apply_lint_fix", { request });
      if (outcome.kind === "applied") {
        set((state) => ({
          fixStatus: { ...state.fixStatus, [issue.id]: "applied" },
          fixConfirm: null,
        }));
      } else if (outcome.pendingAction) {
        // High-risk fix needs an inline confirm. The expected hash is resolved
        // by the view from the live page before the user confirms.
        set({
          fixConfirm: { issue, pendingAction: outcome.pendingAction, expectedHash: "" },
        });
      }
      return outcome;
    } catch (error) {
      set((state) => ({
        fixStatus: { ...state.fixStatus, [issue.id]: "error" },
        error: errorMessage(error),
      }));
      return null;
    }
  },

  confirmHighRisk: async (projectId, rootPath, expectedHash) => {
    if (!hasTauri()) return null;
    const confirm = get().fixConfirm;
    if (!confirm) return null;
    const { issue } = confirm;
    set((state) => ({ fixStatus: { ...state.fixStatus, [issue.id]: "applying" } }));
    const request: ApplyLintFixRequest = {
      projectId,
      projectRootPath: rootPath,
      issue,
      confirmHighRisk: true,
      expectedHash,
    };
    try {
      const outcome = await invoke<LintFixOutcome>("apply_lint_fix", { request });
      if (outcome.kind === "applied") {
        set((state) => ({
          fixStatus: { ...state.fixStatus, [issue.id]: "applied" },
          fixConfirm: null,
        }));
      } else {
        set((state) => ({ fixStatus: { ...state.fixStatus, [issue.id]: "idle" } }));
      }
      return outcome;
    } catch (error) {
      set((state) => ({
        fixStatus: { ...state.fixStatus, [issue.id]: "error" },
        fixConfirm: null,
        error: errorMessage(error),
      }));
      return null;
    }
  },

  cancelHighRisk: () => set({ fixConfirm: null }),

  reset: () => set({ ...initial }),
}));

/** All issues currently in view: local pass + the latest deep-lint report. */
export function selectAllIssues(state: LintState): LintIssue[] {
  const local = state.localReport?.issues ?? [];
  const deep = state.deepReport?.issues ?? [];
  return [...local, ...deep];
}
