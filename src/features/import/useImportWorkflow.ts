import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import type { TaskLauncher } from "../../hooks/useTaskLauncher";
import { registerTaskEventListener } from "../../hooks/useTaskEvents";
import { importV2Api } from "../../services/importV2Api";
import { importProjectKey, useImportStore, type ImportQueueFilter } from "../../stores/importStore";
import { useTaskStore } from "../../stores/taskStore";
import { useToastStore } from "../../stores/toastStore";
import type { ImportedSource } from "../../types/import";
import type { CommitItemDecision, ImportItem, ImportSession } from "../../types/importV2";
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
import type { BackendEvent, BackendTask } from "../../types/task";
import type { ImportHistoryPage } from "../../types/importV2Presentation";
import type { MigrationConfirmation, LegacyInventory, MigrationPlan, MigrationStatusSnapshot } from "../../types/importV2Migration";
import { selectQueueCounts, selectSessionProgress, selectVisibleItems, type ImportQueueCounts, type ImportSessionProgress } from "./importViewModel";
import type { AppView } from "../../stores/navigationStore";

export type ImportBootstrapState = "loading" | "ready" | "blocked" | "error";

export interface ImportWorkflow {
  session: ImportSession | null;
  readiness: ImportFrontendReadiness | null;
  bootstrapState: ImportBootstrapState;
  visibleItems: ImportItem[];
  counts: ImportQueueCounts;
  progress: ImportSessionProgress;
  selectedItemId: string | null;
  filter: ImportQueueFilter;
  addPaths: (paths: string[]) => Promise<void>;
  addUrl: (url: string) => Promise<void>;
  setItemSelected: (itemId: string, selected: boolean) => Promise<void>;
  startItems: (itemIds: string[]) => Promise<void>;
  retryItem: (itemId: string) => Promise<void>;
  cancelItem: (itemId: string) => Promise<void>;
  confirm: (decisions: CommitItemDecision[]) => Promise<void>;
  confirmLegacy: (options: { createCheckpoint: boolean; compileAfterImport: boolean }) => void;
  refreshSession: () => Promise<void>;
  selectItem: (itemId: string | null) => void;
  setFilter: (filter: ImportQueueFilter) => void;
  loadPreview: (identity: { sessionId: string; itemId: string; candidateId: string | null }) => Promise<ImportPreviewContent>;

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
    if (typeof message === "string") return message;
  }
  return String(error);
}

