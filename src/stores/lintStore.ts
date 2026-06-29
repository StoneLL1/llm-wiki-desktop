import { invoke } from "@tauri-apps/api/core";
import { create } from "zustand";

import type {
  AddLintIgnoreRequest,
  ApplyLintFixRequest,
  ApplyLintFixesBatchRequest,
  DeepLintReport,
  GetDeepLintReportRequest,
  LintBatchConfirmation,
  LintBatchOutcome,
  LintFixConfirmRequest,
  LintFixOutcome,
  LintIgnoreEntry,
  LintIssue,
  LintMode,
  LintReport,
  LintRoutePreference,
  LintSafetyPrefs,
  ListLintIgnoresRequest,
  StartDeepLintRequest,
} from "../types/lint";
import type { AgentKind } from "../types/agent";
import type { LlmProviderKind } from "../types/llm";
import { captureProjectScope, isProjectScopeCurrent } from "./projectScope";

const hasTauri = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

function errorMessage(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error) {
    const message = (error as { message: unknown }).message;
    if (typeof message === "string") return message;
  }
  return String(error);
}

const SAFETY_PREFS_KEY = "llm-wiki-desktop.lintSafetyPrefs";

const DEFAULT_SAFETY_PREFS: LintSafetyPrefs = {
  checkpoint: true,
  commitAfter: true,
  recompile: false,
};

function loadSafetyPrefs(): LintSafetyPrefs {
  try {
    const raw = window.localStorage.getItem(SAFETY_PREFS_KEY);
    if (!raw) return { ...DEFAULT_SAFETY_PREFS };
    const parsed = JSON.parse(raw) as Partial<LintSafetyPrefs>;
    return {
      checkpoint: true, // hard boundary; always on regardless of stored value
      commitAfter: parsed.commitAfter ?? true,
      recompile: parsed.recompile ?? false,
    };
  } catch {
    return { ...DEFAULT_SAFETY_PREFS };
  }
}

function saveSafetyPrefs(prefs: LintSafetyPrefs): void {
  try {
    window.localStorage.setItem(SAFETY_PREFS_KEY, JSON.stringify(prefs));
  } catch {
    /* ignore persistence failures */
  }
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
  /** View-mode filter for the list and summary cards. */
  mode: LintMode;
  /** High-risk confirmations collected by the last batch run, awaiting review. */
  batchConfirmations: LintBatchConfirmation[];
  /** "idle" | "running" after a batch auto-fix CTA. */
  batchRunning: boolean;
  /** Persisted ignore entries loaded from .app/lint-ignore.json. */
  ignores: LintIgnoreEntry[];
  /** UI-side safety preferences (checkpoint is a hard boundary, always on). */
  safetyPrefs: LintSafetyPrefs;

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
  setMode: (mode: LintMode) => void;
  setSafetyPrefs: (prefs: Partial<LintSafetyPrefs>) => void;
  loadIgnores: (request: ListLintIgnoresRequest) => Promise<void>;
  addIgnore: (request: AddLintIgnoreRequest) => Promise<boolean>;
  applyFix: (
    projectId: string,
    rootPath: string,
    issue: LintIssue,
    expectedHash?: string | null,
  ) => Promise<LintFixOutcome | null>;
  applyFixesBatch: (
    request: ApplyLintFixesBatchRequest,
  ) => Promise<LintBatchOutcome | null>;
  /** Promote one batched high-risk confirmation into the inline confirm flow. */
  openBatchConfirmation: (issueId: string) => void;
  confirmHighRisk: (
    projectId: string,
    rootPath: string,
    expectedHash: string,
  ) => Promise<LintFixOutcome | null>;
  cancelHighRisk: () => Promise<void>;
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
  mode: "all" as LintMode,
  batchConfirmations: [] as LintBatchConfirmation[],
  batchRunning: false,
  ignores: [] as LintIgnoreEntry[],
  safetyPrefs: loadSafetyPrefs(),
};

