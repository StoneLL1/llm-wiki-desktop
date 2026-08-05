import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import type { TaskLauncher } from "../../hooks/useTaskLauncher";
import { registerTaskEventListener } from "../../hooks/useTaskEvents";
import { importV2Api } from "../../services/importV2Api";
import { useWikiStore } from "../wiki/wikiStore";
import { useImportStore } from "../../stores/importStore";
import { fetchTasks, useTaskStore } from "../../stores/taskStore";
import { useToastStore } from "../../stores/toastStore";
import type { ImportSession, ImportSessionPatchEvent } from "../../types/importV2";
import type { FileScanResult } from "../../types/importV2File";
import { isTerminalStatus, type BackendEvent, type BackendTask } from "../../types/task";
import { importWorkflowErrorMessage } from "./useImportSessionScope";
import { mergeImportItemTask } from "./importTaskProgress";

interface PendingPathTask {
  projectKey: string;
  epoch: number;
  existingItemIds: ReadonlySet<string>;
  mutationKey: string;
}

interface TrackedPathTask extends PendingPathTask {
  settleQueue: () => void;
}

interface PendingItemTask {
  projectKey: string;
  epoch: number;
  itemIds: readonly string[];
  operation: boolean;
}

interface PendingScopedTask {
  projectKey: string;
  epoch: number;
}

interface EarlyOperationPatch {
  projectKey: string;
  epoch: number;
  sessionId: string;
  items: Map<string, ImportSessionPatchEvent["items"][number]>;
  counts: ImportSessionPatchEvent["counts"];
}

interface ImportTaskCoordinatorOptions {
  projectId: string;
  rootPath: string;
  projectKey: string;
  sessionEpoch: number;
  session: ImportSession | null;
  taskList: readonly BackendTask[];
  tasksHydrated: boolean;
  taskLauncher: TaskLauncher;
  isProjectCurrent: (requestKey: string) => boolean;
  isScopeCurrent: (requestKey: string, epoch: number, expectedSessionId?: string) => boolean;
  nextSessionMutationRevision: () => number;
  refreshForScope: (requestKey: string, epoch: number, expectedSessionId?: string) => Promise<void>;
  recordItemBatch: (
    tasks: readonly BackendTask[],
    itemIds: readonly string[],
    requestKey: string,
    epoch: number,
    sessionId: string,
  ) => void;
}

export interface ImportTaskCoordinator {
  pendingItemIds: ReadonlySet<string>;
  discoveryTask: BackendTask | null;
  discoveryScan: FileScanResult | null;
  discoveryTaskUnavailable: boolean;
  beginPendingItems: (itemIds: readonly string[], requestKey: string, epoch: number) => string[];
  endPendingItems: (itemIds: readonly string[], requestKey: string, epoch: number) => void;
  startNewQueuedItems: (requestKey: string, epoch: number, before: ReadonlySet<string>) => Promise<void>;
  trackStartedItems: (
    tasks: readonly BackendTask[],
    itemIds: readonly string[],
    requestKey: string,
    epoch: number,
    sessionId: string,
  ) => void;
  trackPathTask: (
    task: BackendTask,
    pending: PendingPathTask,
  ) => Promise<void>;
  trackConfirmationTask: (task: BackendTask, pending: PendingScopedTask) => void;
  trackCapabilityTask: (task: BackendTask, pending: PendingScopedTask) => void;
  reconcilePendingTasks: (tasks?: readonly BackendTask[]) => void;
  cancelDiscovery: () => Promise<void>;
  acceptDiscovery: (sourcePaths?: readonly string[]) => Promise<void>;
  dismissDiscovery: () => Promise<void>;
}

function isTerminalTaskEvent(event: BackendEvent): boolean {
  return event.eventType === "task_completed"
    || event.eventType === "task_failed"
    || event.eventType === "task_cancelled";
}

function isTaskSnapshotEvent(event: BackendEvent): boolean {
  return event.eventType === "task_updated"
    || event.eventType === "task_completed"
    || event.eventType === "task_failed"
    || event.eventType === "task_cancelled"
    || event.eventType === "confirmation_requested";
}

function isWaitingTaskEvent(event: BackendEvent): boolean {
  return (event.eventType === "task_updated" || event.eventType === "confirmation_requested")
    && (event.payload as Partial<BackendTask> | null)?.status === "waiting_for_confirmation";
}

function isSettledImportTask(task: BackendTask): boolean {
  return isTerminalStatus(task.status) || task.status === "waiting_for_confirmation";
}

