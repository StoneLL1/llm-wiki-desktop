import { invoke } from "@tauri-apps/api/core";
import { create } from "zustand";

import type {
  AddLintIgnoreRequest,
  ApplyLintFixRequest,
  ApplyLintFixesBatchRequest,
  DeepLintReport,
  HealthCheckReport,
  GetDeepLintReportRequest,
  LintBatchConfirmation,
  LintBatchOutcome,
  LintFixConfirmRequest,
  LintFixOutcome,
  LintHistoryEntry,
  LintHistoryFile,
  LintIgnoreEntry,
  LintIssue,
  LintMode,
  ListLintHistoryRequest,
  LintReport,
  LintRoutePreference,
  LintSafetyPrefs,
  PersistedLintReport,
  ReadLintHistoryReportRequest,
  ListLintIgnoresRequest,
  RemoveLintIgnoreRequest,
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

function errorCode(error: unknown): string | null {
  if (typeof error === "object" && error !== null && "code" in error) {
    const code = (error as { code: unknown }).code;
    if (typeof code === "string") return code;
  }
  return null;
}

function isTerminalConfirmationError(error: unknown): boolean {
  return [
    "CONFIRMATION_EXPIRED",
    "CONFIRMATION_EXPIRY_INVALID",
    "CONFIRMATION_NOT_FOUND",
    "CONFIRMATION_REQUIRED",
    "CONFIRMATION_EXECUTION_MISSING",
    "CONFIRMATION_TYPE_MISMATCH",
    "LINT_FIX_HASH_REQUIRED",
    "LINT_FIX_SCAN_BASELINE_REQUIRED",
    "LINT_FIX_SCAN_BASELINE_MISMATCH",
    "LINT_FIX_STALE",
    "LINT_FIX_TYPE_REQUIRED",
  ].includes(
    errorCode(error) ?? "",
  );
}

async function cancelBackendActionBestEffort(actionId: string): Promise<void> {
  try {
    await invoke("confirm_pending_action", {
      request: { actionId, status: "cancelled" },
    });
  } catch {
    // The action may already be expired/cancelled; stale UI must never surface
    // a late confirmation as if it belonged to the current report.
  }
}

const SAFETY_PREFS_KEY = "llm-wiki-desktop.lintSafetyPrefs";

const DEFAULT_SAFETY_PREFS: LintSafetyPrefs = {
  checkpoint: true,
  commitAfter: true,
  recompile: false,
};

let lintOperationEpoch = 0;

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
  healthReport: HealthCheckReport | null;
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
  /** Persisted local/deep lint snapshots from .app/lint-history.json. */
  history: LintHistoryEntry[];
  historyLoading: boolean;
  historyError: string | null;
  activeHistoryId: string | null;

  runLocalLint: (
    projectId: string,
    rootPath: string,
    options?: { preserveBatchConfirmations?: boolean },
  ) => Promise<void>;
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
  loadHistory: (request: ListLintHistoryRequest) => Promise<LintHistoryEntry[]>;
  openHistoryReport: (
    request: ReadLintHistoryReportRequest,
    commitGuard?: () => boolean,
    preservePendingConfirmations?: boolean,
  ) => Promise<PersistedLintReport | null>;
  loadIgnores: (request: ListLintIgnoresRequest) => Promise<void>;
  addIgnore: (request: AddLintIgnoreRequest) => Promise<boolean>;
  removeIgnore: (request: RemoveLintIgnoreRequest) => Promise<boolean>;
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
  /** Best-effort cancellation used before project-scoped state is discarded. */
  cancelPendingActions: () => Promise<void>;
  reset: () => void;
}

const initial = {
  localReport: null as LintReport | null,
  deepTaskId: null as string | null,
  deepReport: null as DeepLintReport | null,
  healthReport: null as HealthCheckReport | null,
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
  history: [] as LintHistoryEntry[],
  historyLoading: false,
  historyError: null as string | null,
  activeHistoryId: null as string | null,
};

