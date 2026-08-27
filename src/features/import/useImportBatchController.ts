import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useShallow } from "zustand/react/shallow";

import type { TaskLauncher } from "../../hooks/useTaskLauncher";
import { importV2Api } from "../../services/importV2Api";
import { useImportStore } from "../../stores/importStore";
import { useTaskStore } from "../../stores/taskStore";
import { useToastStore } from "../../stores/toastStore";
import type { ImportItem, ImportSession, ImportSessionPatchCounts } from "../../types/importV2";
import { isImportBatchOperationTask, isTerminalStatus, type BackendTask } from "../../types/task";
import type { ImportBatchProgress, ImportBatchTask } from "./importWorkflow";
import { hasImportTauriRuntime } from "./useImportSessionScope";

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
  itemIds?: readonly string[];
  operationTaskId?: string;
}

interface ImportBatchControllerOptions {
  projectId: string;
  rootPath: string;
  projectKey: string;
  sessionEpoch: number;
  session: ImportSession | null;
  taskList: readonly BackendTask[];
  operationCountsByBatchId: Readonly<Record<string, ImportSessionPatchCounts>>;
  operationFailedItemIdsByBatchId: Readonly<Record<string, readonly string[]>>;
  tasksHydrated: boolean;
  taskLauncher: TaskLauncher;
  isScopeCurrent: (requestKey: string, epoch: number, expectedSessionId?: string) => boolean;
}

export interface ImportBatchController {
  batches: readonly ImportBatchProgress[];
  batch: ImportBatchProgress | null;
  isCancellingBatch: boolean;
  isBatchCancelling: (batchId: string) => boolean;
  recordItemBatch: (
    tasks: readonly BackendTask[],
    itemIds: readonly string[],
    requestKey: string,
    epoch: number,
    sessionId: string,
  ) => void;
  cancelBatch: (batchId?: string) => Promise<void>;
  dismissBatch: (batchId?: string) => void;
}

function isRecoverableImportItemStatus(status: ImportItem["status"]): boolean {
  return [
    "queued",
    "inspecting",
    "waiting_capability",
    "waiting_login",
    "waiting_authorization",
    "extracting",
    "validating",
    "preview_ready",
    "committing",
    "paused",
  ].includes(status);
}