export const useLintStore = create<LintState>((set, get) => ({
  ...initial,

  runLocalLint: async (projectId, rootPath) => {
    if (!hasTauri()) return;
    const scope = captureProjectScope();
    set({ loadingLocal: true, error: null, fixStatus: {} });
    try {
      const report = await invoke<LintReport>("run_local_lint", {
        request: { projectId, projectRootPath: rootPath },
      });
      if (!isProjectScopeCurrent(scope)) return;
      set({ localReport: report, loadingLocal: false });
    } catch (error) {
      if (!isProjectScopeCurrent(scope)) return;
      set({ loadingLocal: false, error: errorMessage(error) });
    }
  },

  startDeepLint: async (projectId, rootPath, route, agent, provider) => {
    if (!hasTauri()) return null;
    const scope = captureProjectScope();
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
      if (!isProjectScopeCurrent(scope)) return null;
      set({ deepTaskId: task.id, runningDeep: true, deepReport: null });
      return task.id;
    } catch (error) {
      if (!isProjectScopeCurrent(scope)) return null;
      set({ error: errorMessage(error) });
      return null;
    }
  },

  clearDeepTask: () => set({ deepTaskId: null, runningDeep: false }),

  loadDeepReport: async (request) => {
    if (!hasTauri()) return;
    const scope = captureProjectScope();
    try {
      const report = await invoke<DeepLintReport>("get_deep_lint_report", { request });
      if (!isProjectScopeCurrent(scope)) return;
      set({ deepReport: report, runningDeep: false });
    } catch (error) {
      if (!isProjectScopeCurrent(scope)) return;
      set({ runningDeep: false, error: errorMessage(error) });
    }
  },

  selectIssue: (issueId) => set({ selectedIssueId: issueId }),

  setMode: (mode) => set({ mode }),

  setSafetyPrefs: (prefs) =>
    set((state) => {
      // checkpoint is a hard boundary — always on, never stored as off.
      const next: LintSafetyPrefs = {
        ...state.safetyPrefs,
        ...prefs,
        checkpoint: true,
        commitAfter: prefs.commitAfter ?? state.safetyPrefs.commitAfter,
      };
      saveSafetyPrefs(next);
      return { safetyPrefs: next };
    }),

  loadIgnores: async (request) => {
    if (!hasTauri()) return;
    const scope = captureProjectScope();
    try {
      const file = await invoke<{ ignored: LintIgnoreEntry[] }>(
        "list_lint_ignores",
        { request },
      );
      if (!isProjectScopeCurrent(scope)) return;
      set({ ignores: file.ignored ?? [] });
    } catch (error) {
      if (!isProjectScopeCurrent(scope)) return;
      set({ error: errorMessage(error) });
    }
  },

  addIgnore: async (request) => {
    if (!hasTauri()) return false;
    const scope = captureProjectScope();
    try {
      const file = await invoke<{ ignored: LintIgnoreEntry[] }>(
        "add_lint_ignore",
        { request },
      );
      if (!isProjectScopeCurrent(scope)) return false;
      set({ ignores: file.ignored ?? [] });
      return true;
    } catch (error) {
      if (!isProjectScopeCurrent(scope)) return false;
      set({ error: errorMessage(error) });
      return false;
    }
  },

  applyFix: async (projectId, rootPath, issue, expectedHash = null) => {
    if (!hasTauri()) return null;
    const scope = captureProjectScope();
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
      // Safe fixes (missing frontmatter) require the page hash as an
      // optimistic-lock baseline; high-risk fixes ignore it (they go through
      // the confirm flow). The view resolves the hash for safe fixes.
      expectedHash,
      actionId: null,
    };
    try {
      const outcome = await invoke<LintFixOutcome>("apply_lint_fix", { request });
      if (!isProjectScopeCurrent(scope)) return null;
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
      if (!isProjectScopeCurrent(scope)) return null;
      set((state) => ({
        fixStatus: { ...state.fixStatus, [issue.id]: "error" },
        error: errorMessage(error),
      }));
      return null;
    }
  },

  applyFixesBatch: async (request) => {
    if (!hasTauri()) return null;
    const scope = captureProjectScope();
    set({ batchRunning: true, error: null });
    try {
      const outcome = await invoke<LintBatchOutcome>("apply_lint_fixes", {
        request,
      });
      if (!isProjectScopeCurrent(scope)) return null;
      set({
        batchRunning: false,
        batchConfirmations: outcome.needsConfirmation ?? [],
      });
      return outcome;
    } catch (error) {
      if (!isProjectScopeCurrent(scope)) return null;
      set({ batchRunning: false, error: errorMessage(error) });
      return null;
    }
  },

  openBatchConfirmation: (issueId) => {
    const confirmation = get().batchConfirmations.find(
      (entry) => entry.issue.id === issueId,
    );
    if (!confirmation) return;
    set({
      selectedIssueId: issueId,
      fixConfirm: {
        issue: confirmation.issue,
        pendingAction: confirmation.pendingAction,
        expectedHash: "",
      },
    });
  },

  confirmHighRisk: async (projectId, rootPath, expectedHash) => {
    if (!hasTauri()) return null;
    const scope = captureProjectScope();
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
      actionId: confirm.pendingAction.id,
    };
    try {
      const outcome = await invoke<LintFixOutcome>("apply_lint_fix", { request });
      if (!isProjectScopeCurrent(scope)) return null;
      if (outcome.kind === "applied") {
        set((state) => ({
          fixStatus: { ...state.fixStatus, [issue.id]: "applied" },
          fixConfirm: null,
          batchConfirmations: state.batchConfirmations.filter(
            (entry) => entry.issue.id !== issue.id,
          ),
        }));
      } else {
        set((state) => ({ fixStatus: { ...state.fixStatus, [issue.id]: "idle" } }));
      }
      return outcome;
    } catch (error) {
      if (!isProjectScopeCurrent(scope)) return null;
      set((state) => ({
        fixStatus: { ...state.fixStatus, [issue.id]: "error" },
        fixConfirm: null,
        error: errorMessage(error),
      }));
      return null;
    }
  },

  cancelHighRisk: async () => {
    const confirm = get().fixConfirm;
    const actionId = confirm?.pendingAction.id;
    const issueId = confirm?.issue.id;
    set({ fixConfirm: null });
    if (issueId) {
      set((state) => ({
        batchConfirmations: state.batchConfirmations.filter(
          (entry) => entry.issue.id !== issueId,
        ),
      }));
    }
    if (!actionId || !hasTauri()) return;
    try {
      await invoke("confirm_pending_action", {
        request: { actionId, status: "cancelled" },
      });
    } catch (error) {
      set({ error: errorMessage(error) });
    }
  },

  reset: () => set({ ...initial }),
}));

/** All issues currently in view: local pass + the latest deep-lint report. */
export function selectAllIssues(state: LintState): LintIssue[] {
  const local = state.localReport?.issues ?? [];
  const deep = state.deepReport?.issues ?? [];
  return [...local, ...deep];
}
