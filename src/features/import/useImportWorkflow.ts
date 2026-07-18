import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import type { TaskLauncher } from "../../hooks/useTaskLauncher";
import { registerTaskEventListener } from "../../hooks/useTaskEvents";
import { importV2Api } from "../../services/importV2Api";
import { importProjectKey, useImportStore, type ImportQueueFilter } from "../../stores/importStore";
import { fetchTasks, useTaskStore } from "../../stores/taskStore";
import { useToastStore } from "../../stores/toastStore";
import type { ImportedSource } from "../../types/import";
import type { CommitItemDecision, ImportItem, ImportRecoveryAction, ImportSession } from "../../types/importV2";
import type { FileScanResult } from "../../types/importV2File";
import type { AgentKind } from "../../types/agent";
import type { LlmProviderKind } from "../../types/llm";
import type {
  AgentAssistancePolicy,
  AgentAssistanceTrigger,
  AgentCandidateActionResult,
  AgentCandidateView,
  AgentSendScope,
} from "../../types/importV2Agent";
import type {
  ConnectorSessionRef,
  ImportCapabilityRequirement,
  ImportFrontendReadiness,
  ImportPreviewContent,
} from "../../types/importV2Presentation";
import type { ProjectSummary } from "../../types/project";
import { isTerminalStatus, type BackendEvent, type BackendTask } from "../../types/task";
import type { ImportHistoryPage } from "../../types/importV2Presentation";
import type { MigrationConfirmation, LegacyInventory, MigrationPlan, MigrationStatusSnapshot } from "../../types/importV2Migration";
import { selectQueueCounts, selectSessionProgress, selectVisibleItems, type ImportQueueCounts, type ImportSessionProgress } from "./importViewModel";
import type { AppView } from "../../stores/navigationStore";

export type ImportBootstrapState = "loading" | "ready" | "blocked" | "error";

export interface ImportBatchTask {
  id: string;
  itemId: string;
  title: string;
  status: BackendTask["status"] | "unknown";
  cancellable: boolean;
}

export interface ImportBatchProgress {
  id: string;
  sessionId: string;
  total: number;
  taskIds: readonly string[];
  processed: number;
  active: number;
  completed: number;
  /** Number of child tasks waiting for any user action, including login. */
  waitingForConfirmation?: number;
  /** Subset of waiting tasks whose item is ready for preview/commit review. */
  reviewReady?: number;
  failed: number;
  cancelled: number;
  cancelling: number;
  unknown: number;
  nonCancellable: number;
  failedItemIds: readonly string[];
  tasks: readonly ImportBatchTask[];
}

interface ImportBatchTaskRef {
  taskId: string;
  itemId: string;
  title: string;
}

interface ImportBatchRecord {
  id: string;
  sessionId: string;
  projectKey: string;
  epoch: number;
  tasks: readonly ImportBatchTaskRef[];
}

export interface ImportWorkflow {
  session: ImportSession | null;
  readiness: ImportFrontendReadiness | null;
  /** Readiness is advisory; a warning must not prevent V2 staging. */
  readinessWarning?: string | null;
  /** Only session/project bootstrap failures block the import surface. */
  bootstrapError?: string | null;
  retryBootstrap?: () => void;
  bootstrapState: ImportBootstrapState;
  visibleItems: ImportItem[];
  counts: ImportQueueCounts;
  progress: ImportSessionProgress;
  discoveryTask?: BackendTask | null;
  discoveryScan?: FileScanResult | null;
  discoveryTaskUnavailable?: boolean;
  isAddingPaths?: boolean;
  isAddingUrl?: boolean;
  pendingItemIds?: ReadonlySet<string>;
  isSyncingSession?: boolean;
  batches?: readonly ImportBatchProgress[];
  batch?: ImportBatchProgress | null;
  isCancellingBatch?: boolean;
  isBatchCancelling?: (batchId: string) => boolean;
  selectedItemId: string | null;
  filter: ImportQueueFilter;
  addPaths: (paths: string[]) => Promise<void>;
  addUrl: (url: string) => Promise<void>;
  cancelDiscovery?: () => Promise<void>;
  dismissDiscovery?: () => void;
  cancelBatch?: (batchId?: string) => Promise<void>;
  dismissBatch?: (batchId?: string) => void;
  retryBatch?: (batchId: string) => Promise<void>;
  setItemSelected: (itemId: string, selected: boolean) => Promise<void>;
  startItems: (itemIds: readonly string[], recoveryAction?: ImportRecoveryAction | null) => Promise<void>;
  retryItem: (itemId: string, recoveryAction?: ImportRecoveryAction | null) => Promise<void>;
  cancelItem: (itemId: string) => Promise<void>;
  skipItem: (itemId: string) => Promise<void>;
  authorizeLocalAsr: (itemId: string) => Promise<void>;
  confirm: (decisions: CommitItemDecision[]) => Promise<void>;
  confirmLegacy: (options: { createCheckpoint: boolean; compileAfterImport: boolean }) => void;
  refreshSession: () => Promise<void>;
  selectItem: (itemId: string | null) => void;
  setFilter: (filter: ImportQueueFilter) => void;
  loadPreview: (identity: { sessionId: string; itemId: string; candidateId: string | null; historyBatchId?: string | null }) => Promise<ImportPreviewContent>;
  loadSession: (sessionId: string, historyBatchId?: string | null) => Promise<ImportSession | null>;

  getAgentPolicy: () => Promise<AgentAssistancePolicy | null>;
  setAgentPolicy: (policy: AgentAssistancePolicy, localAgentKind: AgentKind | null) => Promise<AgentAssistancePolicy | null>;
  invokeLocalAgent: (itemId: string, trigger: AgentAssistanceTrigger, agentKind: AgentKind) => Promise<BackendTask | null>;
  previewByokScope: (itemId: string, trigger: AgentAssistanceTrigger, provider: LlmProviderKind) => Promise<AgentSendScope | null>;
  approveByokAssistance: (request: {
    itemId: string;
    trigger: AgentAssistanceTrigger;
    provider: LlmProviderKind;
    model: string;
    approvalId: string;
    scopeSha256: string;
    acknowledgePossibleDuplicateCharge: boolean;
  }) => Promise<BackendTask | null>;
  acceptAgentCandidate: (itemId: string, taskId: string) => Promise<AgentCandidateView | null>;
  selectAgentCandidate: (request: {
    itemId: string;
    candidateId: string;
    mergedMarkdown: string | null;
    expectedCurrentWikiSha256: string | null;
  }) => Promise<AgentCandidateActionResult | null>;
  discardAgentCandidate: (itemId: string, candidateId: string) => Promise<AgentCandidateActionResult | null>;
  beginLogin: (itemId: string, platform: string) => Promise<ConnectorSessionRef | null>;
  completeLogin: (itemId: string, connectorSessionId: string) => Promise<ConnectorSessionRef | null>;
  revokeLogin: (connectorSessionId: string) => Promise<boolean>;
  authorizePrivateTarget: (itemId: string, url: string) => Promise<string | null>;
  getCapabilityRequirement: (itemId: string) => Promise<ImportCapabilityRequirement | null>;
  installCapability: (itemId: string, capabilityId: string) => Promise<BackendTask | null>;
  scanMigration: () => Promise<LegacyInventory | null>;
  planMigration: (inventory: LegacyInventory) => Promise<MigrationPlan | null>;
  applyMigration: (plan: MigrationPlan, confirmation: MigrationConfirmation) => Promise<BackendTask | null>;
  getMigrationStatus: () => Promise<MigrationStatusSnapshot | null>;
  resumeMigration: (plan: MigrationPlan, confirmation: MigrationConfirmation) => Promise<BackendTask | null>;
  listHistory: (cursor?: string | null) => Promise<ImportHistoryPage | null>;

  // Compatibility fields are retained only until ImportView is replaced in Task 10.
  importedSources: ImportedSource[];
  isConfirming: boolean;
  requestPreview: (paths: string[]) => void;
  requestClipboard: (content: string) => Promise<void>;
  requestUrl: (url: string) => Promise<void>;
  requestDeleteSource: (path: string) => Promise<void>;
  requestReplaceSource: (path: string, replacementPath: string) => Promise<void>;
}

const hasTauri = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

function errorMessage(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error) {
    const message = (error as { message: unknown }).message;
    if (typeof message === "string") {
      const code = "code" in error ? (error as { code: unknown }).code : null;
      return typeof code === "string" && code.length > 0 ? `${code}: ${message}` : message;
    }
  }
  return String(error);
}

