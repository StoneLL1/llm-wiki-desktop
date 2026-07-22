import { useCallback, useMemo } from "react";
import { useTranslation } from "react-i18next";

import type { TaskLauncher } from "../../hooks/useTaskLauncher";
import { importV2Api } from "../../services/importV2Api";
import { useImportStore } from "../../stores/importStore";
import { useTaskStore } from "../../stores/taskStore";
import { useToastStore } from "../../stores/toastStore";
import type { CommitItemDecision, ImportRecoveryAction, ImportSession, MediaSaveMode } from "../../types/importV2";
import type { ProjectSummary } from "../../types/project";
import { selectQueueCounts, selectSessionProgress, selectVisibleItems } from "./importViewModel";
import type { ImportWorkflow } from "./importWorkflow";
import type { AppView } from "../../stores/navigationStore";
import { importWorkflowErrorMessage as errorMessage, useImportSessionScope } from "./useImportSessionScope";
import { useImportBatchController } from "./useImportBatchController";
import { useImportTaskCoordinator } from "./useImportTaskCoordinator";
import { useImportSupportingActions } from "./useImportSupportingActions";

export type { ImportBatchProgress, ImportWorkflow } from "./importWorkflow";

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

export function useImportWorkflow(
  project: ProjectSummary,
  activeView: AppView,
  taskLauncher: TaskLauncher,
): ImportWorkflow {
  const { t } = useTranslation();
  const {
    projectId,
    rootPath,
    projectKey,
    readiness,
    readinessWarning,
    bootstrapError,
    bootstrapState,
    isSyncingSession,
    retryBootstrap,
    isProjectCurrent,
    isScopeCurrent,
    nextSessionMutationRevision,
    isSessionMutationRevisionCurrent,
    refreshForScope,
  } = useImportSessionScope(project, activeView);
  const session = useImportStore((state) => state.session);
  const selectedItemId = useImportStore((state) => state.selectedItemId);
  const filter = useImportStore((state) => state.filter);
  const isConfirming = useImportStore((state) => state.isConfirming);
  const taskList = useTaskStore((state) => state.tasks);
  const tasksHydrated = useTaskStore((state) => state.tasksHydrated);
  const pushToast = useToastStore((state) => state.pushToast);
  const mutationKeys = useImportStore((state) => state.mutationKeys);
  const sessionEpoch = useImportStore((state) => state.sessionEpoch);
  const isAddingPaths = [...mutationKeys].some((key) => key.startsWith(`add-paths:${projectKey}:`));
  const isAddingUrl = [...mutationKeys].some((key) => key.startsWith(`add-url:${projectKey}:`));
  const {
    batches,
    batch,
    isCancellingBatch,
    isBatchCancelling,
    recordItemBatch,
    cancelBatch,
    dismissBatch,
  } = useImportBatchController({
    projectId,
    rootPath,
    projectKey,
    sessionEpoch,
    session,
    taskList,
    tasksHydrated,
    taskLauncher,
    isScopeCurrent,
  });

  const {
    pendingItemIds,
    discoveryTask,
    discoveryScan,
    discoveryTaskUnavailable,
    beginPendingItems,
    endPendingItems,
    startNewQueuedItems,
    trackStartedItems,
    trackPathTask,
    trackConfirmationTask,
    trackCapabilityTask,
    cancelDiscovery,
    dismissDiscovery,
  } = useImportTaskCoordinator({
    projectId,
    rootPath,
    projectKey,
    sessionEpoch,
    session,
    taskList,
    tasksHydrated,
    taskLauncher,
    isProjectCurrent,
    isScopeCurrent,
    nextSessionMutationRevision,
    refreshForScope,
    recordItemBatch,
  });

  const reconcileMutationSession = useCallback(async (
    nextSession: ImportSession,
    requestKey: string,
    epoch: number,
    mutationRevision: number,
    expectedSessionId: string,
  ) => {
    if (!isScopeCurrent(requestKey, epoch, expectedSessionId)) return false;
    if (isSessionMutationRevisionCurrent(mutationRevision)) {
      useImportStore.getState().replaceSession(requestKey, nextSession, epoch);
    } else {
      await refreshForScope(requestKey, epoch, expectedSessionId);
    }
    return isScopeCurrent(requestKey, epoch, expectedSessionId);
  }, [isScopeCurrent, isSessionMutationRevisionCurrent, refreshForScope]);

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
      taskStarted = true;
      trackPathTask(task, { projectKey, epoch, existingItemIds, mutationKey });
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch)) pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
      throw error;
    } finally {
      if (!taskStarted) useImportStore.getState().endMutation(mutationKey);
    }
  }, [isScopeCurrent, nextSessionMutationRevision, projectId, projectKey, pushToast, rootPath, t, trackPathTask]);

  const addUrl = useCallback(async (url: string, mediaSaveMode?: MediaSaveMode) => {
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
        ...(mediaSaveMode ? { mediaSaveMode } : {}),
      });
      if (await reconcileMutationSession(
        nextSession,
        projectKey,
        epoch,
        mutationRevision,
        current.session.sessionId,
      )) {
        await startNewQueuedItems(projectKey, epoch, existingItemIds);
      }
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch)) pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
      throw error;
    } finally {
      useImportStore.getState().endMutation(mutationKey);
    }
  }, [isScopeCurrent, nextSessionMutationRevision, projectId, projectKey, pushToast, reconcileMutationSession, rootPath, startNewQueuedItems, t]);

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
      await reconcileMutationSession(nextSession, projectKey, epoch, mutationRevision, current.session.sessionId);
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch, current.session.sessionId)) {
        if (isSessionMutationRevisionCurrent(mutationRevision)) {
          useImportStore.getState().replaceItem(projectKey, originalItem, epoch);
        } else {
          void refreshForScope(projectKey, epoch, current.session.sessionId).catch(() => undefined);
        }
        pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
      }
    } finally {
      endPendingItems(acceptedIds, projectKey, epoch);
    }
  }, [beginPendingItems, endPendingItems, isScopeCurrent, isSessionMutationRevisionCurrent, nextSessionMutationRevision, projectId, projectKey, pushToast, reconcileMutationSession, refreshForScope, rootPath, t]);

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
      trackStartedItems(tasks, acceptedIds, projectKey, epoch, current.session.sessionId);
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch)) pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
      endPendingItems(acceptedIds, projectKey, epoch);
    }
  }, [beginPendingItems, endPendingItems, isScopeCurrent, nextSessionMutationRevision, projectId, projectKey, pushToast, rootPath, t, trackStartedItems]);

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
      await reconcileMutationSession(nextSession, projectKey, epoch, mutationRevision, current.session!.sessionId);
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch)) pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
    } finally {
      endPendingItems(acceptedIds, projectKey, epoch);
    }
  }, [beginPendingItems, endPendingItems, isScopeCurrent, nextSessionMutationRevision, projectId, projectKey, pushToast, reconcileMutationSession, rootPath, t]);

  const authorizeLocalAsr = useCallback(async (itemId: string) => {
    const current = useImportStore.getState();
    if (!current.session || current.projectKey !== projectKey || !current.session.items.some((item) => item.itemId === itemId)) return;
    const epoch = current.sessionEpoch;
    nextSessionMutationRevision();
    try {
      await importV2Api.authorizeLocalAsr({
        projectId,
        projectRootPath: rootPath,
        sessionId: current.session.sessionId,
        itemId,
      });
      if (!isScopeCurrent(projectKey, epoch, current.session.sessionId)) return;
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
          await reconcileMutationSession(nextSession, projectKey, epoch, mutationRevision, current.session!.sessionId);
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
  }, [beginPendingItems, endPendingItems, isScopeCurrent, nextSessionMutationRevision, projectId, projectKey, pushToast, reconcileMutationSession, refreshForScope, rootPath, t, taskLauncher]);

  const confirm = useCallback(async (decisions: CommitItemDecision[]) => {
    const current = useImportStore.getState();
    if (!current.session || current.projectKey !== projectKey || decisions.length === 0 || current.isConfirming) return;
    const epoch = current.sessionEpoch;
    current.setIsConfirming(true);
    try {
      const task = await importV2Api.confirmSession({ projectId, projectRootPath: rootPath, sessionId: current.session.sessionId, decisions });
      trackConfirmationTask(task, { projectKey, epoch });
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch)) {
        useImportStore.getState().setIsConfirming(false);
        pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
      }
    }
  }, [isScopeCurrent, projectId, projectKey, pushToast, rootPath, t, trackConfirmationTask]);

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
      if (!isProjectCurrent(projectKey)) throw new Error("Preview identity is no longer current");
      return content;
    } catch (error) {
      if (isProjectCurrent(projectKey)) pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
      throw error;
    }
  }, [projectId, projectKey, pushToast, rootPath, t]);

  const loadSession = useCallback(async (sessionId: string, historyBatchId: string | null = null) => {
    if (!sessionId || !isProjectCurrent(projectKey)) return null;
    try {
      const result = await importV2Api.getHistorySession({ projectId, projectRootPath: rootPath, sessionId, historyBatchId });
      return isProjectCurrent(projectKey) && useImportStore.getState().projectKey === projectKey ? result : null;
    } catch (error) {
      if (isProjectCurrent(projectKey)) pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
      return null;
    }
  }, [projectId, projectKey, pushToast, rootPath, t]);

  const visibleItems = useMemo(() => selectVisibleItems(session, filter), [filter, session]);
  const counts = useMemo(() => selectQueueCounts(session), [session]);
  const progress = useMemo(() => selectSessionProgress(session), [session]);
  const selectItem = useImportStore((state) => state.selectItem);
  const setFilter = useImportStore((state) => state.setFilter);

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
  const {
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
  } = useImportSupportingActions({
    projectId,
    rootPath,
    projectKey,
    isProjectCurrent,
    isScopeCurrent,
    refreshForScope,
    startItems,
    trackCapabilityTask,
  });

  return {
    projectKey,
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
    isConfirming,
    refreshSession,
    selectItem,
    setFilter,
    requestClipboard,
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
  };
}
