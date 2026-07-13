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
import type { ImportFrontendReadiness } from "../../types/importV2Presentation";
import type { ProjectSummary } from "../../types/project";
import type { BackendEvent, BackendTask } from "../../types/task";
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
        if (!nextReadiness.active) {
          setBootstrapState("blocked");
          return;
        }
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
    importedSources,
    isConfirming,
    requestPreview,
    requestClipboard,
    requestUrl,
    requestDeleteSource,
    requestReplaceSource,
  };
}