function copyTextWithDomFallback(content: string): boolean {
  if (typeof document === "undefined" || typeof document.execCommand !== "function") return false;
  const textarea = document.createElement("textarea");
  textarea.value = content;
  textarea.setAttribute("readonly", "true");
  textarea.style.position = "fixed";
  textarea.style.opacity = "0";
  document.body.appendChild(textarea);
  textarea.select();
  try {
    return document.execCommand("copy");
  } finally {
    textarea.remove();
  }
}

function isTerminalTaskEvent(event: BackendEvent): boolean {
  return event.eventType === "task_completed" || event.eventType === "task_failed" || event.eventType === "task_cancelled";
}

function isWaitingTaskEvent(event: BackendEvent): boolean {
  return (event.eventType === "task_updated" || event.eventType === "confirmation_requested")
    && (event.payload as Partial<BackendTask> | null)?.status === "waiting_for_confirmation";
}

function isSettledImportTask(task: BackendTask): boolean {
  return isTerminalStatus(task.status) || task.status === "waiting_for_confirmation";
}

function isRecoverableImportItemStatus(status: ImportItem["status"]): boolean {
  return [
    "queued",
    "inspecting",
    "waiting_capability",
    "waiting_login",
    "waiting_log",
    "extracting",
    "validating",
    "committing",
    "paused",
  ].includes(status);
}