export function buildImportBatchProgress(
  records: readonly ImportBatchRecord[],
  taskList: readonly BackendTask[],
  session: ImportSession | null,
  operationCountsByBatchId: Readonly<Record<string, ImportSessionPatchCounts>> = {},
  operationFailedItemIdsByBatchId: Readonly<Record<string, readonly string[]>> = {},
): readonly ImportBatchProgress[] {
  const taskById = new Map(taskList.map((task) => [task.id, task]));
  let itemById: Map<string, ImportItem> | null = null;
  const itemFor = (itemId: string) => {
    itemById ??= new Map((session?.items ?? []).map((item) => [item.itemId, item]));
    return itemById.get(itemId);
  };
  return records.map((record) => {
    if (record.operationTaskId) {
      const operation = taskById.get(record.operationTaskId);
      const itemIds = record.itemIds ?? [];
      const operationCounts = operationCountsByBatchId[record.operationTaskId];
      if (operationCounts) {
        const operationStatus = operation?.status ?? "unknown";
        const active = Math.max(0, operationCounts.total - operationCounts.processed);
        return {
          id: record.id,
          sessionId: record.sessionId,
          total: operationCounts.total,
          taskIds: [record.operationTaskId],
          processed: operationCounts.processed,
          active,
          completed: operationCounts.succeeded,
          waitingForConfirmation: operationCounts.succeeded + operationCounts.waiting,
          reviewReady: operationCounts.succeeded,
          failed: operationCounts.failed,
          cancelled: operationCounts.cancelled,
          cancelling: operationStatus === "cancelling" ? 1 : 0,
          unknown: 0,
          nonCancellable: active > 0 && operation && !operation.cancellable ? 1 : 0,
          failedItemIds: operationFailedItemIdsByBatchId[record.operationTaskId] ?? [],
          tasks: [{
            id: record.operationTaskId,
            itemId: "",
            title: operation?.title ?? "Import batch",
            status: operationStatus,
            cancellable: operation?.cancellable ?? false,
          }],
        };
      }
      let completed = 0;
      let waitingForConfirmation = 0;
      let reviewReady = 0;
      let failed = 0;
      let cancelled = 0;
      let active = 0;
      let unknown = 0;
      const failedItemIds: string[] = [];
      for (const itemId of itemIds) {
        const item = itemFor(itemId);
        if (!item) {
          unknown += 1;
          continue;
        }
        if (item.status === "preview_ready" || item.status === "needs_merge") {
          completed += 1;
          waitingForConfirmation += 1;
          reviewReady += 1;
        } else if (["waiting_capability", "waiting_login", "waiting_authorization", "paused"].includes(item.status)) {
          waitingForConfirmation += 1;
        } else if (item.status === "failed") {
          failed += 1;
          failedItemIds.push(itemId);
        } else if (item.status === "cancelled" || item.status === "skipped") {
          cancelled += 1;
        } else if (item.status === "completed") {
          completed += 1;
        } else {
          active += 1;
        }
      }
      const operationStatus = operation?.status ?? "unknown";
      const operationActive = operation
        ? !isTerminalStatus(operation.status) && operation.status !== "waiting_for_confirmation"
        : false;
      const tasks = [{
        id: record.operationTaskId,
        itemId: "",
        title: operation?.title ?? "Import batch",
        status: operationStatus,
        cancellable: operation?.cancellable ?? false,
      } satisfies ImportBatchTask];
      return {
        id: record.id,
        sessionId: record.sessionId,
        total: itemIds.length,
        taskIds: [record.operationTaskId],
        processed: completed + waitingForConfirmation + failed + cancelled - reviewReady,
        active,
        completed,
        waitingForConfirmation,
        reviewReady,
        failed,
        cancelled,
        cancelling: operationStatus === "cancelling" ? 1 : 0,
        unknown,
        nonCancellable: operationActive && operation && !operation.cancellable ? 1 : 0,
        failedItemIds,
        tasks,
      };
    }
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
        if (itemFor(reference.itemId)?.status === "preview_ready") reviewReady += 1;
      }
      if (status === "failed") {
        failed += 1;
        failedItemIds.push(reference.itemId);
      }
      if (status === "cancelled") cancelled += 1;
      if (status === "cancelling") cancelling += 1;
      if (status === "unknown") unknown += 1;
      if (task && !task.cancellable && !isTerminalStatus(task.status) && task.status !== "waiting_for_confirmation") {
        nonCancellable += 1;
      }
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
}

function recoverImportBatchRecords(
  session: ImportSession,
  taskList: readonly BackendTask[],
  projectKey: string,
  epoch: number,
): readonly ImportBatchRecord[] {
  const taskById = new Map(taskList.map((task) => [task.id, task]));
  const recovered = new Map<string, ImportBatchRecord>();
  for (const item of session.items) {
    if (!item.taskId) continue;
    const task = taskById.get(item.taskId);
    const isOperation = task ? isImportBatchOperationTask(task) : false;
    if (!isOperation && !isRecoverableImportItemStatus(item.status)) continue;
    if (task && isTerminalStatus(task.status) && !isOperation) continue;
    if (isOperation) {
      const current = recovered.get(task!.id) ?? {
        id: task!.id,
        sessionId: session.sessionId,
        projectKey,
        epoch,
        tasks: [{ taskId: task!.id, itemId: "", title: task!.title }],
        itemIds: [],
        operationTaskId: task!.id,
      };
      if (current.itemIds?.includes(item.itemId)) continue;
      recovered.set(task!.id, { ...current, itemIds: [...(current.itemIds ?? []), item.itemId] });
      continue;
    }
    const batchId = task?.batchId ?? `recovered:${session.sessionId}:${item.taskId}`;
    const current = recovered.get(batchId) ?? {
      id: batchId,
      sessionId: session.sessionId,
      projectKey,
      epoch,
      tasks: [],
    };
    if (current.tasks.some((reference) => reference.taskId === item.taskId)) continue;
    recovered.set(batchId, {
      ...current,
      tasks: [...current.tasks, {
        taskId: item.taskId,
        itemId: item.itemId,
        title: item.input.displayName,
      }],
    });
  }
  return [...recovered.values()];
}

export function useImportBatchController({
  projectId,
  rootPath,
  projectKey,
  sessionEpoch,
  session,
  taskList,
  operationCountsByBatchId,
  operationFailedItemIdsByBatchId,
  tasksHydrated,
  taskLauncher,
  isScopeCurrent,
}: ImportBatchControllerOptions): ImportBatchController {
  const { t } = useTranslation();
  const pushToast = useToastStore((state) => state.pushToast);
  const selectedTasksUpsert = useTaskStore((state) => state.upsertTasks);
  const [batchRecords, setBatchRecords] = useState<readonly ImportBatchRecord[]>([]);
  const [cancellingBatchIds, setCancellingBatchIds] = useState<ReadonlySet<string>>(new Set());
  const [dismissedBatchIds, setDismissedBatchIds] = useState<ReadonlySet<string>>(new Set());
  const localBatchCounter = useRef(0);
  const trackedTaskIds = useMemo(() => [...new Set(batchRecords.flatMap((record) => [
    ...(record.operationTaskId ? [record.operationTaskId] : []),
    ...record.tasks.map((task) => task.taskId),
  ]))], [batchRecords]);
  const trackedTasks = useTaskStore(useShallow((state) => trackedTaskIds
    .flatMap((taskId) => state.taskById[taskId] ? [state.taskById[taskId]!] : [])));
  const presentationTasks = useMemo(() => {
    const taskById = new Map(taskList.map((task) => [task.id, task]));
    for (const task of trackedTasks) taskById.set(task.id, task);
    return [...taskById.values()];
  }, [taskList, trackedTasks]);

  const batches = useMemo(
    () => buildImportBatchProgress(
      batchRecords.filter((record) => record.projectKey === projectKey && record.epoch === sessionEpoch),
      presentationTasks,
      session,
      operationCountsByBatchId,
      operationFailedItemIdsByBatchId,
    ),
    [batchRecords, operationCountsByBatchId, operationFailedItemIdsByBatchId, presentationTasks, projectKey, session, sessionEpoch],
  );
  const batch = batches[0] ?? null;
  const isCancellingBatch = batches.some((candidate) => cancellingBatchIds.has(candidate.id));
  const isBatchCancelling = useCallback(
    (batchId: string) => cancellingBatchIds.has(batchId),
    [cancellingBatchIds],
  );

  const nextLocalBatchId = useCallback(() => {
    localBatchCounter.current += 1;
    return `local:${Date.now()}:${localBatchCounter.current}`;
  }, []);

  const recordItemBatch = useCallback((
    tasks: readonly BackendTask[],
    itemIds: readonly string[],
    requestKey: string,
    epoch: number,
    sessionId: string,
  ) => {
    if (tasks.length === 0 || !isScopeCurrent(requestKey, epoch, sessionId)) return;
    const itemById = useImportStore.getState().itemById;
    const operationTask = tasks.length === 1
      && tasks[0] && isImportBatchOperationTask(tasks[0])
      ? tasks[0]
      : null;
    const batchId = operationTask?.id
      ?? tasks.find((task) => task.batchId)?.batchId
      ?? nextLocalBatchId();
    const taskRefs = tasks.map((task, index) => ({
      taskId: task.id,
      itemId: itemIds[index] ?? "",
      title: itemById[itemIds[index] ?? ""]?.input.displayName ?? task.title,
    }));
    const taskIds = new Set(taskRefs.map((task) => task.taskId));
    setBatchRecords((current) => [
      ...current.filter(
        (record) => record.id !== batchId
          && !record.tasks.some((task) => taskIds.has(task.taskId)),
      ),
      {
        id: batchId,
        sessionId,
        projectKey: requestKey,
        epoch,
        tasks: operationTask
          ? [{ taskId: operationTask.id, itemId: "", title: operationTask.title }]
          : taskRefs,
        ...(operationTask ? { itemIds: [...itemIds], operationTaskId: operationTask.id } : {}),
      },
    ]);
    setCancellingBatchIds((current) => {
      if (!current.has(batchId)) return current;
      const next = new Set(current);
      next.delete(batchId);
      return next;
    });
  }, [isScopeCurrent, nextLocalBatchId]);

  const cancelBatch = useCallback(async (requestedBatchId?: string) => {
    const target = batches.find((candidate) => candidate.id === requestedBatchId) ?? batches[0];
    if (!target || cancellingBatchIds.has(target.id)) return;
    const activeTasks = target.tasks.filter((task) =>
      task.status !== "unknown"
      && !isTerminalStatus(task.status)
      && task.status !== "waiting_for_confirmation"
      && task.cancellable,
    );
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
        selectedTasksUpsert(cancelled);
      } else {
        const results = await Promise.allSettled(
          activeTasks.map((task) => taskLauncher.cancel(task.id, { suppressToast: true })),
        );
        if (results.some((result) => result.status === "rejected" || (result.status === "fulfilled" && !result.value))) {
          throw new Error(t("importV2.workflow.batchCancelFailed"));
        }
      }
    } catch (error) {
      if (isScopeCurrent(projectKey, sessionEpoch, target.sessionId)) {
        pushToast("error", error instanceof Error ? error.message : t("importV2.workflow.batchCancelFailed"));
      }
    } finally {
      setCancellingBatchIds((current) => {
        const next = new Set(current);
        next.delete(target.id);
        return next;
      });
    }
  }, [batches, cancellingBatchIds, isScopeCurrent, projectId, projectKey, pushToast, rootPath, selectedTasksUpsert, sessionEpoch, t, taskLauncher]);

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

  useEffect(() => {
    setBatchRecords([]);
    setCancellingBatchIds(new Set());
    setDismissedBatchIds(new Set());
  }, [projectKey]);

  useEffect(() => {
    if (!session || session.projectId !== projectId) return;
    const taskById = new Map(taskList.map((task) => [task.id, task]));
    const hasReferencedTaskSnapshot = Object.keys(useImportStore.getState().itemIdsByTaskId)
      .some((taskId) => taskById.has(taskId));
    if (hasImportTauriRuntime() && !tasksHydrated && !hasReferencedTaskSnapshot) return;
    const recovered = recoverImportBatchRecords(session, taskList, projectKey, sessionEpoch);
    if (recovered.length === 0) return;
    setBatchRecords((current) => {
      const existing = new Set(current.map((record) => record.id));
      const recordedTaskIds = new Set(
        current.flatMap((record) => record.tasks.map((task) => task.taskId)),
      );
      const additions = recovered.filter(
        (record) => !existing.has(record.id)
          && !dismissedBatchIds.has(record.id)
          && !record.tasks.some((task) => recordedTaskIds.has(task.taskId)),
      );
      return additions.length > 0 ? [...current, ...additions] : current;
    });
  }, [dismissedBatchIds, projectId, projectKey, session, sessionEpoch, taskList, tasksHydrated]);

  return {
    batches,
    batch,
    isCancellingBatch,
    isBatchCancelling,
    recordItemBatch,
    cancelBatch,
    dismissBatch,
  };
}