export function useImportTaskCoordinator({
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
}: ImportTaskCoordinatorOptions): ImportTaskCoordinator {
  const { t } = useTranslation();
  const pushToast = useToastStore((state) => state.pushToast);
  const selectedTaskUpsert = useTaskStore((state) => state.upsertTask);
  const selectedTasksUpsert = useTaskStore((state) => state.upsertTasks);
  const openTaskDrawer = useTaskStore((state) => state.openDrawer);
  const pendingPathTasks = useRef(new Map<string, TrackedPathTask>());
  const pendingItemTasks = useRef(new Map<string, PendingItemTask>());
  const settledOperationTaskIdsRef = useRef(new Set<string>());
  const earlyOperationPatchesRef = useRef(new Map<string, EarlyOperationPatch>());
  const pendingConfirmationTasks = useRef(new Map<string, PendingScopedTask>());
  const pendingCapabilityTasks = useRef(new Map<string, PendingScopedTask>());
  const pendingActionKeysRef = useRef(new Set<string>());
  const consumedCompletionTaskIdsRef = useRef(new Set<string>());
  const [pendingActionKeys, setPendingActionKeys] = useState<ReadonlySet<string>>(new Set());
  const [discoveryTaskId, setDiscoveryTaskId] = useState<string | null>(null);
  const [discoveryScan, setDiscoveryScan] = useState<FileScanResult | null>(null);
  const discoveryScanTaskIdRef = useRef<string | null>(null);
  const discoveryScanLoadingTaskIdRef = useRef<string | null>(null);
  const discoverySessionIdRef = useRef<string | null>(null);
  const trackedDiscoveryTaskIdsRef = useRef(new Set<string>());
  const reconciliationDelayRef = useRef(250);
  const [reconciliationRevision, setReconciliationRevision] = useState(0);

  const requestTaskReconciliation = useCallback(() => {
    reconciliationDelayRef.current = 250;
    setReconciliationRevision((revision) => revision + 1);
  }, []);

  const beginPendingItems = useCallback((
    itemIds: readonly string[],
    requestKey: string,
    epoch: number,
  ): string[] => {
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

  const endPendingItems = useCallback((
    itemIds: readonly string[],
    requestKey: string,
    epoch: number,
  ) => {
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

  const pendingItemIds = useMemo(() => {
    const prefix = `${projectKey}\0${sessionEpoch}\0`;
    const ids = new Set<string>();
    for (const key of pendingActionKeys) {
      if (key.startsWith(prefix)) ids.add(key.slice(prefix.length));
    }
    return ids;
  }, [pendingActionKeys, projectKey, sessionEpoch]);

  const hasCurrentSession = session?.projectId === projectId;
  const discoveryTask = useMemo(
    () => hasCurrentSession && discoveryTaskId
      ? taskList.find((task) => task.id === discoveryTaskId && task.projectId === projectId) ?? null
      : null,
    [discoveryTaskId, hasCurrentSession, projectId, taskList],
  );
  const discoveryTaskUnavailable = Boolean(hasCurrentSession && discoveryTaskId && tasksHydrated && !discoveryTask);

  const loadDiscoveryScan = useCallback(async (
    taskId: string,
    requestKey: string,
    epoch: number,
    sessionId: string,
  ) => {
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
      // Scan details are supplementary and older sessions may not have them.
    } finally {
      if (discoveryScanLoadingTaskIdRef.current === taskId) {
        discoveryScanLoadingTaskIdRef.current = null;
      }
    }
  }, [isScopeCurrent, projectId, rootPath]);

  const registerItemTasks = useCallback((
    tasks: readonly BackendTask[],
    itemIds: readonly string[],
    requestKey: string,
    epoch: number,
  ) => {
    const operation = tasks.length === 1
      && tasks[0]?.batchId?.startsWith("import-v2-operation:")
      ? tasks[0]
      : null;
    if (operation) {
      if (settledOperationTaskIdsRef.current.has(operation.id)) {
        endPendingItems(itemIds, requestKey, epoch);
        return [...itemIds];
      }
      if (isSettledImportTask(operation)) {
        settledOperationTaskIdsRef.current.add(operation.id);
        endPendingItems(itemIds, requestKey, epoch);
        return [...itemIds];
      }
      pendingItemTasks.current.set(operation.id, {
        projectKey: requestKey,
        epoch,
        itemIds: [...itemIds],
        operation: true,
      });
      return [];
    }
    const trackedIds = new Set<string>();
    const terminalIds: string[] = [];
    tasks.forEach((task, index) => {
      const itemId = itemIds[index];
      if (!itemId) return;
      trackedIds.add(itemId);
      if (isSettledImportTask(task)) {
        terminalIds.push(itemId);
      } else {
        pendingItemTasks.current.set(task.id, {
          projectKey: requestKey,
          epoch,
          itemIds: [itemId],
          operation: false,
        });
      }
    });
    const untrackedIds = itemIds.filter((itemId) => !trackedIds.has(itemId));
    endPendingItems([...untrackedIds, ...terminalIds], requestKey, epoch);
    return terminalIds;
  }, [endPendingItems]);

  const syncItemTask = useCallback((
    task: BackendTask,
    itemId: string,
    requestKey: string,
    epoch: number,
    allowBinding = false,
  ) => {
    const current = useImportStore.getState();
    if (current.projectKey !== requestKey || current.sessionEpoch !== epoch || !current.session) return;
    const item = current.session.items.find((candidate) => candidate.itemId === itemId);
    if (!item) return;
    const next = mergeImportItemTask(item, task, allowBinding);
    if (next !== item) current.replaceItem(requestKey, next, epoch);
  }, []);

  const consumeTaskCompletion = useCallback((
    task: BackendTask,
    requestKey: string,
    epoch: number,
    expectedSessionId?: string,
  ): boolean => {
    if (task.status !== "succeeded" || consumedCompletionTaskIdsRef.current.has(task.id)) {
      return false;
    }
    const reference = task.result?.reference;
    if (
      reference?.type !== "import_v2_session_preview"
      || !reference.batchId
      || reference.sessionId !== expectedSessionId
      || !isScopeCurrent(requestKey, epoch, expectedSessionId)
    ) {
      return false;
    }
    consumedCompletionTaskIdsRef.current.add(task.id);
    if (reference.completion) {
      useImportStore.getState().setCompletion(requestKey, reference.completion, epoch);
    }
    void useWikiStore.getState().scan(projectId, rootPath);
    return true;
  }, [isScopeCurrent, projectId, rootPath]);

  const settleItemTask = useCallback((task: BackendTask): boolean => {
    const pending = pendingItemTasks.current.get(task.id);
    if (!pending || !isSettledImportTask(task)) return false;
    if (pending.operation) return false;
    pendingItemTasks.current.delete(task.id);
    endPendingItems(pending.itemIds, pending.projectKey, pending.epoch);
    if (isScopeCurrent(pending.projectKey, pending.epoch)) {
      void refreshForScope(pending.projectKey, pending.epoch).catch(() => undefined);
    }
    return true;
  }, [endPendingItems, isScopeCurrent, refreshForScope]);

  const settleOperationTask = useCallback((task: BackendTask): boolean => {
    const pending = pendingItemTasks.current.get(task.id);
    if (!pending?.operation || !isSettledImportTask(task)) return false;
    pendingItemTasks.current.delete(task.id);
    endPendingItems(pending.itemIds, pending.projectKey, pending.epoch);
    if (!settledOperationTaskIdsRef.current.has(task.id)) {
      settledOperationTaskIdsRef.current.add(task.id);
      if (isScopeCurrent(pending.projectKey, pending.epoch)) {
        void refreshForScope(pending.projectKey, pending.epoch).catch(() => undefined);
      }
    }
    return true;
  }, [endPendingItems, isScopeCurrent, refreshForScope]);

  const applyOperationPatch = useCallback((
    patch: ImportSessionPatchEvent,
    requestKey: string,
    epoch: number,
  ) => {
    const current = useImportStore.getState();
    if (
      patch.projectId !== projectId
      || patch.projectRootPath !== rootPath
      || current.projectKey !== requestKey
      || current.session?.sessionId !== patch.sessionId
      || !isScopeCurrent(requestKey, epoch, patch.sessionId)
    ) return;
    nextSessionMutationRevision();
    current.patchItems(requestKey, patch.items, epoch);
    if (patch.counts.processed !== patch.counts.total) return;
    const firstTerminalPatch = !settledOperationTaskIdsRef.current.has(patch.batchId);
    settledOperationTaskIdsRef.current.add(patch.batchId);
    const pending = pendingItemTasks.current.get(patch.batchId);
    if (pending?.operation) {
      pendingItemTasks.current.delete(patch.batchId);
      endPendingItems(pending.itemIds, pending.projectKey, pending.epoch);
    }
    if (firstTerminalPatch) {
      void refreshForScope(requestKey, epoch, patch.sessionId).catch(() => undefined);
    }
  }, [endPendingItems, isScopeCurrent, nextSessionMutationRevision, projectId, refreshForScope, rootPath]);

  const trackStartedItems = useCallback((
    tasks: readonly BackendTask[],
    itemIds: readonly string[],
    requestKey: string,
    epoch: number,
    sessionId: string,
  ) => {
    selectedTasksUpsert(tasks);
    if (!isScopeCurrent(requestKey, epoch, sessionId)) {
      endPendingItems(itemIds, requestKey, epoch);
      return;
    }
    const resolvedTasks = tasks.map(
      (task) => useTaskStore.getState().tasks.find((current) => current.id === task.id) ?? task,
    );
    const operation = resolvedTasks.length === 1
      && resolvedTasks[0]?.batchId?.startsWith("import-v2-operation:");
    resolvedTasks.forEach((task, index) => {
      const itemId = itemIds[index];
      if (!operation && itemId) syncItemTask(task, itemId, requestKey, epoch, true);
      consumeTaskCompletion(task, requestKey, epoch, sessionId);
    });
    recordItemBatch(resolvedTasks, itemIds, requestKey, epoch, sessionId);
    const terminalIds = registerItemTasks(resolvedTasks, itemIds, requestKey, epoch);
    const operationTask = resolvedTasks.length === 1
      && resolvedTasks[0]?.batchId?.startsWith("import-v2-operation:")
      ? resolvedTasks[0]
      : null;
    if (operationTask) {
      const early = earlyOperationPatchesRef.current.get(operationTask.id);
      if (
        early
        && early.projectKey === requestKey
        && early.epoch === epoch
        && early.sessionId === sessionId
      ) {
        earlyOperationPatchesRef.current.delete(operationTask.id);
        applyOperationPatch({
          projectId,
          projectRootPath: rootPath,
          sessionId,
          batchId: operationTask.id,
          items: [...early.items.values()],
          counts: early.counts,
        }, requestKey, epoch);
      }
    }
    if (terminalIds.length > 0 && isScopeCurrent(requestKey, epoch)) {
      void refreshForScope(requestKey, epoch).catch(() => undefined);
    }
    for (const task of resolvedTasks) settleItemTask(task);
    requestTaskReconciliation();
  }, [applyOperationPatch, consumeTaskCompletion, endPendingItems, isScopeCurrent, projectId, recordItemBatch, refreshForScope, registerItemTasks, requestTaskReconciliation, rootPath, selectedTasksUpsert, settleItemTask, syncItemTask]);

  const startNewQueuedItems = useCallback(async (
    requestKey: string,
    epoch: number,
    before: ReadonlySet<string>,
  ) => {
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
      const task = await importV2Api.startBatch({
        projectId,
        projectRootPath: rootPath,
        sessionId: current.sessionId,
        itemIds: acceptedIds,
      });
      trackStartedItems([task], acceptedIds, requestKey, epoch, current.sessionId);
    } catch (error) {
      if (isScopeCurrent(requestKey, epoch)) {
        pushToast("error", t("importV2.workflow.error", { message: importWorkflowErrorMessage(error) }));
      }
      endPendingItems(acceptedIds, requestKey, epoch);
    }
  }, [beginPendingItems, endPendingItems, isScopeCurrent, nextSessionMutationRevision, projectId, pushToast, rootPath, t, trackStartedItems]);

  const settlePathTask = useCallback((task: BackendTask): boolean => {
    const pending = pendingPathTasks.current.get(task.id);
    if (!pending || !isSettledImportTask(task)) return false;
    pendingPathTasks.current.delete(task.id);
    useImportStore.getState().endMutation(pending.mutationKey);
    if (task.status === "succeeded") {
      void refreshForScope(pending.projectKey, pending.epoch)
        .then(() => startNewQueuedItems(pending.projectKey, pending.epoch, pending.existingItemIds))
        .catch(() => undefined)
        .finally(pending.settleQueue);
      const currentSessionId = useImportStore.getState().session?.sessionId;
      if (currentSessionId) {
        void loadDiscoveryScan(task.id, pending.projectKey, pending.epoch, currentSessionId);
      }
    } else {
      pending.settleQueue();
    }
    return true;
  }, [loadDiscoveryScan, refreshForScope, startNewQueuedItems]);

  const settleConfirmationTask = useCallback((task: BackendTask, pending: PendingScopedTask) => {
    pendingConfirmationTasks.current.delete(task.id);
    if (!isScopeCurrent(pending.projectKey, pending.epoch)) return;
    useImportStore.getState().setIsConfirming(false);
    const expectedSessionId = useImportStore.getState().session?.sessionId;
    consumeTaskCompletion(task, pending.projectKey, pending.epoch, expectedSessionId);
    void refreshForScope(pending.projectKey, pending.epoch).catch(() => undefined);
    if (task.status === "failed") pushToast("error", t("importV2.workflow.confirmFailed"));
  }, [consumeTaskCompletion, isScopeCurrent, pushToast, refreshForScope, t]);

  const reconcilePendingTasks = useCallback((
    tasks: readonly BackendTask[] = useTaskStore.getState().tasks,
  ) => {
    const current = useImportStore.getState();
    if (current.projectKey === projectKey && current.session) {
      for (const task of tasks) {
        if (task.projectId === projectId) {
          consumeTaskCompletion(task, projectKey, current.sessionEpoch, current.session.sessionId);
        }
      }
    }
    const byId = new Map(tasks.map((task) => [task.id, task]));
    for (const taskId of pendingPathTasks.current.keys()) {
      const task = byId.get(taskId);
      if (task) settlePathTask(task);
    }
    for (const taskId of pendingItemTasks.current.keys()) {
      const task = byId.get(taskId);
      if (task && !settleOperationTask(task)) settleItemTask(task);
    }
    for (const [taskId, pending] of pendingConfirmationTasks.current) {
      const task = byId.get(taskId);
      if (task && isTerminalStatus(task.status)) settleConfirmationTask(task, pending);
    }
    for (const [taskId, pending] of pendingCapabilityTasks.current) {
      const task = byId.get(taskId);
      if (!task || !isSettledImportTask(task)) continue;
      pendingCapabilityTasks.current.delete(taskId);
      if (isScopeCurrent(pending.projectKey, pending.epoch)) {
        void refreshForScope(pending.projectKey, pending.epoch).catch(() => undefined);
      }
    }
  }, [consumeTaskCompletion, isScopeCurrent, projectId, projectKey, refreshForScope, settleConfirmationTask, settleItemTask, settleOperationTask, settlePathTask]);

  const trackPathTask = useCallback((task: BackendTask, pending: PendingPathTask) => {
    const knownTask = useTaskStore.getState().tasks.find((candidate) => candidate.id === task.id);
    const observedTask = knownTask && isSettledImportTask(knownTask) ? knownTask : task;
    selectedTaskUpsert(observedTask);
    if (!isScopeCurrent(pending.projectKey, pending.epoch)) {
      useImportStore.getState().endMutation(pending.mutationKey);
      return Promise.resolve();
    }
    return new Promise<void>((settleQueue) => {
      trackedDiscoveryTaskIdsRef.current.add(task.id);
      pendingPathTasks.current.set(task.id, { ...pending, settleQueue });
      if (isScopeCurrent(pending.projectKey, pending.epoch)) {
        setDiscoveryTaskId(task.id);
        discoveryScanTaskIdRef.current = task.id;
        discoveryScanLoadingTaskIdRef.current = null;
        setDiscoveryScan(null);
      }
      settlePathTask(observedTask);
      requestTaskReconciliation();
    });
  }, [isScopeCurrent, requestTaskReconciliation, selectedTaskUpsert, settlePathTask]);

  const trackConfirmationTask = useCallback((task: BackendTask, pending: PendingScopedTask) => {
    const knownTask = useTaskStore.getState().tasks.find((candidate) => candidate.id === task.id);
    const observedTask = knownTask && isTerminalStatus(knownTask.status) ? knownTask : task;
    selectedTaskUpsert(observedTask);
    if (!isScopeCurrent(pending.projectKey, pending.epoch)) return;
    pendingConfirmationTasks.current.set(task.id, pending);
    if (isScopeCurrent(pending.projectKey, pending.epoch)) openTaskDrawer(task.id);
    if (isTerminalStatus(observedTask.status)) settleConfirmationTask(observedTask, pending);
    requestTaskReconciliation();
  }, [isScopeCurrent, openTaskDrawer, requestTaskReconciliation, selectedTaskUpsert, settleConfirmationTask]);

  const trackCapabilityTask = useCallback((task: BackendTask, pending: PendingScopedTask) => {
    selectedTaskUpsert(task);
    if (!isScopeCurrent(pending.projectKey, pending.epoch)) return;
    pendingCapabilityTasks.current.set(task.id, pending);
    openTaskDrawer(task.id);
    reconcilePendingTasks([task]);
    requestTaskReconciliation();
  }, [isScopeCurrent, openTaskDrawer, reconcilePendingTasks, requestTaskReconciliation, selectedTaskUpsert]);

  const handleTaskEvent = useCallback((event: BackendEvent) => {
    if (event.eventType === "import_session_patch") {
      const patch = event.payload as ImportSessionPatchEvent;
      const current = useImportStore.getState();
      if (
        event.projectId !== projectId
        || event.taskId !== patch.batchId
        || patch.projectId !== projectId
        || patch.projectRootPath !== rootPath
        || current.projectKey !== projectKey
        || current.session?.sessionId !== patch.sessionId
        || !isScopeCurrent(projectKey, current.sessionEpoch, patch.sessionId)
      ) return;
      const pending = pendingItemTasks.current.get(patch.batchId);
      if (pending?.operation) {
        applyOperationPatch(patch, pending.projectKey, pending.epoch);
        return;
      }
      if (settledOperationTaskIdsRef.current.has(patch.batchId)) return;
      const prefix = `${projectKey}\0${current.sessionEpoch}\0`;
      const pendingIds = new Set(
        [...pendingActionKeysRef.current]
          .filter((key) => key.startsWith(prefix))
          .map((key) => key.slice(prefix.length)),
      );
      if (
        pendingIds.size === 0
        || patch.items.some((item) => !pendingIds.has(item.itemId))
      ) return;
      const existing = earlyOperationPatchesRef.current.get(patch.batchId);
      const early = existing
        && existing.projectKey === projectKey
        && existing.epoch === current.sessionEpoch
        && existing.sessionId === patch.sessionId
        ? existing
        : {
            projectKey,
            epoch: current.sessionEpoch,
            sessionId: patch.sessionId,
            items: new Map(),
            counts: patch.counts,
          };
      for (const item of patch.items) early.items.set(item.itemId, item);
      early.counts = patch.counts;
      earlyOperationPatchesRef.current.set(patch.batchId, early);
      return;
    }
    if (!event.taskId || !isTaskSnapshotEvent(event)) return;
    const payloadTask = event.payload as BackendTask;
    selectedTaskUpsert(payloadTask);
    const task = useTaskStore.getState().tasks.find((candidate) => candidate.id === event.taskId)
      ?? payloadTask;
    const currentScope = useImportStore.getState();
    if (event.projectId === projectId && currentScope.projectKey === projectKey && currentScope.session) {
      consumeTaskCompletion(
        task,
        projectKey,
        currentScope.sessionEpoch,
        currentScope.session.sessionId,
      );
    }
    const pendingItem = pendingItemTasks.current.get(event.taskId);
    if (pendingItem && !pendingItem.operation) {
      for (const itemId of pendingItem.itemIds) {
        syncItemTask(task, itemId, pendingItem.projectKey, pendingItem.epoch);
      }
    }
    if (!isTerminalTaskEvent(event) && !isWaitingTaskEvent(event)) return;
    settlePathTask(task);
    const confirmation = pendingConfirmationTasks.current.get(event.taskId);
    if (confirmation && isTerminalStatus(task.status)) {
      settleConfirmationTask(task, confirmation);
    }
    if (pendingItem?.operation && isTerminalTaskEvent(event)) {
      settleOperationTask(task);
    } else {
      settleItemTask(task);
    }
    const capability = pendingCapabilityTasks.current.get(event.taskId);
    if (capability && isTerminalTaskEvent(event)) {
      pendingCapabilityTasks.current.delete(event.taskId);
      if (isScopeCurrent(capability.projectKey, capability.epoch)) {
        void refreshForScope(capability.projectKey, capability.epoch).catch(() => undefined);
      }
    }
    const current = useImportStore.getState();
    if (event.projectId !== projectId || current.projectKey !== projectKey || !current.session) return;
    if (
      !settledOperationTaskIdsRef.current.has(event.taskId)
      && current.session.items.some((item) => item.taskId === event.taskId)
    ) {
      void refreshForScope(projectKey, current.sessionEpoch).catch(() => undefined);
    }
  }, [applyOperationPatch, consumeTaskCompletion, isScopeCurrent, projectId, projectKey, refreshForScope, rootPath, selectedTaskUpsert, settleConfirmationTask, settleItemTask, settleOperationTask, settlePathTask, syncItemTask]);

  const cancelDiscovery = useCallback(async () => {
    if (!discoveryTaskId) return;
    try {
      await taskLauncher.cancel(discoveryTaskId);
    } catch (error) {
      if (isProjectCurrent(projectKey)) {
        pushToast("error", t("importV2.workflow.error", { message: importWorkflowErrorMessage(error) }));
      }
      throw error;
    }
  }, [discoveryTaskId, isProjectCurrent, projectKey, pushToast, t, taskLauncher]);

  const acceptDiscovery = useCallback(async (sourcePaths?: readonly string[]) => {
    const current = useImportStore.getState();
    const taskId = discoveryTaskId;
    const scan = discoveryScan;
    const confirmationToken = scan?.confirmationToken;
    if (!current.session || !taskId || !confirmationToken || current.projectKey !== projectKey) return;
    const epoch = current.sessionEpoch;
    const sessionId = current.session.sessionId;
    const existingItemIds = new Set(current.session.items.map((item) => item.itemId));
    const mutationKey = `add-paths:${projectKey}:${epoch}:accept:${taskId}`;
    nextSessionMutationRevision();
    current.beginMutation(mutationKey);
    try {
      const nextSession = await importV2Api.acceptScan({
        projectId,
        projectRootPath: rootPath,
        sessionId,
        taskId,
        confirmationToken,
        ...(!sourcePaths && scan?.totals?.requiresConfirmation && !scan.aggregateConfirmedAt
          ? { acknowledgeAggregate: true }
          : {}),
        ...(sourcePaths && sourcePaths.length > 0 ? { sourcePaths: [...sourcePaths] } : {}),
      });
      if (!isScopeCurrent(projectKey, epoch, sessionId)) return;
      useImportStore.getState().replaceSession(projectKey, nextSession.session, epoch);
      if (nextSession.scan.acceptedAt) {
        setDiscoveryTaskId(null);
        setDiscoveryScan(null);
      } else {
        setDiscoveryScan(nextSession.scan);
      }
      await startNewQueuedItems(projectKey, epoch, existingItemIds);
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch, sessionId)) {
        pushToast("error", t("importV2.workflow.error", { message: importWorkflowErrorMessage(error) }));
      }
      throw error;
    } finally {
      useImportStore.getState().endMutation(mutationKey);
    }
  }, [discoveryScan, discoveryTaskId, isScopeCurrent, nextSessionMutationRevision, projectId, projectKey, pushToast, rootPath, startNewQueuedItems, t]);

  const dismissDiscovery = useCallback(async () => {
    const current = useImportStore.getState();
    const taskId = discoveryTaskId;
    const confirmationToken = discoveryScan?.confirmationToken;
    const sessionId = current.session?.sessionId;
    const epoch = current.sessionEpoch;
    if (taskId && confirmationToken && sessionId && current.projectKey === projectKey) {
      try {
        await importV2Api.discardScan({
          projectId,
          projectRootPath: rootPath,
          sessionId,
          taskId,
          confirmationToken,
        });
      } catch (error) {
        if (isScopeCurrent(projectKey, epoch, sessionId)) {
          pushToast("error", t("importV2.workflow.error", { message: importWorkflowErrorMessage(error) }));
        }
        throw error;
      }
      if (!isScopeCurrent(projectKey, epoch, sessionId)) return;
    }
    setDiscoveryTaskId(null);
    setDiscoveryScan(null);
  }, [discoveryScan?.confirmationToken, discoveryTaskId, isScopeCurrent, projectId, projectKey, pushToast, rootPath, t]);

  const hasPendingTaskForCurrentScope = useCallback(() => {
    const pendingScopes = [
      ...pendingPathTasks.current.values(),
      ...pendingItemTasks.current.values(),
      ...pendingConfirmationTasks.current.values(),
      ...pendingCapabilityTasks.current.values(),
    ];
    return pendingScopes.some((pending) => isScopeCurrent(pending.projectKey, pending.epoch));
  }, [isScopeCurrent]);

  useEffect(() => registerTaskEventListener(handleTaskEvent), [handleTaskEvent]);

  useEffect(() => {
    reconcilePendingTasks(taskList);
  }, [reconcilePendingTasks, taskList]);

  useEffect(() => {
    if (!hasPendingTaskForCurrentScope()) return;
    let cancelled = false;
    const timer = setTimeout(() => {
      void fetchTasks(projectId, rootPath)
        .then(() => reconcilePendingTasks())
        .catch(() => undefined)
        .finally(() => {
          if (cancelled) return;
          reconciliationDelayRef.current = Math.min(5_000, reconciliationDelayRef.current * 2);
          setReconciliationRevision((revision) => revision + 1);
        });
    }, reconciliationDelayRef.current);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [hasPendingTaskForCurrentScope, projectId, projectKey, reconcilePendingTasks, reconciliationRevision, rootPath]);

  useEffect(() => {
    if (discoveryTask?.status !== "succeeded" || !session?.sessionId) return;
    if (discoveryScanTaskIdRef.current === discoveryTask.id && discoveryScan) return;
    discoveryScanTaskIdRef.current = discoveryTask.id;
    void loadDiscoveryScan(discoveryTask.id, projectKey, sessionEpoch, session.sessionId);
  }, [discoveryScan, discoveryTask?.id, discoveryTask?.status, loadDiscoveryScan, projectKey, session?.sessionId, sessionEpoch]);

  useEffect(() => {
    for (const pending of pendingPathTasks.current.values()) pending.settleQueue();
    pendingPathTasks.current.clear();
    pendingItemTasks.current.clear();
    settledOperationTaskIdsRef.current.clear();
    earlyOperationPatchesRef.current.clear();
    pendingConfirmationTasks.current.clear();
    pendingCapabilityTasks.current.clear();
    pendingActionKeysRef.current.clear();
    consumedCompletionTaskIdsRef.current.clear();
    setPendingActionKeys(new Set());
    setDiscoveryTaskId(null);
    discoverySessionIdRef.current = null;
    trackedDiscoveryTaskIdsRef.current.clear();
    discoveryScanTaskIdRef.current = null;
    discoveryScanLoadingTaskIdRef.current = null;
    reconciliationDelayRef.current = 250;
    setDiscoveryScan(null);
  }, [projectKey, sessionEpoch]);

  useEffect(() => {
    if (!session || session.projectId !== projectId) return;
    if (discoverySessionIdRef.current === session.sessionId) return;
    discoverySessionIdRef.current = session.sessionId;
    setDiscoveryTaskId(session.discoveryTaskId ?? null);
  }, [projectId, session]);

  useEffect(() => {
    if (!session || session.projectId !== projectId || !session.discoveryTaskId) return;
    const taskId = session.discoveryTaskId;
    if (trackedDiscoveryTaskIdsRef.current.has(taskId) || pendingPathTasks.current.has(taskId)) return;
    const task = taskList.find((candidate) => candidate.id === taskId && candidate.projectId === projectId);
    if (!task) return;
    const mutationKey = `add-paths:${projectKey}:${sessionEpoch}:recovered:${taskId}`;
    useImportStore.getState().beginMutation(mutationKey);
    void trackPathTask(task, {
      projectKey,
      epoch: sessionEpoch,
      existingItemIds: new Set(session.items.map((item) => item.itemId)),
      mutationKey,
    });
  }, [projectId, projectKey, session, sessionEpoch, taskList, trackPathTask]);

  useEffect(() => {
    if (!session || session.projectId !== projectId) return;
    const tasksById = new Map(taskList.map((task) => [task.id, task]));
    const operationItems = new Map<string, string[]>();
    for (const item of session.items) {
      if (!item.taskId) continue;
      const task = tasksById.get(item.taskId);
      if (!task || task.projectId !== projectId || isSettledImportTask(task)) continue;
      if (task.batchId?.startsWith("import-v2-operation:")) {
        const itemIds = operationItems.get(task.id) ?? [];
        itemIds.push(item.itemId);
        operationItems.set(task.id, itemIds);
        continue;
      }
      pendingItemTasks.current.set(task.id, {
        projectKey,
        epoch: sessionEpoch,
        itemIds: [item.itemId],
        operation: false,
      });
      syncItemTask(task, item.itemId, projectKey, sessionEpoch);
    }
    for (const [taskId, itemIds] of operationItems) {
      pendingItemTasks.current.set(taskId, {
        projectKey,
        epoch: sessionEpoch,
        itemIds,
        operation: true,
      });
    }
  }, [projectId, projectKey, session, sessionEpoch, syncItemTask, taskList]);

  return {
    pendingItemIds,
    discoveryTask,
    discoveryScan: hasCurrentSession ? discoveryScan : null,
    discoveryTaskUnavailable,
    beginPendingItems,
    endPendingItems,
    startNewQueuedItems,
    trackStartedItems,
    trackPathTask,
    trackConfirmationTask,
    trackCapabilityTask,
    reconcilePendingTasks,
    cancelDiscovery,
    acceptDiscovery,
    dismissDiscovery,
  };
}