function isTerminalTaskEvent(event: BackendEvent): boolean {
  return event.eventType === "task_completed" || event.eventType === "task_failed" || event.eventType === "task_cancelled";
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
  const [bootstrapState, setBootstrapState] = useState<ImportBootstrapState>("loading");
  const session = useImportStore((state) => state.session);
  const selectedItemId = useImportStore((state) => state.selectedItemId);
  const filter = useImportStore((state) => state.filter);
  const importedSources = useImportStore((state) => state.importedSources);
  const isConfirming = useImportStore((state) => state.isConfirming);
  const selectedTaskUpsert = useTaskStore((state) => state.upsertTask);
  const openTaskDrawer = useTaskStore((state) => state.openDrawer);
  const pushToast = useToastStore((state) => state.pushToast);
  const pendingPathTasks = useRef(new Map<string, { projectKey: string; epoch: number; existingItemIds: ReadonlySet<string> }>());
  const refreshInFlight = useRef<Promise<void> | null>(null);

  const isScopeCurrent = useCallback(
    (requestKey: string, epoch: number) =>
      latestProjectKey.current === requestKey &&
      useImportStore.getState().projectKey === requestKey &&
      useImportStore.getState().sessionEpoch === epoch,
    [],
  );

  const refreshForScope = useCallback(async (requestKey: string, epoch: number, expectedSessionId?: string) => {
    if (!isScopeCurrent(requestKey, epoch)) return;
    const currentSession = useImportStore.getState().session;
    const sessionId = expectedSessionId ?? currentSession?.sessionId;
    if (!sessionId) return;
    if (refreshInFlight.current) return refreshInFlight.current;
    const request = importV2Api.getSession({ projectId, projectRootPath: rootPath, sessionId });
    const refresh = request
      .then((nextSession) => {
        if (isScopeCurrent(requestKey, epoch)) {
          useImportStore.getState().replaceSession(requestKey, nextSession, epoch);
        }
      })
      .catch((error) => {
        if (isScopeCurrent(requestKey, epoch)) {
          pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
        }
      })
      .finally(() => {
        refreshInFlight.current = null;
      });
    refreshInFlight.current = refresh;
    return refresh;
  }, [isScopeCurrent, projectId, pushToast, rootPath, t]);

  const startNewQueuedItems = useCallback(async (requestKey: string, epoch: number, before: ReadonlySet<string>) => {
    if (!isScopeCurrent(requestKey, epoch)) return;
    const current = useImportStore.getState().session;
    if (!current) return;
    const itemIds = current.items
      .filter((item) => !before.has(item.itemId) && item.status === "queued")
      .map((item) => item.itemId);
    if (itemIds.length === 0) return;
    const tasks = await importV2Api.startItems({
      projectId,
      projectRootPath: rootPath,
      sessionId: current.sessionId,
      itemIds,
    });
    for (const task of tasks) {
      selectedTaskUpsert(task);
      if (isScopeCurrent(requestKey, epoch)) openTaskDrawer(task.id);
    }
  }, [isScopeCurrent, openTaskDrawer, projectId, rootPath, selectedTaskUpsert]);

  const handleTaskEvent = useCallback((event: BackendEvent) => {
    if (!isTerminalTaskEvent(event) || !event.taskId) return;
    const pending = pendingPathTasks.current.get(event.taskId);
    if (pending) {
      pendingPathTasks.current.delete(event.taskId);
      const task = event.payload as BackendTask;
      if (task.status === "succeeded") {
        void refreshForScope(pending.projectKey, pending.epoch).then(() =>
          startNewQueuedItems(pending.projectKey, pending.epoch, pending.existingItemIds),
        );
      }
    }
    const current = useImportStore.getState();
    if (event.projectId !== projectId || current.projectKey !== projectKey || !current.session) return;
    if (current.session.items.some((item) => item.taskId === event.taskId)) {
      void refreshForScope(projectKey, current.sessionEpoch);
    }
  }, [projectId, projectKey, refreshForScope, startNewQueuedItems]);

  useEffect(() => registerTaskEventListener(handleTaskEvent), [handleTaskEvent]);

  useEffect(() => {
    const store = useImportStore.getState();
    store.resetProjectPresentation(projectKey);
    store.setImportedSources([]);
    const epoch = store.beginSessionEpoch(projectKey);
    setReadiness(null);
    setBootstrapState("loading");
    pendingPathTasks.current.clear();
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
        const nextReadiness = await importV2Api.getReadiness({ projectId, projectRootPath: rootPath });
        if (cancelled || !isScopeCurrent(projectKey, epoch)) return;
        setReadiness(nextReadiness);
        const nextSession = nextReadiness.unfinishedSessionId
          ? await importV2Api.getSession({ projectId, projectRootPath: rootPath, sessionId: nextReadiness.unfinishedSessionId })
          : await importV2Api.createSession({ projectId, projectRootPath: rootPath, resourceMode: "balanced" });
        if (cancelled || !isScopeCurrent(projectKey, epoch)) return;
        store.attachSession(projectKey, nextSession, epoch);
        setBootstrapState("ready");
      } catch (error) {
        if (cancelled || !isScopeCurrent(projectKey, epoch)) return;
        setBootstrapState("error");
        pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [activeView, isScopeCurrent, projectId, projectKey, pushToast, rootPath, t]);

  const addPaths = useCallback(async (paths: string[]) => {
    const sourcePaths = paths.map((path) => path.trim()).filter(Boolean);
    const current = useImportStore.getState();
    if (sourcePaths.length === 0 || !current.session || current.projectKey !== projectKey) return;
    const epoch = current.sessionEpoch;
    const existingItemIds = new Set(current.session.items.map((item) => item.itemId));
    const mutationKey = `add-paths:${projectKey}`;
    current.beginMutation(mutationKey);
    try {
      const task = await importV2Api.addPaths({
        projectId,
        projectRootPath: rootPath,
        sessionId: current.session.sessionId,
        sourcePaths,
      });
      selectedTaskUpsert(task);
      pendingPathTasks.current.set(task.id, { projectKey, epoch, existingItemIds });
      if (isScopeCurrent(projectKey, epoch)) openTaskDrawer(task.id);
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch)) pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
    } finally {
      useImportStore.getState().endMutation(mutationKey);
    }
  }, [isScopeCurrent, openTaskDrawer, projectId, projectKey, pushToast, rootPath, selectedTaskUpsert, t]);

  const addUrl = useCallback(async (url: string) => {
    const current = useImportStore.getState();
    if (!url.trim() || !current.session || current.projectKey !== projectKey) return;
    const epoch = current.sessionEpoch;
    const existingItemIds = new Set(current.session.items.map((item) => item.itemId));
    const mutationKey = `add-url:${projectKey}`;
    current.beginMutation(mutationKey);
    try {
      const nextSession = await importV2Api.addUrl({
        projectId,
        projectRootPath: rootPath,
        sessionId: current.session.sessionId,
        url: url.trim(),
      });
      if (!isScopeCurrent(projectKey, epoch)) return;
      useImportStore.getState().replaceSession(projectKey, nextSession, epoch);
      await startNewQueuedItems(projectKey, epoch, existingItemIds);
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch)) pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
      throw error;
    } finally {
      useImportStore.getState().endMutation(mutationKey);
    }
  }, [isScopeCurrent, projectId, projectKey, pushToast, rootPath, startNewQueuedItems, t]);

  const setItemSelected = useCallback(async (itemId: string, selected: boolean) => {
    const current = useImportStore.getState();
    if (!current.session || current.projectKey !== projectKey) return;
    const epoch = current.sessionEpoch;
    try {
      const nextSession = await importV2Api.setSelection({ projectId, projectRootPath: rootPath, sessionId: current.session.sessionId, itemId, selected });
      if (isScopeCurrent(projectKey, epoch)) useImportStore.getState().replaceSession(projectKey, nextSession, epoch);
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch)) pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
    }
  }, [isScopeCurrent, projectId, projectKey, pushToast, rootPath, t]);

  const startItems = useCallback(async (itemIds: string[]) => {
    const current = useImportStore.getState();
    if (!current.session || current.projectKey !== projectKey) return;
    const ids = [...new Set(itemIds)].filter((id) => current.session?.items.some((item) => item.itemId === id));
    if (ids.length === 0) return;
    const epoch = current.sessionEpoch;
    try {
      const tasks = await importV2Api.startItems({ projectId, projectRootPath: rootPath, sessionId: current.session.sessionId, itemIds: ids });
      for (const task of tasks) {
        selectedTaskUpsert(task);
        if (isScopeCurrent(projectKey, epoch)) openTaskDrawer(task.id);
      }
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch)) pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
    }
  }, [isScopeCurrent, openTaskDrawer, projectId, projectKey, pushToast, rootPath, selectedTaskUpsert, t]);

  const retryItem = useCallback(async (itemId: string) => startItems([itemId]), [startItems]);

  const cancelItem = useCallback(async (itemId: string) => {
    const current = useImportStore.getState();
    const item = current.session?.items.find((candidate) => candidate.itemId === itemId);
    if (!item?.taskId || current.projectKey !== projectKey) return;
    const epoch = current.sessionEpoch;
    await taskLauncher.cancel(item.taskId);
    if (isScopeCurrent(projectKey, epoch)) await refreshForScope(projectKey, epoch);
  }, [isScopeCurrent, projectKey, refreshForScope, taskLauncher]);

  const confirm = useCallback(async (decisions: CommitItemDecision[]) => {
    const current = useImportStore.getState();
    if (!current.session || current.projectKey !== projectKey || decisions.length === 0) return;
    const epoch = current.sessionEpoch;
    current.setIsConfirming(true);
    try {
      const task = await importV2Api.confirmSession({ projectId, projectRootPath: rootPath, sessionId: current.session.sessionId, decisions });
      selectedTaskUpsert(task);
      if (isScopeCurrent(projectKey, epoch)) openTaskDrawer(task.id);
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch)) pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
    } finally {
      if (isScopeCurrent(projectKey, epoch)) useImportStore.getState().setIsConfirming(false);
    }
  }, [isScopeCurrent, openTaskDrawer, projectId, projectKey, pushToast, rootPath, selectedTaskUpsert, t]);

  const confirmLegacy = useCallback((options: { createCheckpoint: boolean; compileAfterImport: boolean }) => {
    void options;
    const decisions: CommitItemDecision[] = (useImportStore.getState().session?.items ?? [])
      .filter((item) => item.selected && item.status === "preview_ready")
      .map((item) => ({ itemId: item.itemId, conflictAction: null, expectedWikiHash: null }));
    void confirm(decisions);
  }, [confirm]);

  const refreshSession = useCallback(() => {
    const current = useImportStore.getState();
    return current.session && current.projectKey === projectKey
      ? refreshForScope(projectKey, current.sessionEpoch, current.session.sessionId)
      : Promise.resolve();
  }, [projectKey, refreshForScope]);

  const loadPreview = useCallback(async (identity: { sessionId: string; itemId: string; candidateId: string | null }) => {
    const current = useImportStore.getState();
    if (!current.session || current.projectKey !== projectKey || current.session.sessionId !== identity.sessionId || !current.session.items.some((item) => item.itemId === identity.itemId)) {
      throw new Error("Preview identity is no longer current");
    }
    const epoch = current.sessionEpoch;
    try {
      const content = await importV2Api.getPreviewContent({ projectId, projectRootPath: rootPath, ...identity });
      if (!isScopeCurrent(projectKey, epoch)) throw new Error("Preview identity is no longer current");
      return content;
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch)) pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
      throw error;
    }
  }, [isScopeCurrent, projectId, projectKey, pushToast, rootPath, t]);

  const visibleItems = useMemo(() => selectVisibleItems(session, filter), [filter, session]);
  const counts = useMemo(() => selectQueueCounts(session), [session]);
  const progress = useMemo(() => selectSessionProgress(session), [session]);
  const selectItem = useImportStore((state) => state.selectItem);
  const setFilter = useImportStore((state) => state.setFilter);

  const requestPreview = useCallback((paths: string[]) => { void addPaths(paths); }, [addPaths]);
  const requestUrl = useCallback((url: string) => addUrl(url), [addUrl]);
  const requestClipboard = useCallback(async () => {
    pushToast("warning", t("importV2.workflow.clipboardUnavailable"));
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
      return isScopeCurrent(projectKey, epoch) ? result : null;
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch)) pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
      throw error;
    }
  }, [isScopeCurrent, projectId, projectKey, pushToast, rootPath, t]);

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
    bootstrapState,
    visibleItems,
    counts,
    progress,
    selectedItemId,
    filter,
    addPaths,
    addUrl,
    setItemSelected,
    startItems,
    retryItem,
    cancelItem,
    confirm,
    confirmLegacy,
    refreshSession,
    selectItem,
    setFilter,
    loadPreview,
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