export function useImportWorkflow(
  project: ProjectSummary,
  activeView: AppView,
  taskLauncher: TaskLauncher,
): ImportWorkflow {
  const { t } = useTranslation();
  const projectId = project.projectId;
  const rootPath = project.rootPath;
  const projectKey = importProjectKey(projectId, rootPath);
  const latestProjectKey = useRef(projectKey);
  latestProjectKey.current = projectKey;
  const [readiness, setReadiness] = useState<ImportFrontendReadiness | null>(null);
  const [readinessWarning, setReadinessWarning] = useState<string | null>(null);
  const [bootstrapError, setBootstrapError] = useState<string | null>(null);
  const [bootstrapState, setBootstrapState] = useState<ImportBootstrapState>("loading");
  const [bootstrapAttempt, setBootstrapAttempt] = useState(0);
  const [isSyncingSession, setIsSyncingSession] = useState(false);
  const session = useImportStore((state) => state.session);
  const selectedItemId = useImportStore((state) => state.selectedItemId);
  const filter = useImportStore((state) => state.filter);
  const importedSources = useImportStore((state) => state.importedSources);
  const isConfirming = useImportStore((state) => state.isConfirming);
  const selectedTaskUpsert = useTaskStore((state) => state.upsertTask);
  const openTaskDrawer = useTaskStore((state) => state.openDrawer);
  const taskList = useTaskStore((state) => state.tasks);
  const tasksHydrated = useTaskStore((state) => state.tasksHydrated);
  const pushToast = useToastStore((state) => state.pushToast);
  const mutationKeys = useImportStore((state) => state.mutationKeys);
  const pendingPathTasks = useRef(new Map<string, { projectKey: string; epoch: number; existingItemIds: ReadonlySet<string>; mutationKey: string }>());
  const pendingItemTasks = useRef(new Map<string, { projectKey: string; epoch: number; itemIds: readonly string[] }>());
  const pendingConfirmationTasks = useRef(new Map<string, { projectKey: string; epoch: number }>());
  const pendingActionKeysRef = useRef(new Set<string>());
  const [pendingActionKeys, setPendingActionKeys] = useState<ReadonlySet<string>>(new Set());
  const [discoveryTaskId, setDiscoveryTaskId] = useState<string | null>(null);
  const [discoveryScan, setDiscoveryScan] = useState<FileScanResult | null>(null);
  const discoveryScanTaskIdRef = useRef<string | null>(null);
  const discoveryScanLoadingTaskIdRef = useRef<string | null>(null);
  const [batchRecords, setBatchRecords] = useState<readonly ImportBatchRecord[]>([]);
  const [cancellingBatchIds, setCancellingBatchIds] = useState<ReadonlySet<string>>(new Set());
  const [dismissedBatchIds, setDismissedBatchIds] = useState<ReadonlySet<string>>(new Set());
  const localBatchCounter = useRef(0);
  const initializedProjectKeyRef = useRef<string | null>(null);
  const refreshInFlight = useRef<{ scopeKey: string; promise: Promise<void> } | null>(null);
  const discoveryPollTimers = useRef(new Map<string, ReturnType<typeof setTimeout>>());
  const sessionMutationRevisionRef = useRef(0);
  const nextSessionMutationRevision = useCallback(() => {
    sessionMutationRevisionRef.current += 1;
    return sessionMutationRevisionRef.current;
  }, []);
  const retryBootstrap = useCallback(() => setBootstrapAttempt((attempt) => attempt + 1), []);

  const isScopeCurrent = useCallback(
    (requestKey: string, epoch: number, expectedSessionId?: string) =>
      latestProjectKey.current === requestKey &&
      useImportStore.getState().projectKey === requestKey &&
      useImportStore.getState().sessionEpoch === epoch &&
      (!expectedSessionId || useImportStore.getState().session?.sessionId === expectedSessionId),
    [],
  );

  const beginPendingItems = useCallback((itemIds: readonly string[], requestKey: string, epoch: number): string[] => {
    const accepted: string[] = [];
    for (const itemId of [...new Set(itemIds)]) {
      const key = `${requestKey}\0${epoch}\0${itemId}`;
      if (pendingActionKeysRef.current.has(key)) continue;
      pendingActionKeysRef.current.add(key);
      accepted.push(itemId);
    }
    if (accepted.length > 0) {
      setPendingActionKeys((current) => {
        const next = new Set(current);
        for (const itemId of accepted) next.add(`${requestKey}\0${epoch}\0${itemId}`);
        return next;
      });
    }
    return accepted;
  }, []);

  const endPendingItems = useCallback((itemIds: readonly string[], requestKey: string, epoch: number) => {
    const keys = [...new Set(itemIds)].map((itemId) => `${requestKey}\0${epoch}\0${itemId}`);
    for (const key of keys) pendingActionKeysRef.current.delete(key);
    if (keys.length > 0) {
      setPendingActionKeys((current) => {
        const next = new Set(current);
        for (const key of keys) next.delete(key);
        return next;
      });
    }
  }, []);

  const sessionEpoch = useImportStore((state) => state.sessionEpoch);
  const pendingItemIds = useMemo(() => {
    const prefix = `${projectKey}\0${sessionEpoch}\0`;
    const ids = new Set<string>();
    for (const key of pendingActionKeys) {
      if (key.startsWith(prefix)) ids.add(key.slice(prefix.length));
    }
    return ids;
  }, [pendingActionKeys, projectKey, sessionEpoch]);
  const discoveryTask = useMemo(
    () => discoveryTaskId ? taskList.find((task) => task.id === discoveryTaskId) ?? null : null,
    [discoveryTaskId, taskList],
  );
  const discoveryTaskUnavailable = Boolean(discoveryTaskId && tasksHydrated && !discoveryTask);
  const isAddingPaths = [...mutationKeys].some((key) => key.startsWith(`add-paths:${projectKey}:`));
  const isAddingUrl = [...mutationKeys].some((key) => key.startsWith(`add-url:${projectKey}:`));
  const batches = useMemo<readonly ImportBatchProgress[]>(() => {
    const taskById = new Map(taskList.map((task) => [task.id, task]));
    return batchRecords.map((record) => {
      let completed = 0;
      let waitingForConfirmation = 0;
      let reviewReady = 0;
      let failed = 0;
      let cancelled = 0;
      let cancelling = 0;
      let unknown = 0;
      let nonCancellable = 0;
      const failedItemIds: string[] = [];
      const tasks = record.tasks.map((reference) => {
        const task = taskById.get(reference.taskId);
        const status = task?.status ?? "unknown";
        if (status === "succeeded") completed += 1;
        if (status === "waiting_for_confirmation") {
          waitingForConfirmation += 1;
          if (session?.items.find((item) => item.itemId === reference.itemId)?.status === "preview_ready") reviewReady += 1;
        }
        if (status === "failed") {
          failed += 1;
          failedItemIds.push(reference.itemId);
        }
        if (status === "cancelled") cancelled += 1;
        if (status === "cancelling") cancelling += 1;
        if (status === "unknown") unknown += 1;
        if (task && !task.cancellable && !isTerminalStatus(task.status) && task.status !== "waiting_for_confirmation") nonCancellable += 1;
        return {
          id: reference.taskId,
          itemId: reference.itemId,
          title: task?.title ?? reference.title,
          status,
          cancellable: task?.cancellable ?? false,
        } satisfies ImportBatchTask;
      });
      const processed = completed + waitingForConfirmation + failed + cancelled;
      return {
        id: record.id,
        sessionId: record.sessionId,
        total: record.tasks.length,
        taskIds: record.tasks.map((task) => task.taskId),
        processed,
        active: Math.max(0, record.tasks.length - processed - unknown),
        completed,
        waitingForConfirmation,
        reviewReady,
        failed,
        cancelled,
        cancelling,
        unknown,
        nonCancellable,
        failedItemIds,
        tasks,
      };
    });
  }, [batchRecords, session, taskList]);
  const batch = batches[0] ?? null;
  const isCancellingBatch = cancellingBatchIds.size > 0;
  const isBatchCancelling = useCallback((batchId: string) => cancellingBatchIds.has(batchId), [cancellingBatchIds]);
  const nextLocalBatchId = useCallback(() => {
    localBatchCounter.current += 1;
    return `local:${Date.now()}:${localBatchCounter.current}`;
  }, []);
  const recordItemBatch = useCallback((tasks: readonly BackendTask[], itemIds: readonly string[], requestKey: string, epoch: number, sessionId: string) => {
    if (tasks.length === 0 || !isScopeCurrent(requestKey, epoch, sessionId)) return;
    const currentSession = useImportStore.getState().session;
    const itemById = new Map((currentSession?.items ?? []).map((item) => [item.itemId, item]));
    const batchId = tasks.find((task) => task.batchId)?.batchId ?? nextLocalBatchId();
    const taskRefs = tasks.map((task, index) => ({
      taskId: task.id,
      itemId: itemIds[index] ?? "",
      title: itemById.get(itemIds[index] ?? "")?.input.displayName ?? task.title,
    }));
    setBatchRecords((current) => [
      ...current.filter((record) => record.id !== batchId),
      { id: batchId, sessionId, projectKey: requestKey, epoch, tasks: taskRefs },
    ]);
    setCancellingBatchIds((current) => {
      if (!current.has(batchId)) return current;
      const next = new Set(current);
      next.delete(batchId);
      return next;
    });
  }, [isScopeCurrent, nextLocalBatchId]);

  const registerItemTasks = useCallback((tasks: readonly BackendTask[], itemIds: readonly string[], requestKey: string, epoch: number) => {
    const trackedIds = new Set<string>();
    const terminalIds: string[] = [];
    tasks.forEach((task, index) => {
      const itemId = itemIds[index];
      if (!itemId) return;
      trackedIds.add(itemId);
      if (isSettledImportTask(task)) {
        terminalIds.push(itemId);
      } else {
        pendingItemTasks.current.set(task.id, { projectKey: requestKey, epoch, itemIds: [itemId] });
      }
    });
    const untrackedIds = itemIds.filter((itemId) => !trackedIds.has(itemId));
    endPendingItems([...untrackedIds, ...terminalIds], requestKey, epoch);
    return { terminalIds };
  }, [endPendingItems]);

  const refreshForScope = useCallback(async (requestKey: string, epoch: number, expectedSessionId?: string) => {
    if (!isScopeCurrent(requestKey, epoch)) return;
    const currentSession = useImportStore.getState().session;
    const sessionId = expectedSessionId ?? currentSession?.sessionId;
    if (!sessionId) return;
    const scopeKey = `${requestKey}\0${epoch}\0${sessionId}`;
    if (refreshInFlight.current?.scopeKey === scopeKey) return refreshInFlight.current.promise;
    const refreshRevision = sessionMutationRevisionRef.current;
    let refreshAgain = false;
    setIsSyncingSession(true);
    const request = importV2Api.getSession({ projectId, projectRootPath: rootPath, sessionId });
    const refresh = request
      .then((nextSession) => {
        if (isScopeCurrent(requestKey, epoch) && sessionMutationRevisionRef.current === refreshRevision) {
          useImportStore.getState().replaceSession(requestKey, nextSession, epoch);
        } else if (isScopeCurrent(requestKey, epoch) && sessionMutationRevisionRef.current !== refreshRevision) {
          refreshAgain = true;
        }
      })
      .catch((error) => {
        if (isScopeCurrent(requestKey, epoch)) {
          pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
        }
        // A caller that depends on the refreshed session must not continue
        // against the stale in-memory snapshot after a failed request.
        throw error;
      });
    refreshInFlight.current = { scopeKey, promise: refresh };
    try {
      await refresh;
      if (refreshAgain && isScopeCurrent(requestKey, epoch, sessionId)) {
        // Do not let callers continue against the stale session returned by
        // the first request. Clear the in-flight marker before recursing so
        // the latest request is not mistaken for the current one.
        refreshInFlight.current = null;
        await refreshForScope(requestKey, epoch, sessionId);
      }
    } finally {
      if (refreshInFlight.current?.promise === refresh) {
        refreshInFlight.current = null;
        if (isScopeCurrent(requestKey, epoch, sessionId)) setIsSyncingSession(false);
      }
    }
  }, [isScopeCurrent, projectId, pushToast, rootPath, t]);

  const settleItemTask = useCallback((task: BackendTask): boolean => {
    const pending = pendingItemTasks.current.get(task.id);
    if (!pending || !isSettledImportTask(task)) return false;
    pendingItemTasks.current.delete(task.id);
    endPendingItems(pending.itemIds, pending.projectKey, pending.epoch);
    if (isScopeCurrent(pending.projectKey, pending.epoch)) {
      void refreshForScope(pending.projectKey, pending.epoch).catch(() => undefined);
    }
    return true;
  }, [endPendingItems, isScopeCurrent, refreshForScope]);

  const reconcilePendingItemTasks = useCallback((tasks: readonly BackendTask[] = useTaskStore.getState().tasks) => {
    for (const task of tasks) settleItemTask(task);
  }, [settleItemTask]);

  const loadDiscoveryScan = useCallback(async (taskId: string, requestKey: string, epoch: number, sessionId: string) => {
    if (discoveryScanLoadingTaskIdRef.current === taskId) return;
    discoveryScanLoadingTaskIdRef.current = taskId;
    try {
      const scan = await importV2Api.getScanResult({
        projectId,
        projectRootPath: rootPath,
        sessionId,
        taskId,
      });
      if (isScopeCurrent(requestKey, epoch) && discoveryScanTaskIdRef.current === taskId) {
        setDiscoveryScan(scan);
      }
    } catch {
      // Scan details are supplementary; older sessions may not have an
      // artifact even though the task/session reconciliation succeeded.
    } finally {
      if (discoveryScanLoadingTaskIdRef.current === taskId) discoveryScanLoadingTaskIdRef.current = null;
    }
  }, [isScopeCurrent, projectId, rootPath]);

  useEffect(() => {
    if (discoveryTask?.status !== "succeeded" || !session?.sessionId) return;
    if (discoveryScanTaskIdRef.current === discoveryTask.id && discoveryScan) return;
    discoveryScanTaskIdRef.current = discoveryTask.id;
    void loadDiscoveryScan(discoveryTask.id, projectKey, sessionEpoch, session.sessionId);
  }, [discoveryScan, discoveryTask?.id, discoveryTask?.status, loadDiscoveryScan, projectKey, session?.sessionId, sessionEpoch]);

  const startNewQueuedItems = useCallback(async (requestKey: string, epoch: number, before: ReadonlySet<string>) => {
    if (!isScopeCurrent(requestKey, epoch)) return;
    const current = useImportStore.getState().session;
    if (!current) return;
    const itemIds = current.items
      .filter((item) => !before.has(item.itemId) && item.status === "queued")
      .map((item) => item.itemId);
    if (itemIds.length === 0) return;
    const acceptedIds = beginPendingItems(itemIds, requestKey, epoch);
    if (acceptedIds.length === 0) return;
    nextSessionMutationRevision();
    try {
      const tasks = await importV2Api.startItems({
        projectId,
        projectRootPath: rootPath,
        sessionId: current.sessionId,
        itemIds: acceptedIds,
      });
      tasks.forEach((task) => selectedTaskUpsert(task));
      const resolvedTasks = tasks.map((task) => useTaskStore.getState().tasks.find((current) => current.id === task.id) ?? task);
      recordItemBatch(resolvedTasks, acceptedIds, requestKey, epoch, current.sessionId);
      const { terminalIds } = registerItemTasks(resolvedTasks, acceptedIds, requestKey, epoch);
      if (terminalIds.length > 0 && isScopeCurrent(requestKey, epoch)) {
        void refreshForScope(requestKey, epoch).catch(() => undefined);
      }
      void fetchTasks().then(() => reconcilePendingItemTasks()).catch(() => undefined);
    } catch (error) {
      if (isScopeCurrent(requestKey, epoch)) pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
      endPendingItems(acceptedIds, requestKey, epoch);
    }
  }, [beginPendingItems, endPendingItems, isScopeCurrent, nextSessionMutationRevision, projectId, pushToast, recordItemBatch, reconcilePendingItemTasks, refreshForScope, registerItemTasks, rootPath, selectedTaskUpsert, t]);

  const settlePathTask = useCallback((task: BackendTask): boolean => {
    const pending = pendingPathTasks.current.get(task.id);
    if (!pending || !isSettledImportTask(task)) return false;
    pendingPathTasks.current.delete(task.id);
    const timer = discoveryPollTimers.current.get(task.id);
    if (timer) {
      clearTimeout(timer);
      discoveryPollTimers.current.delete(task.id);
    }
    useImportStore.getState().endMutation(pending.mutationKey);
    if (task.status === "succeeded") {
      void refreshForScope(pending.projectKey, pending.epoch).then(() =>
        startNewQueuedItems(pending.projectKey, pending.epoch, pending.existingItemIds),
      ).catch(() => undefined);
      const sessionId = useImportStore.getState().session?.sessionId;
      if (sessionId) void loadDiscoveryScan(task.id, pending.projectKey, pending.epoch, sessionId);
    }
    return true;
  }, [loadDiscoveryScan, refreshForScope, startNewQueuedItems]);

  const reconcilePendingTasks = useCallback((tasks: readonly BackendTask[] = useTaskStore.getState().tasks) => {
    const byId = new Map(tasks.map((task) => [task.id, task]));
    for (const taskId of pendingPathTasks.current.keys()) {
      const task = byId.get(taskId);
      if (task) settlePathTask(task);
    }
    for (const taskId of pendingItemTasks.current.keys()) {
      const task = byId.get(taskId);
      if (task) settleItemTask(task);
    }
  }, [settleItemTask, settlePathTask]);

  const watchDiscoveryTask = useCallback((taskId: string, requestKey: string, epoch: number) => {
    let attempts = 0;
    const poll = async () => {
      if (!isScopeCurrent(requestKey, epoch) || !pendingPathTasks.current.has(taskId)) {
        discoveryPollTimers.current.delete(taskId);
        return;
      }
      attempts += 1;
      try {
        // Tauri events are the low-latency path. The task snapshot is the
        // recovery path for a completion that happened while the listener was
        // reconnecting or before the IPC creation response returned.
        await fetchTasks();
        reconcilePendingTasks();
      } catch {
        // A task event can still settle the operation; do not turn a failed
        // observability poll into a second import error.
      }
      if (!pendingPathTasks.current.has(taskId) || !isScopeCurrent(requestKey, epoch) || attempts >= 120) {
        discoveryPollTimers.current.delete(taskId);
        return;
      }
      discoveryPollTimers.current.set(taskId, setTimeout(() => { void poll(); }, 250));
    };
    void poll();
  }, [isScopeCurrent, reconcilePendingTasks]);

  useEffect(() => () => {
    for (const timer of discoveryPollTimers.current.values()) clearTimeout(timer);
    discoveryPollTimers.current.clear();
  }, []);

  useEffect(() => {
    reconcilePendingTasks(taskList);
  }, [reconcilePendingTasks, taskList]);

  const handleTaskEvent = useCallback((event: BackendEvent) => {
    if ((!isTerminalTaskEvent(event) && !isWaitingTaskEvent(event)) || !event.taskId) return;
    settlePathTask(event.payload as BackendTask);
    const confirmation = pendingConfirmationTasks.current.get(event.taskId);
    if (confirmation) {
      pendingConfirmationTasks.current.delete(event.taskId);
      const task = event.payload as BackendTask;
      if (isScopeCurrent(confirmation.projectKey, confirmation.epoch)) {
        useImportStore.getState().setIsConfirming(false);
        void refreshForScope(confirmation.projectKey, confirmation.epoch).catch(() => undefined);
        if (task.status === "failed") {
          pushToast("error", t("importV2.workflow.confirmFailed"));
        }
      }
    }
    settleItemTask(event.payload as BackendTask);
    const current = useImportStore.getState();
    if (event.projectId !== projectId || current.projectKey !== projectKey || !current.session) return;
    if (current.session.items.some((item) => item.taskId === event.taskId)) {
      void refreshForScope(projectKey, current.sessionEpoch).catch(() => undefined);
    }
  }, [isScopeCurrent, projectId, projectKey, pushToast, refreshForScope, settleItemTask, settlePathTask, t]);

  useEffect(() => registerTaskEventListener(handleTaskEvent), [handleTaskEvent]);

  useEffect(() => {
    if (!projectId) {
      setBootstrapState("ready");
      return;
    }
    if (activeView !== "import") {
      // Keep the import session, pending work, and batch task ids alive while
      // the user visits another workspace view. The task event listener stays
      // mounted, so returning to Import can show the latest state immediately.
      setBootstrapState("ready");
      return;
    }
    const currentStore = useImportStore.getState();
    if (currentStore.projectKey === projectKey && currentStore.session?.projectId === projectId) {
      // A hook remount can reuse the Zustand session while its local task id
      // state starts empty. Reattach once for a fresh hook instance, but do
      // not resurrect a status the user dismissed during ordinary navigation.
      if (initializedProjectKeyRef.current !== projectKey) {
        setDiscoveryTaskId(currentStore.session.discoveryTaskId ?? null);
      }
      initializedProjectKeyRef.current = projectKey;
      setBootstrapState("ready");
      return;
    }
    const store = useImportStore.getState();
    store.resetProjectPresentation(projectKey);
    store.setImportedSources([]);
    const epoch = store.beginSessionEpoch(projectKey);
    setReadiness(null);
    setReadinessWarning(null);
    setBootstrapError(null);
    setIsSyncingSession(false);
    setBootstrapState("loading");
    pendingPathTasks.current.clear();
    pendingItemTasks.current.clear();
    pendingConfirmationTasks.current.clear();
    pendingActionKeysRef.current.clear();
    setPendingActionKeys(new Set());
    setDiscoveryTaskId(null);
    discoveryScanTaskIdRef.current = null;
    discoveryScanLoadingTaskIdRef.current = null;
    setDiscoveryScan(null);
    setBatchRecords([]);
    setCancellingBatchIds(new Set());
    setDismissedBatchIds(new Set());
    if (activeView !== "import" || !projectId) {
      setBootstrapState("ready");
      return;
    }
    if (!hasTauri()) {
      setBootstrapState("blocked");
      return;
    }
    let cancelled = false;
    void (async () => {
      try {
        // Migration and activation metadata describe project history. They are
        // not a prerequisite for creating a new V2 session. Older projects can
        // have no report yet, and a damaged optional report must not hide the
        // current V2 import path.
        let nextReadiness: ImportFrontendReadiness | null = null;
        try {
          nextReadiness = await importV2Api.getReadiness({ projectId, projectRootPath: rootPath });
        } catch (error) {
          if (cancelled || !isScopeCurrent(projectKey, epoch)) return;
          setReadinessWarning(errorMessage(error));
        }
        if (cancelled || !isScopeCurrent(projectKey, epoch)) return;
        setReadiness(nextReadiness);

        let nextSession: ImportSession;
        if (nextReadiness?.unfinishedSessionId) {
          try {
            nextSession = await importV2Api.getSession({ projectId, projectRootPath: rootPath, sessionId: nextReadiness.unfinishedSessionId });
          } catch (error) {
            // A stale/corrupt unfinished record must not prevent a fresh V2
            // session. The old record remains untouched for later inspection.
            if (cancelled || !isScopeCurrent(projectKey, epoch)) return;
            setReadinessWarning(errorMessage(error));
            nextSession = await importV2Api.createSession({ projectId, projectRootPath: rootPath, resourceMode: "balanced" });
          }
        } else {
          nextSession = await importV2Api.createSession({ projectId, projectRootPath: rootPath, resourceMode: "balanced" });
        }
        if (cancelled || !isScopeCurrent(projectKey, epoch)) return;
        store.attachSession(projectKey, nextSession, epoch);
        setDiscoveryTaskId(nextSession.discoveryTaskId ?? null);
        initializedProjectKeyRef.current = projectKey;
        setBootstrapState("ready");
      } catch (error) {
        if (cancelled || !isScopeCurrent(projectKey, epoch)) return;
        setBootstrapError(errorMessage(error));
        setBootstrapState("error");
        pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [activeView, bootstrapAttempt, isScopeCurrent, projectId, projectKey, pushToast, rootPath, t]);

  // Rebuild visible batch cards from persisted session/task identities after
  // a remount or application restart. Only unfinished item work is restored;
  // completed historical batches remain available through History instead of
  // cluttering the live import surface.
  useEffect(() => {
    if (!session || session.projectId !== projectId) return;
    const hasReferencedTaskSnapshot = session.items.some((item) => item.taskId && taskList.some((task) => task.id === item.taskId));
    if (hasTauri() && !tasksHydrated && !hasReferencedTaskSnapshot) return;
    const taskById = new Map(taskList.map((task) => [task.id, task]));
    const recovered = new Map<string, ImportBatchRecord>();
    session.items.forEach((item) => {
      if (!item.taskId || !isRecoverableImportItemStatus(item.status)) return;
      const task = taskById.get(item.taskId);
      if (task && isTerminalStatus(task.status)) return;
      const batchId = task?.batchId ?? `recovered:${session.sessionId}:${item.taskId}`;
      const current = recovered.get(batchId) ?? {
        id: batchId,
        sessionId: session.sessionId,
        projectKey,
        epoch: sessionEpoch,
        tasks: [],
      };
      if (!current.tasks.some((reference) => reference.taskId === item.taskId)) {
        recovered.set(batchId, {
          ...current,
          tasks: [...current.tasks, {
            taskId: item.taskId,
            itemId: item.itemId,
            title: item.input.displayName,
          }],
        });
      }
    });
    if (recovered.size === 0) return;
    setBatchRecords((current) => {
      const existing = new Set(current.map((record) => record.id));
      const additions = [...recovered.values()].filter((record) => !existing.has(record.id) && !dismissedBatchIds.has(record.id));
      return additions.length > 0 ? [...current, ...additions] : current;
    });
  }, [dismissedBatchIds, projectId, projectKey, session, sessionEpoch, taskList, tasksHydrated]);

  const addPaths = useCallback(async (paths: string[]) => {
    const sourcePaths = paths.map((path) => path.trim()).filter(Boolean);
    const current = useImportStore.getState();
    if (sourcePaths.length === 0 || !current.session || current.projectKey !== projectKey) return;
    const epoch = current.sessionEpoch;
    const existingItemIds = new Set(current.session.items.map((item) => item.itemId));
    const mutationKey = `add-paths:${projectKey}:${epoch}`;
    if (current.mutationKeys.has(mutationKey) || current.mutationKeys.has(`add-url:${projectKey}:${epoch}`)) return;
    nextSessionMutationRevision();
    current.beginMutation(mutationKey);
    let taskStarted = false;
    try {
      const task = await importV2Api.addPaths({
        projectId,
        projectRootPath: rootPath,
        sessionId: current.session!.sessionId,
        sourcePaths,
      });
      // The global task listener may have received a terminal event while
      // this IPC call was still resolving. Preserve that newer fact instead
      // of overwriting it with the task snapshot returned at creation time.
      const knownTask = useTaskStore.getState().tasks.find((candidate) => candidate.id === task.id);
      const observedTask = knownTask && isSettledImportTask(knownTask) ? knownTask : task;
      selectedTaskUpsert(observedTask);
      pendingPathTasks.current.set(task.id, { projectKey, epoch, existingItemIds, mutationKey });
      taskStarted = true;
      if (isScopeCurrent(projectKey, epoch)) {
        setDiscoveryTaskId(task.id);
        discoveryScanTaskIdRef.current = task.id;
        discoveryScanLoadingTaskIdRef.current = null;
        setDiscoveryScan(null);
      }
      // A very fast local scan can finish before the task event subscription
      // observes it. Resolve from both the returned task and the persisted
      // task snapshot so a source cannot stay busy waiting for a missed event.
      settlePathTask(observedTask);
      watchDiscoveryTask(task.id, projectKey, epoch);
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch)) pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
      throw error;
    } finally {
      if (!taskStarted) useImportStore.getState().endMutation(mutationKey);
    }
  }, [fetchTasks, isScopeCurrent, nextSessionMutationRevision, projectId, projectKey, pushToast, reconcilePendingTasks, rootPath, selectedTaskUpsert, settlePathTask, t, watchDiscoveryTask]);

  const addUrl = useCallback(async (url: string) => {
    const current = useImportStore.getState();
    if (!url.trim() || !current.session || current.projectKey !== projectKey) return;
    const epoch = current.sessionEpoch;
    const existingItemIds = new Set(current.session.items.map((item) => item.itemId));
    const mutationKey = `add-url:${projectKey}:${epoch}`;
    if (current.mutationKeys.has(mutationKey) || current.mutationKeys.has(`add-paths:${projectKey}:${epoch}`)) return;
    const mutationRevision = nextSessionMutationRevision();
    current.beginMutation(mutationKey);
    try {
      const nextSession = await importV2Api.addUrl({
        projectId,
        projectRootPath: rootPath,
        sessionId: current.session.sessionId,
        url: url.trim(),
      });
      if (!isScopeCurrent(projectKey, epoch) || sessionMutationRevisionRef.current !== mutationRevision) return;
      useImportStore.getState().replaceSession(projectKey, nextSession, epoch);
      await startNewQueuedItems(projectKey, epoch, existingItemIds);
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch)) pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
      throw error;
    } finally {
      useImportStore.getState().endMutation(mutationKey);
    }
  }, [isScopeCurrent, nextSessionMutationRevision, projectId, projectKey, pushToast, rootPath, startNewQueuedItems, t]);

  const cancelDiscovery = useCallback(async () => {
    const taskId = discoveryTaskId;
    if (!taskId) return;
    try {
      await taskLauncher.cancel(taskId);
    } catch (error) {
      if (latestProjectKey.current === projectKey) pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
      throw error;
    }
  }, [discoveryTaskId, projectKey, pushToast, t, taskLauncher]);

  const dismissDiscovery = useCallback(() => setDiscoveryTaskId(null), []);

  const cancelBatch = useCallback(async (requestedBatchId?: string) => {
    const target = batches.find((candidate) => candidate.id === requestedBatchId) ?? batches[0];
    if (!target || cancellingBatchIds.has(target.id)) return;
    const activeTasks = target.tasks.filter((task) => task.status !== "unknown" && !isTerminalStatus(task.status) && task.status !== "waiting_for_confirmation" && task.cancellable);
    if (activeTasks.length === 0) return;
    setCancellingBatchIds((current) => new Set(current).add(target.id));
    try {
      const isBackendBatch = !target.id.startsWith("local:") && !target.id.startsWith("recovered:");
      if (isBackendBatch) {
        const cancelled = await importV2Api.cancelBatch({
          projectId,
          projectRootPath: rootPath,
          sessionId: target.sessionId,
          batchId: target.id,
        });
        cancelled.forEach((task) => selectedTaskUpsert(task));
      } else {
        const results = await Promise.allSettled(activeTasks.map((task) => taskLauncher.cancel(task.id, { suppressToast: true })));
        if (results.some((result) => result.status === "rejected" || (result.status === "fulfilled" && !result.value))) {
          throw new Error(t("importV2.workflow.batchCancelFailed"));
        }
      }
    } catch (error) {
      pushToast("error", error instanceof Error ? error.message : t("importV2.workflow.batchCancelFailed"));
    } finally {
      setCancellingBatchIds((current) => {
        const next = new Set(current);
        next.delete(target.id);
        return next;
      });
    }
  }, [batches, cancellingBatchIds, projectId, pushToast, rootPath, selectedTaskUpsert, t, taskLauncher]);

  const dismissBatch = useCallback((requestedBatchId?: string) => {
    const target = batches.find((candidate) => candidate.id === requestedBatchId) ?? batches[0];
    if (!target || target.active > 0 || target.cancelling > 0) return;
    setDismissedBatchIds((current) => new Set(current).add(target.id));
    setBatchRecords((current) => current.filter((record) => record.id !== target.id));
    setCancellingBatchIds((current) => {
      if (!current.has(target.id)) return current;
      const next = new Set(current);
      next.delete(target.id);
      return next;
    });
  }, [batches]);

  const setItemSelected = useCallback(async (itemId: string, selected: boolean) => {
    const current = useImportStore.getState();
    const originalItem = current.session?.items.find((item) => item.itemId === itemId);
    if (!current.session || !originalItem || current.projectKey !== projectKey) return;
    const epoch = current.sessionEpoch;
    const acceptedIds = beginPendingItems([itemId], projectKey, epoch);
    if (acceptedIds.length === 0) return;
    const mutationRevision = nextSessionMutationRevision();
    useImportStore.getState().replaceItem(projectKey, { ...originalItem, selected }, epoch);
    try {
      const nextSession = await importV2Api.setSelection({ projectId, projectRootPath: rootPath, sessionId: current.session.sessionId, itemId, selected });
      if (isScopeCurrent(projectKey, epoch) && sessionMutationRevisionRef.current === mutationRevision) {
        useImportStore.getState().replaceSession(projectKey, nextSession, epoch);
      }
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch) && sessionMutationRevisionRef.current === mutationRevision) {
        useImportStore.getState().replaceItem(projectKey, originalItem, epoch);
        pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
      }
    } finally {
      endPendingItems(acceptedIds, projectKey, epoch);
    }
  }, [beginPendingItems, endPendingItems, isScopeCurrent, nextSessionMutationRevision, projectId, projectKey, pushToast, rootPath, t]);

  const startItems = useCallback(async (itemIds: readonly string[], recoveryAction: ImportRecoveryAction | null = null) => {
    const current = useImportStore.getState();
    if (!current.session || current.projectKey !== projectKey) return;
    const ids = [...new Set(itemIds)].filter((id) => current.session?.items.some((item) => item.itemId === id));
    if (ids.length === 0) return;
    const epoch = current.sessionEpoch;
    const acceptedIds = beginPendingItems(ids, projectKey, epoch);
    if (acceptedIds.length === 0) return;
    nextSessionMutationRevision();
    try {
      const tasks = await importV2Api.startItems({
        projectId,
        projectRootPath: rootPath,
        sessionId: current.session.sessionId,
        itemIds: acceptedIds,
        ...(recoveryAction ? { recoveryAction } : {}),
      });
      tasks.forEach((task) => selectedTaskUpsert(task));
      const resolvedTasks = tasks.map((task) => useTaskStore.getState().tasks.find((current) => current.id === task.id) ?? task);
      recordItemBatch(resolvedTasks, acceptedIds, projectKey, epoch, current.session.sessionId);
      const { terminalIds } = registerItemTasks(resolvedTasks, acceptedIds, projectKey, epoch);
      if (terminalIds.length > 0 && isScopeCurrent(projectKey, epoch)) {
        void refreshForScope(projectKey, epoch);
      }
      reconcilePendingTasks(resolvedTasks);
      void fetchTasks().then(() => reconcilePendingTasks()).catch(() => undefined);
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch)) pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
      endPendingItems(acceptedIds, projectKey, epoch);
    }
  }, [beginPendingItems, endPendingItems, isScopeCurrent, nextSessionMutationRevision, projectId, projectKey, pushToast, recordItemBatch, reconcilePendingTasks, refreshForScope, registerItemTasks, rootPath, selectedTaskUpsert, t]);

  const retryItem = useCallback(async (itemId: string, recoveryAction: ImportRecoveryAction | null = null) => startItems([itemId], recoveryAction), [startItems]);

  const retryBatch = useCallback(async (batchId: string) => {
    const target = batches.find((candidate) => candidate.id === batchId);
    if (!target || target.failedItemIds.length === 0 || target.active > 0) return;
    await startItems(target.failedItemIds);
  }, [batches, startItems]);

  const skipItem = useCallback(async (itemId: string) => {
    const current = useImportStore.getState();
    const item = current.session?.items.find((candidate) => candidate.itemId === itemId);
    if (!item || current.projectKey !== projectKey) return;
    const epoch = current.sessionEpoch;
    const acceptedIds = beginPendingItems([itemId], projectKey, epoch);
    if (acceptedIds.length === 0) return;
    const mutationRevision = nextSessionMutationRevision();
    try {
      const nextSession = await importV2Api.skipItem({
        projectId,
        projectRootPath: rootPath,
        sessionId: current.session!.sessionId,
        itemId,
      });
      if (isScopeCurrent(projectKey, epoch) && sessionMutationRevisionRef.current === mutationRevision) {
        useImportStore.getState().replaceSession(projectKey, nextSession, epoch);
      }
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch)) pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
    } finally {
      endPendingItems(acceptedIds, projectKey, epoch);
    }
  }, [beginPendingItems, endPendingItems, isScopeCurrent, nextSessionMutationRevision, projectId, projectKey, pushToast, rootPath, t]);

  const authorizeLocalAsr = useCallback(async (itemId: string) => {
    const current = useImportStore.getState();
    if (!current.session || current.projectKey !== projectKey || !current.session.items.some((item) => item.itemId === itemId)) return;
    const epoch = current.sessionEpoch;
    const mutationRevision = nextSessionMutationRevision();
    try {
      await importV2Api.authorizeBilibiliAsr({
        projectId,
        projectRootPath: rootPath,
        sessionId: current.session.sessionId,
        itemId,
      });
      if (!isScopeCurrent(projectKey, epoch) || sessionMutationRevisionRef.current !== mutationRevision) return;
      await refreshForScope(projectKey, epoch);
      await startItems([itemId]);
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch)) pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
    }
  }, [isScopeCurrent, nextSessionMutationRevision, projectId, projectKey, pushToast, refreshForScope, rootPath, startItems, t]);

  const cancelItem = useCallback(async (itemId: string) => {
    const current = useImportStore.getState();
    const item = current.session?.items.find((candidate) => candidate.itemId === itemId);
    if (!item || current.projectKey !== projectKey) return;
    if (!item.taskId) {
      if (item.status === "queued") {
        const epoch = current.sessionEpoch;
        const acceptedIds = beginPendingItems([itemId], projectKey, epoch);
        if (acceptedIds.length === 0) return;
        const mutationRevision = nextSessionMutationRevision();
        try {
          const nextSession = await importV2Api.cancelItem({
            projectId,
            projectRootPath: rootPath,
            sessionId: current.session!.sessionId,
            itemId,
          });
          if (isScopeCurrent(projectKey, epoch) && sessionMutationRevisionRef.current === mutationRevision) {
            useImportStore.getState().replaceSession(projectKey, nextSession, epoch);
          }
        } catch (error) {
          if (isScopeCurrent(projectKey, epoch)) pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
        } finally {
          endPendingItems(acceptedIds, projectKey, epoch);
        }
      }
      return;
    }
    const epoch = current.sessionEpoch;
    const acceptedIds = beginPendingItems([itemId], projectKey, epoch);
    if (acceptedIds.length === 0) return;
    nextSessionMutationRevision();
    try {
      await taskLauncher.cancel(item.taskId);
      if (isScopeCurrent(projectKey, epoch)) await refreshForScope(projectKey, epoch);
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch)) pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
    } finally {
      endPendingItems(acceptedIds, projectKey, epoch);
    }
  }, [beginPendingItems, endPendingItems, isScopeCurrent, nextSessionMutationRevision, projectId, projectKey, pushToast, refreshForScope, rootPath, t, taskLauncher]);

  const confirm = useCallback(async (decisions: CommitItemDecision[]) => {
    const current = useImportStore.getState();
    if (!current.session || current.projectKey !== projectKey || decisions.length === 0 || current.isConfirming) return;
    const epoch = current.sessionEpoch;
    current.setIsConfirming(true);
    try {
      const task = await importV2Api.confirmSession({ projectId, projectRootPath: rootPath, sessionId: current.session.sessionId, decisions });
      // A commit can finish before this IPC response resolves. Keep a newer
      // terminal snapshot from the global task store instead of reintroducing
      // a queued task and leaving the commit bar locked.
      const knownTask = useTaskStore.getState().tasks.find((candidate) => candidate.id === task.id);
      const observedTask = knownTask && isTerminalStatus(knownTask.status) ? knownTask : task;
      selectedTaskUpsert(observedTask);
      pendingConfirmationTasks.current.set(task.id, { projectKey, epoch });
      if (isScopeCurrent(projectKey, epoch)) openTaskDrawer(task.id);
      if (isTerminalStatus(observedTask.status)) {
        pendingConfirmationTasks.current.delete(task.id);
        if (isScopeCurrent(projectKey, epoch)) {
          useImportStore.getState().setIsConfirming(false);
          void refreshForScope(projectKey, epoch).catch(() => undefined);
          if (observedTask.status === "failed") pushToast("error", t("importV2.workflow.confirmFailed"));
        }
      }
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch)) {
        useImportStore.getState().setIsConfirming(false);
        pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
      }
    }
  }, [isScopeCurrent, openTaskDrawer, projectId, projectKey, pushToast, refreshForScope, rootPath, selectedTaskUpsert, t]);

  const confirmLegacy = useCallback((options: { createCheckpoint: boolean; compileAfterImport: boolean }) => {
    void options;
    const decisions: CommitItemDecision[] = (useImportStore.getState().session?.items ?? [])
      .filter((item) => item.selected && item.status === "preview_ready")
      .map((item) => ({ itemId: item.itemId, conflictAction: "create_new", expectedWikiHash: null }));
    void confirm(decisions);
  }, [confirm]);

  const refreshSession = useCallback(() => {
    const current = useImportStore.getState();
    return current.session && current.projectKey === projectKey
      ? refreshForScope(projectKey, current.sessionEpoch, current.session.sessionId)
      : Promise.resolve();
  }, [projectKey, refreshForScope]);

  const loadPreview = useCallback(async (identity: { sessionId: string; itemId: string; candidateId: string | null; historyBatchId?: string | null }) => {
    const current = useImportStore.getState();
    if (current.projectKey !== projectKey) {
      throw new Error("Preview identity is no longer current");
    }
    try {
      const content = await importV2Api.getPreviewContent({ projectId, projectRootPath: rootPath, ...identity });
      if (latestProjectKey.current !== projectKey) throw new Error("Preview identity is no longer current");
      return content;
    } catch (error) {
      if (latestProjectKey.current === projectKey) pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
      throw error;
    }
  }, [projectId, projectKey, pushToast, rootPath, t]);

  const loadSession = useCallback(async (sessionId: string, historyBatchId: string | null = null) => {
    if (!sessionId || latestProjectKey.current !== projectKey) return null;
    try {
      const result = await (importV2Api.getHistorySession ?? importV2Api.getSession)({ projectId, projectRootPath: rootPath, sessionId, historyBatchId });
      return latestProjectKey.current === projectKey && useImportStore.getState().projectKey === projectKey ? result : null;
    } catch (error) {
      if (latestProjectKey.current === projectKey) pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
      return null;
    }
  }, [projectId, projectKey, pushToast, rootPath, t]);

  const visibleItems = useMemo(() => selectVisibleItems(session, filter), [filter, session]);
  const counts = useMemo(() => selectQueueCounts(session), [session]);
  const progress = useMemo(() => selectSessionProgress(session), [session]);
  const selectItem = useImportStore((state) => state.selectItem);
  const setFilter = useImportStore((state) => state.setFilter);

  const requestPreview = useCallback((paths: string[]) => { void addPaths(paths).catch(() => undefined); }, [addPaths]);
  const requestUrl = useCallback((url: string) => addUrl(url), [addUrl]);
  const requestClipboard = useCallback(async (content: string) => {
    if (!content.trim()) return;
    try {
      if (typeof navigator !== "undefined" && navigator.clipboard) {
        await navigator.clipboard.writeText(content);
      } else if (!copyTextWithDomFallback(content)) {
        throw new Error("Clipboard unavailable");
      }
      pushToast("info", t("importV2.workflow.clipboardCopied"));
    } catch {
      if (copyTextWithDomFallback(content)) {
        pushToast("info", t("importV2.workflow.clipboardCopied"));
      } else {
        pushToast("warning", t("importV2.workflow.clipboardUnavailable"));
      }
    }
  }, [pushToast, t]);
  const requestDeleteSource = useCallback(async () => {
    pushToast("warning", t("importV2.workflow.legacyActionUnavailable"));
  }, [pushToast, t]);
  const requestReplaceSource = useCallback(async () => {
    pushToast("warning", t("importV2.workflow.legacyActionUnavailable"));
  }, [pushToast, t]);

  const getAgentPolicy = useCallback(async () => {
    try {
      const result = await importV2Api.getAgentPolicy({ projectId, projectRootPath: rootPath });
      return latestProjectKey.current === projectKey ? result : null;
    } catch (error) {
      if (latestProjectKey.current === projectKey) pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
      return null;
    }
  }, [projectId, projectKey, pushToast, rootPath, t]);

  const setAgentPolicy = useCallback(async (policy: AgentAssistancePolicy, localAgentKind: AgentKind | null) => {
    try {
      const result = await importV2Api.setAgentPolicy({ projectId, projectRootPath: rootPath, policy, localAgentKind });
      return latestProjectKey.current === projectKey ? result : null;
    } catch (error) {
      if (latestProjectKey.current === projectKey) pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
      throw error;
    }
  }, [projectId, projectKey, pushToast, rootPath, t]);

  const invokeLocalAgent = useCallback(async (itemId: string, trigger: AgentAssistanceTrigger, agentKind: AgentKind) => {
    const current = useImportStore.getState();
    if (!current.session || current.projectKey !== projectKey || !current.session.items.some((item) => item.itemId === itemId)) return null;
    const epoch = current.sessionEpoch;
    try {
      const task = await importV2Api.startAgentAssistance({ projectId, projectRootPath: rootPath, sessionId: current.session.sessionId, itemId, trigger, agentKind });
      if (!isScopeCurrent(projectKey, epoch)) return null;
      selectedTaskUpsert(task);
      openTaskDrawer(task.id);
      return task;
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch)) pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
      throw error;
    }
  }, [isScopeCurrent, openTaskDrawer, projectId, projectKey, pushToast, rootPath, selectedTaskUpsert, t]);

  const previewByokScope = useCallback(async (itemId: string, trigger: AgentAssistanceTrigger, provider: LlmProviderKind) => {
    const current = useImportStore.getState();
    if (!current.session || current.projectKey !== projectKey || !current.session.items.some((item) => item.itemId === itemId)) return null;
    const epoch = current.sessionEpoch;
    try {
      const scope = await importV2Api.previewByokScope({ projectId, projectRootPath: rootPath, sessionId: current.session.sessionId, itemId, trigger, provider });
      return isScopeCurrent(projectKey, epoch) ? scope : null;
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch)) pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
      throw error;
    }
  }, [isScopeCurrent, projectId, projectKey, pushToast, rootPath, t]);

  const approveByokAssistance = useCallback(async (request: {
    itemId: string;
    trigger: AgentAssistanceTrigger;
    provider: LlmProviderKind;
    model: string;
    approvalId: string;
    scopeSha256: string;
    acknowledgePossibleDuplicateCharge: boolean;
  }) => {
    const current = useImportStore.getState();
    if (!current.session || current.projectKey !== projectKey || !current.session.items.some((item) => item.itemId === request.itemId)) return null;
    const epoch = current.sessionEpoch;
    try {
      const task = await importV2Api.approveByokAssistance({ projectId, projectRootPath: rootPath, sessionId: current.session.sessionId, ...request });
      if (!isScopeCurrent(projectKey, epoch)) return null;
      selectedTaskUpsert(task);
      openTaskDrawer(task.id);
      return task;
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch)) pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
      throw error;
    }
  }, [isScopeCurrent, openTaskDrawer, projectId, projectKey, pushToast, rootPath, selectedTaskUpsert, t]);

  const acceptAgentCandidate = useCallback(async (itemId: string, taskId: string) => {
    const current = useImportStore.getState();
    if (!current.session || current.projectKey !== projectKey || !current.session.items.some((item) => item.itemId === itemId)) return null;
    const epoch = current.sessionEpoch;
    try {
      const view = await importV2Api.acceptAgentCandidate({ projectId, projectRootPath: rootPath, sessionId: current.session.sessionId, itemId, taskId });
      return isScopeCurrent(projectKey, epoch) ? view : null;
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch)) pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
      throw error;
    }
  }, [isScopeCurrent, projectId, projectKey, pushToast, rootPath, t]);

  const selectAgentCandidate = useCallback(async (request: {
    itemId: string;
    candidateId: string;
    mergedMarkdown: string | null;
    expectedCurrentWikiSha256: string | null;
  }) => {
    const current = useImportStore.getState();
    if (!current.session || current.projectKey !== projectKey || !current.session.items.some((item) => item.itemId === request.itemId)) return null;
    const epoch = current.sessionEpoch;
    try {
      const result = await importV2Api.selectAgentCandidate({ projectId, projectRootPath: rootPath, sessionId: current.session.sessionId, ...request });
      if (isScopeCurrent(projectKey, epoch)) useImportStore.getState().replaceItem(projectKey, result.item, epoch);
      return isScopeCurrent(projectKey, epoch) ? result : null;
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch)) pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
      throw error;
    }
  }, [isScopeCurrent, projectId, projectKey, pushToast, rootPath, t]);

  const discardAgentCandidate = useCallback(async (itemId: string, candidateId: string) => {
    const current = useImportStore.getState();
    if (!current.session || current.projectKey !== projectKey || !current.session.items.some((item) => item.itemId === itemId)) return null;
    const epoch = current.sessionEpoch;
    try {
      const result = await importV2Api.discardAgentCandidate({ projectId, projectRootPath: rootPath, sessionId: current.session.sessionId, itemId, candidateId });
      if (isScopeCurrent(projectKey, epoch)) useImportStore.getState().replaceItem(projectKey, result.item, epoch);
      return isScopeCurrent(projectKey, epoch) ? result : null;
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch)) pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
      throw error;
    }
  }, [isScopeCurrent, projectId, projectKey, pushToast, rootPath, t]);

  const beginLogin = useCallback(async (itemId: string, platform: string) => {
    const current = useImportStore.getState();
    if (!current.session || current.projectKey !== projectKey || !current.session.items.some((item) => item.itemId === itemId)) return null;
    const epoch = current.sessionEpoch;
    try {
      const result = await importV2Api.beginLogin({ projectId, projectRootPath: rootPath, sessionId: current.session.sessionId, itemId, platform });
      return isScopeCurrent(projectKey, epoch) ? result : null;
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch)) pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
      throw error;
    }
  }, [isScopeCurrent, projectId, projectKey, pushToast, rootPath, t]);

  const completeLogin = useCallback(async (itemId: string, connectorSessionId: string) => {
    const current = useImportStore.getState();
    if (!current.session || current.projectKey !== projectKey || !current.session.items.some((item) => item.itemId === itemId)) return null;
    const epoch = current.sessionEpoch;
    try {
      const result = await importV2Api.completeLogin({ projectId, projectRootPath: rootPath, importSessionId: current.session.sessionId, itemId, connectorSessionId });
      if (!isScopeCurrent(projectKey, epoch)) return null;
      await refreshForScope(projectKey, epoch);
      await startItems([itemId]);
      return result;
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch)) pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
      throw error;
    }
  }, [isScopeCurrent, projectId, projectKey, pushToast, refreshForScope, rootPath, startItems, t]);

  const revokeLogin = useCallback(async (connectorSessionId: string) => {
    if (latestProjectKey.current !== projectKey) return false;
    try {
      await importV2Api.revokeLogin({ sessionId: connectorSessionId });
      return latestProjectKey.current === projectKey;
    } catch (error) {
      if (latestProjectKey.current === projectKey) pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
      throw error;
    }
  }, [projectKey, pushToast, t]);

  const authorizePrivateTarget = useCallback(async (itemId: string, url: string) => {
    const current = useImportStore.getState();
    if (!current.session || current.projectKey !== projectKey || !current.session.items.some((item) => item.itemId === itemId)) return null;
    const epoch = current.sessionEpoch;
    try {
      const grant = await importV2Api.authorizePrivateTarget({ projectId, projectRootPath: rootPath, sessionId: current.session.sessionId, itemId, url });
      return isScopeCurrent(projectKey, epoch) ? grant : null;
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch)) pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
      throw error;
    }
  }, [isScopeCurrent, projectId, projectKey, pushToast, rootPath, t]);

  const getCapabilityRequirement = useCallback(async (itemId: string) => {
    const current = useImportStore.getState();
    if (!current.session || current.projectKey !== projectKey || !current.session.items.some((item) => item.itemId === itemId)) return null;
    const epoch = current.sessionEpoch;
    try {
      const result = await importV2Api.getCapabilityRequirement({ projectId, projectRootPath: rootPath, sessionId: current.session.sessionId, itemId });
      return isScopeCurrent(projectKey, epoch) ? result : null;
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch)) pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
      throw error;
    }
  }, [isScopeCurrent, projectId, projectKey, pushToast, rootPath, t]);

  const installCapability = useCallback(async (itemId: string, capabilityId: string) => {
    const current = useImportStore.getState();
    if (!current.session || current.projectKey !== projectKey || !current.session.items.some((item) => item.itemId === itemId)) return null;
    const epoch = current.sessionEpoch;
    try {
      const task = await importV2Api.installCapability({ projectId, projectRootPath: rootPath, sessionId: current.session.sessionId, itemId, capabilityId, acknowledgeInstall: true });
      if (!isScopeCurrent(projectKey, epoch)) return null;
      selectedTaskUpsert(task);
      openTaskDrawer(task.id);
      return task;
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch)) pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
      throw error;
    }
  }, [isScopeCurrent, openTaskDrawer, projectId, projectKey, pushToast, rootPath, selectedTaskUpsert, t]);

  const scanMigration = useCallback(async () => {
    if (latestProjectKey.current !== projectKey) return null;
    try {
      const inventory = await importV2Api.scanMigration({ projectId, projectRootPath: rootPath });
      return latestProjectKey.current === projectKey ? inventory : null;
    } catch (error) {
      if (latestProjectKey.current === projectKey) pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
      throw error;
    }
  }, [projectId, projectKey, pushToast, rootPath, t]);

  const planMigration = useCallback(async (inventory: LegacyInventory) => {
    if (latestProjectKey.current !== projectKey) return null;
    try {
      const plan = await importV2Api.planMigration({ projectId, projectRootPath: rootPath, inventory });
      return latestProjectKey.current === projectKey ? plan : null;
    } catch (error) {
      if (latestProjectKey.current === projectKey) pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
      throw error;
    }
  }, [projectId, projectKey, pushToast, rootPath, t]);

  const applyMigration = useCallback(async (plan: MigrationPlan, confirmation: MigrationConfirmation) => {
    if (latestProjectKey.current !== projectKey) return null;
    try {
      const task = await importV2Api.applyMigration({ projectId, projectRootPath: rootPath, plan, confirmation });
      if (latestProjectKey.current !== projectKey) return null;
      selectedTaskUpsert(task);
      openTaskDrawer(task.id);
      return task;
    } catch (error) {
      if (latestProjectKey.current === projectKey) pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
      throw error;
    }
  }, [openTaskDrawer, projectId, projectKey, pushToast, rootPath, selectedTaskUpsert, t]);

  const getMigrationStatus = useCallback(async () => {
    if (latestProjectKey.current !== projectKey) return null;
    try {
      const status = await importV2Api.getMigrationStatus({ projectId, projectRootPath: rootPath });
      return latestProjectKey.current === projectKey ? status : null;
    } catch (error) {
      if (latestProjectKey.current === projectKey) pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
      throw error;
    }
  }, [projectId, projectKey, pushToast, rootPath, t]);

  const resumeMigration = useCallback(async (plan: MigrationPlan, confirmation: MigrationConfirmation) => {
    if (latestProjectKey.current !== projectKey) return null;
    try {
      const task = await importV2Api.resumeMigration({ projectId, projectRootPath: rootPath, plan, confirmation });
      if (latestProjectKey.current !== projectKey) return null;
      selectedTaskUpsert(task);
      openTaskDrawer(task.id);
      return task;
    } catch (error) {
      if (latestProjectKey.current === projectKey) pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
      throw error;
    }
  }, [openTaskDrawer, projectId, projectKey, pushToast, rootPath, selectedTaskUpsert, t]);

  const listHistory = useCallback(async (cursor: string | null = null) => {
    if (latestProjectKey.current !== projectKey) return null;
    try {
      const page = await importV2Api.listHistory({ projectId, projectRootPath: rootPath, cursor, limit: 50 });
      return latestProjectKey.current === projectKey ? page : null;
    } catch (error) {
      if (latestProjectKey.current === projectKey) pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
      throw error;
    }
  }, [projectId, projectKey, pushToast, rootPath, t]);

  return {
    session,
    readiness,
    readinessWarning,
    bootstrapError,
    retryBootstrap,
    bootstrapState,
    visibleItems,
    counts,
    progress,
    discoveryTask,
    discoveryScan,
    discoveryTaskUnavailable,
    isAddingPaths,
    isAddingUrl,
    pendingItemIds,
    isSyncingSession,
    batch,
    batches,
    isCancellingBatch,
    isBatchCancelling,
    selectedItemId,
    filter,
    addPaths,
    addUrl,
    cancelDiscovery,
    dismissDiscovery,
    cancelBatch,
    dismissBatch,
    retryBatch,
    setItemSelected,
    startItems,
    retryItem,
    cancelItem,
    skipItem,
    authorizeLocalAsr,
    confirm,
    confirmLegacy,
    refreshSession,
    selectItem,
    setFilter,
    loadPreview,
    loadSession,
    getAgentPolicy,
    setAgentPolicy,
    invokeLocalAgent,
    previewByokScope,
    approveByokAssistance,
    acceptAgentCandidate,
    selectAgentCandidate,
    discardAgentCandidate,
    beginLogin,
    completeLogin,
    revokeLogin,
    authorizePrivateTarget,
    getCapabilityRequirement,
    installCapability,
    scanMigration,
    planMigration,
    applyMigration,
    getMigrationStatus,
    resumeMigration,
    listHistory,
    importedSources,
    isConfirming,
    requestPreview,
    requestClipboard,
    requestUrl,
    requestDeleteSource,
    requestReplaceSource,
  };
}