export const useLintStore = create<LintState>((set, get) => ({
  ...initial,

  runLocalLint: async (projectId, rootPath, options) => {
    if (!hasTauri()) return;
    const current = get();
    const preserveBatchConfirmations = options?.preserveBatchConfirmations === true;
    if (
      current.batchRunning ||
      current.fixConfirm ||
      (!preserveBatchConfirmations && current.batchConfirmations.length > 0) ||
      Object.values(current.fixStatus).some((status) => status === "applying")
    ) {
      return;
    }
    const operationEpoch = ++lintOperationEpoch;
    const scope = captureProjectScope();
    set({
      loadingLocal: true,
      error: null,
      localReport: null,
      healthReport: null,
      selectedIssueId: null,
      activeHistoryId: null,
      fixStatus: {},
      // Never discard an in-flight batch or confirmations explicitly preserved
      // by the batch completion rescan. Ordinary new scans invalidate old
      // confirmations so their scan hashes cannot outlive the report.
      ...(preserveBatchConfirmations
        ? {}
        : { fixConfirm: null, batchConfirmations: [], batchRunning: false }),
    });
    try {
      const report = await invoke<LintReport>("run_local_lint", {
        request: { projectId, projectRootPath: rootPath },
      });
      if (!isProjectScopeCurrent(scope) || operationEpoch !== lintOperationEpoch) return;
      set({
        localReport: report,
        activeHistoryId: null,
        loadingLocal: false,
      });
      void get().loadHistory({ projectId, projectRootPath: rootPath });
    } catch (error) {
      if (!isProjectScopeCurrent(scope) || operationEpoch !== lintOperationEpoch) return;
      set({ loadingLocal: false, error: errorMessage(error) });
    }
  },

  startDeepLint: async (projectId, rootPath, route, agent, provider) => {
    if (!hasTauri()) return null;
    if (get().runningDeep) return null;
    const scope = captureProjectScope();
    // Claim the running slot before the IPC round-trip so a double click
    // cannot enqueue two deep scans.
    set({ error: null, runningDeep: true, healthReport: null });
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
      set({ deepTaskId: task.id, deepReport: null });
      return task.id;
    } catch (error) {
      if (!isProjectScopeCurrent(scope)) return null;
      set({ runningDeep: false, error: errorMessage(error) });
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
      set({
        deepReport: report,
        healthReport: null,
        activeHistoryId: request.taskId,
        runningDeep: false,
      });
      void get().loadHistory({
        projectId: request.projectId,
        projectRootPath: request.projectRootPath,
      });
    } catch (error) {
      if (!isProjectScopeCurrent(scope)) return;
      set({ runningDeep: false, error: errorMessage(error) });
    }
  },

  selectIssue: (issueId) => {
    const activeConfirmation = get().fixConfirm;
    if (activeConfirmation && issueId !== activeConfirmation.issue.id) return;
    set({ selectedIssueId: issueId });
  },

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

  loadHistory: async (request) => {
    if (!hasTauri()) return [];
    const scope = captureProjectScope();
    set({ historyLoading: true, historyError: null });
    try {
      const file = await invoke<LintHistoryFile>("list_lint_history", { request });
      if (!isProjectScopeCurrent(scope)) return [];
      const history = file.entries ?? [];
      set({ history, historyLoading: false });
      return history;
    } catch (error) {
      if (!isProjectScopeCurrent(scope)) return [];
      set({ historyLoading: false, historyError: errorMessage(error) });
      return [];
    }
  },

  openHistoryReport: async (
    request,
    commitGuard = () => true,
    preservePendingConfirmations = false,
  ) => {
    if (!hasTauri()) return null;
    const current = get();
    if (
      current.loadingLocal ||
      current.batchRunning ||
      Object.values(current.fixStatus).some((status) => status === "applying")
    ) {
      set({ historyError: "lint.history.waitForFix" });
      return null;
    }
    const operationEpoch = ++lintOperationEpoch;
    const scope = captureProjectScope();
    const pendingActionIds = [
      ...(get().fixConfirm ? [get().fixConfirm!.pendingAction.id] : []),
      ...get().batchConfirmations.map((entry) => entry.pendingAction.id),
    ].filter((id, index, ids) => ids.indexOf(id) === index);
    if (preservePendingConfirmations && pendingActionIds.length > 0) return null;
    if (pendingActionIds.length > 0) {
      const cancelledActionIds: string[] = [];
      try {
        for (const actionId of pendingActionIds) {
          if (!commitGuard()) return null;
          try {
            await invoke("confirm_pending_action", {
              request: { actionId, status: "cancelled" },
            });
          } catch (error) {
            if (!isTerminalConfirmationError(error)) throw error;
          }
          if (!commitGuard()) return null;
          cancelledActionIds.push(actionId);
        }
      } catch (error) {
        if (!isProjectScopeCurrent(scope) || operationEpoch !== lintOperationEpoch || !commitGuard()) return null;
        set((state) => ({
          fixConfirm:
            state.fixConfirm && cancelledActionIds.includes(state.fixConfirm.pendingAction.id)
              ? null
              : state.fixConfirm,
          batchConfirmations: state.batchConfirmations.filter(
            (entry) => !cancelledActionIds.includes(entry.pendingAction.id),
          ),
        }));
        set({ historyError: errorMessage(error) });
        return null;
      }
      if (!isProjectScopeCurrent(scope) || operationEpoch !== lintOperationEpoch || !commitGuard()) return null;
      set({ fixConfirm: null, batchConfirmations: [] });
    }
    set({ historyError: null });
    try {
      const persisted = await invoke<PersistedLintReport>(
        "read_lint_history_report",
        { request },
      );
      if (!isProjectScopeCurrent(scope) || operationEpoch !== lintOperationEpoch || !commitGuard()) return null;
      set({
        localReport: persisted.localReport ?? null,
        deepReport: persisted.deepReport ?? null,
        healthReport: persisted.healthCheckReport ?? null,
        loadingLocal: false,
        selectedIssueId: null,
        fixStatus: {},
        fixConfirm: null,
        batchConfirmations: [],
        batchRunning: false,
        activeHistoryId: persisted.entry.id,
        mode:
          persisted.entry.kind === "local"
            ? "local"
            : persisted.entry.kind === "deep"
              ? "agent"
              : "all",
      });
      return persisted;
    } catch (error) {
      if (!isProjectScopeCurrent(scope) || operationEpoch !== lintOperationEpoch || !commitGuard()) return null;
      set({ historyError: errorMessage(error) });
      return null;
    }
  },

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

  removeIgnore: async (request) => {
    if (!hasTauri()) return false;
    const scope = captureProjectScope();
    try {
      const file = await invoke<{ ignored: LintIgnoreEntry[] }>(
        "remove_lint_ignore",
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
    const current = get();
    if (
      current.fixConfirm ||
      current.loadingLocal ||
      current.batchRunning ||
      current.batchConfirmations.length > 0 ||
      Object.values(current.fixStatus).some((status) => status === "applying")
    ) {
      return null;
    }
    const operationEpoch = ++lintOperationEpoch;
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
      if (!isProjectScopeCurrent(scope) || operationEpoch !== lintOperationEpoch) {
        if (outcome.pendingAction?.id) {
          void cancelBackendActionBestEffort(outcome.pendingAction.id);
        }
        return null;
      }
      if (outcome.kind === "applied") {
        set((state) => ({
          fixStatus: { ...state.fixStatus, [issue.id]: "applied" },
          fixConfirm: null,
        }));
      } else if (outcome.pendingAction) {
        // High-risk fix needs an inline confirm. The expected hash comes from
        // the immutable scan snapshot carried by the finding.
        const pendingAction = outcome.pendingAction;
        set((state) => ({
          fixStatus: { ...state.fixStatus, [issue.id]: "idle" },
          fixConfirm: {
            issue,
            pendingAction,
            expectedHash: issue.scanHash ?? "",
          },
        }));
      }
      return outcome;
    } catch (error) {
      if (!isProjectScopeCurrent(scope) || operationEpoch !== lintOperationEpoch) return null;
      if (isTerminalConfirmationError(error)) {
        set((state) => ({
          fixConfirm: null,
          batchConfirmations: state.batchConfirmations.filter(
            (entry) => entry.issue.id !== issue.id,
          ),
          fixStatus: { ...state.fixStatus, [issue.id]: "error" },
          error: errorMessage(error),
        }));
        return null;
      }
      set((state) => ({
        fixStatus: { ...state.fixStatus, [issue.id]: "error" },
        error: errorMessage(error),
      }));
      return null;
    }
  },

  applyFixesBatch: async (request) => {
    if (!hasTauri()) return null;
    const current = get();
    if (
      current.loadingLocal ||
      current.batchRunning ||
      current.fixConfirm ||
      current.batchConfirmations.length > 0 ||
      Object.values(current.fixStatus).some((status) => status === "applying")
    ) return null;
    const operationEpoch = ++lintOperationEpoch;
    const scope = captureProjectScope();
    set({ batchRunning: true, error: null });
    try {
      const outcome = await invoke<LintBatchOutcome>("apply_lint_fixes", {
        request,
      });
      if (!isProjectScopeCurrent(scope) || operationEpoch !== lintOperationEpoch) {
        for (const confirmation of outcome.needsConfirmation ?? []) {
          void cancelBackendActionBestEffort(confirmation.pendingAction.id);
        }
        return null;
      }
      set({
        batchRunning: false,
        batchConfirmations: outcome.needsConfirmation ?? [],
      });
      return outcome;
    } catch (error) {
      if (!isProjectScopeCurrent(scope) || operationEpoch !== lintOperationEpoch) return null;
      set({ batchRunning: false, error: errorMessage(error) });
      return null;
    }
  },

  openBatchConfirmation: (issueId) => {
    const activeConfirmation = get().fixConfirm;
    if (activeConfirmation && activeConfirmation.issue.id !== issueId) return;
    const confirmation = get().batchConfirmations.find(
      (entry) => entry.issue.id === issueId,
    );
    if (!confirmation) return;
    set({
      selectedIssueId: issueId,
      fixConfirm: {
        issue: confirmation.issue,
        pendingAction: confirmation.pendingAction,
        expectedHash: confirmation.issue.scanHash ?? "",
      },
    });
  },

  confirmHighRisk: async (projectId, rootPath, expectedHash) => {
    if (!hasTauri()) return null;
    const current = get();
    const activeConfirmation = current.fixConfirm;
    if (!activeConfirmation) return null;
    if (current.fixStatus[activeConfirmation.issue.id] === "applying") return null;
    const operationEpoch = ++lintOperationEpoch;
    const scope = captureProjectScope();
    const confirm = activeConfirmation;
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
      if (!isProjectScopeCurrent(scope) || operationEpoch !== lintOperationEpoch) return null;
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
      if (!isProjectScopeCurrent(scope) || operationEpoch !== lintOperationEpoch) return null;
      if (isTerminalConfirmationError(error)) {
        void cancelBackendActionBestEffort(confirm.pendingAction.id);
        set((state) => ({
          fixConfirm: null,
          batchConfirmations: state.batchConfirmations.filter(
            (entry) => entry.issue.id !== issue.id,
          ),
          fixStatus: { ...state.fixStatus, [issue.id]: "error" },
          error: errorMessage(error),
        }));
        return null;
      }
      set((state) => ({
        fixStatus: { ...state.fixStatus, [issue.id]: "error" },
        error: errorMessage(error),
      }));
      return null;
    }
  },

  cancelHighRisk: async () => {
    const confirm = get().fixConfirm;
    const actionId = confirm?.pendingAction.id;
    const issueId = confirm?.issue.id;
    if (!confirm) return;
    if (issueId && get().fixStatus[issueId] === "applying") return;
    if (issueId) {
      set((state) => ({
        fixStatus: { ...state.fixStatus, [issueId]: "applying" },
      }));
    }
    const clearConfirmation = () => {
      set((state) => ({
        fixConfirm: null,
        fixStatus: issueId
          ? { ...state.fixStatus, [issueId]: "idle" }
          : state.fixStatus,
        batchConfirmations: issueId
          ? state.batchConfirmations.filter((entry) => entry.issue.id !== issueId)
          : state.batchConfirmations,
      }));
    };
    if (!actionId || !hasTauri()) {
      clearConfirmation();
      return;
    }
    try {
      await invoke("confirm_pending_action", {
        request: { actionId, status: "cancelled" },
      });
      clearConfirmation();
    } catch (error) {
      // Keep the confirmation visible so a transient IPC failure can be retried;
      // the backend action must not become an orphaned, unreviewable request.
      if (isTerminalConfirmationError(error)) {
        clearConfirmation();
      } else if (issueId) {
        set((state) => ({
          fixStatus: { ...state.fixStatus, [issueId]: "idle" },
        }));
      }
      set({ error: errorMessage(error) });
    }
  },

  cancelPendingActions: async () => {
    if (!hasTauri()) return;
    const state = get();
    const actionIds = [
      ...(state.fixConfirm ? [state.fixConfirm.pendingAction.id] : []),
      ...state.batchConfirmations.map((entry) => entry.pendingAction.id),
    ].filter((id, index, ids) => ids.indexOf(id) === index);
    for (const actionId of actionIds) {
      try {
        await invoke("confirm_pending_action", {
          request: { actionId, status: "cancelled" },
        });
      } catch {
        // Teardown is best-effort. The registry also enforces expiry, and a
        // cancellation failure must not leak an old project's error into the
        // newly selected project after the synchronous store reset.
      }
    }
  },

  reset: () => set({ ...initial }),
}));

/** All issues currently in view: local pass + the latest deep-lint report. */
export function selectAllIssues(state: LintState): LintIssue[] {
  if (state.healthReport) return state.healthReport.issues;
  const local = state.localReport?.issues ?? [];
  const deep = state.deepReport?.issues ?? [];
  return [...local, ...deep];
}
