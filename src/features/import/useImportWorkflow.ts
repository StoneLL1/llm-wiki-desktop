import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import type { TaskLauncher } from "../../hooks/useTaskLauncher";
import { importV2Api } from "../../services/importV2Api";
import { useImportStore } from "../../stores/importStore";
import {
  selectProjectTaskById,
  selectTaskIdsForProject,
  selectTasksForProject,
  useTaskStore,
} from "../../stores/taskStore";
import { useToastStore } from "../../stores/toastStore";
import type { CommitItemDecision, ImportItemResolution, ImportRecoveryAction, ImportSession, MediaSaveMode } from "../../types/importV2";
import type { ProjectSummary } from "../../types/project";
import { selectImportViewModel } from "./importViewModel";
import type { AsrAuthorizationOptions, ImportWorkflow } from "./importWorkflow";
import type { AppView } from "../../stores/navigationStore";
import { importWorkflowErrorMessage as errorMessage, useImportSessionScope } from "./useImportSessionScope";
import { useImportBatchController } from "./useImportBatchController";
import { useImportTaskCoordinator } from "./useImportTaskCoordinator";
import { useImportSupportingActions } from "./useImportSupportingActions";
import { useNavigationStore } from "../../stores/navigationStore";
import { useWikiStore } from "../wiki/wikiStore";
import type { ImportCompletion } from "../../types/importV2";
import type { ImportCollectionPreview, RemoteMediaRetentionPlan } from "../../types/importV2Web";
import type { ImportWorkbenchPreferences } from "../../types/importV2Presentation";

export type { ImportBatchProgress, ImportWorkflow } from "./importWorkflow";

const selectImportTasks = (state: Parameters<typeof selectTasksForProject>[0], projectId: string) =>
  selectTasksForProject(state, projectId).filter((task) => task.taskType === "import");

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
  const importActive = activeView === "import";
  const inactiveSnapshotRef = useRef({
    session: useImportStore.getState().session,
    completion: useImportStore.getState().completion,
    selectedItemId: useImportStore.getState().selectedItemId,
    filter: useImportStore.getState().filter,
    isConfirming: useImportStore.getState().isConfirming,
    mutationKeys: useImportStore.getState().mutationKeys,
    sessionEpoch: useImportStore.getState().sessionEpoch,
    taskList: selectImportTasks(useTaskStore.getState(), project.projectId),
    taskIds: selectTaskIdsForProject(useTaskStore.getState(), project.projectId),
    tasksHydrated: useTaskStore.getState().tasksHydrated,
  });
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
  const session = useImportStore((state) => importActive ? state.session : inactiveSnapshotRef.current.session);
  const completion = useImportStore((state) => importActive ? state.completion : inactiveSnapshotRef.current.completion);
  const selectedItemId = useImportStore((state) => importActive ? state.selectedItemId : inactiveSnapshotRef.current.selectedItemId);
  const filter = useImportStore((state) => importActive ? state.filter : inactiveSnapshotRef.current.filter);
  const isConfirming = useImportStore((state) => importActive ? state.isConfirming : inactiveSnapshotRef.current.isConfirming);
  const taskIds = useTaskStore((state) => importActive
    ? selectTaskIdsForProject(state, project.projectId)
    : inactiveSnapshotRef.current.taskIds);
  // Import progress is reconciled by the event coordinator. Keep this
  // workflow-level task snapshot stable for progress-only updates so its
  // session/batch effects run only on task membership or lifecycle changes.
  const taskList = useMemo(() => importActive
    ? taskIds
        .map((taskId) => selectProjectTaskById(useTaskStore.getState(), project.projectId, taskId))
        .filter((task): task is NonNullable<typeof task> => task?.taskType === "import")
    : inactiveSnapshotRef.current.taskList,
  [importActive, project.projectId, taskIds]);
  const tasksHydrated = useTaskStore((state) => importActive ? state.tasksHydrated : inactiveSnapshotRef.current.tasksHydrated);
  const pushToast = useToastStore((state) => state.pushToast);
  const mutationKeys = useImportStore((state) => importActive ? state.mutationKeys : inactiveSnapshotRef.current.mutationKeys);
  const sessionEpoch = useImportStore((state) => importActive ? state.sessionEpoch : inactiveSnapshotRef.current.sessionEpoch);
  if (importActive) {
    inactiveSnapshotRef.current = {
      session,
      completion,
      selectedItemId,
      filter,
      isConfirming,
      mutationKeys,
      sessionEpoch,
      taskList,
      taskIds,
      tasksHydrated,
    };
  }
  const isAddingPaths = [...mutationKeys].some((key) => key.startsWith(`add-paths:${projectKey}:`));
  const isAddingText = [...mutationKeys].some((key) => key.startsWith(`add-text:${projectKey}:`));
  const isAddingUrl = [...mutationKeys].some((key) => key.startsWith(`add-url:${projectKey}:`));
  const [pendingCollection, setPendingCollection] = useState<{
    preview: ImportCollectionPreview;
    mediaSaveMode: MediaSaveMode;
  } | null>(null);
  const sessionRenewalRef = useRef<{
    projectKey: string;
    promise: Promise<boolean>;
  } | null>(null);
  const sourceAdditionTailsRef = useRef(new Map<string, Promise<void>>());
  const sourceAdditionRevisionRef = useRef(0);
  const [remoteMediaRetentionPlan, setRemoteMediaRetentionPlan] = useState<RemoteMediaRetentionPlan | null>(null);
  const [restrictedCommitDecisions, setRestrictedCommitDecisions] = useState<CommitItemDecision[] | null>(null);
  useEffect(() => {
    setPendingCollection(null);
    setRemoteMediaRetentionPlan(null);
    setRestrictedCommitDecisions(null);
  }, [projectKey]);
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
    acceptDiscovery,
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

  const enqueueSourceAddition = useCallback((
    requestProjectKey: string,
    operation: () => Promise<void>,
  ) => {
    const previous = sourceAdditionTailsRef.current.get(requestProjectKey) ?? Promise.resolve();
    const queued = previous.then(operation, operation);
    const tail = queued.then(() => undefined, () => undefined);
    sourceAdditionTailsRef.current.set(requestProjectKey, tail);
    void tail.then(() => {
      if (sourceAdditionTailsRef.current.get(requestProjectKey) === tail) {
        sourceAdditionTailsRef.current.delete(requestProjectKey);
      }
    });
    return queued;
  }, []);

  const ensureActiveSession = useCallback(async () => {
    const current = useImportStore.getState();
    if (!current.session || current.projectKey !== projectKey) return false;
    const ended = current.session.status === "completed"
      || current.session.status === "cancelled"
      || (
        current.session.items.length > 0
        && current.session.items.every((item) =>
          item.status === "completed"
          || item.status === "skipped"
          || item.status === "cancelled")
      );
    if (!ended) return true;

    const inFlight = sessionRenewalRef.current;
    if (inFlight?.projectKey === projectKey) return inFlight.promise;

    const epoch = current.sessionEpoch;
    const endedSessionId = current.session.sessionId;
    const resourceMode = current.session.resourceMode;
    const promise = (async () => {
      const nextSession = await importV2Api.createSession({
        projectId,
        projectRootPath: rootPath,
        resourceMode,
      });
      const latest = useImportStore.getState();
      if (latest.projectKey !== projectKey) return false;
      if (latest.session?.sessionId === nextSession.sessionId) return true;
      if (
        latest.sessionEpoch !== epoch
        || latest.session?.sessionId !== endedSessionId
        || !isScopeCurrent(projectKey, epoch, endedSessionId)
      ) return false;
      const nextEpoch = latest.beginSessionEpoch(projectKey);
      return useImportStore.getState().attachSession(projectKey, nextSession, nextEpoch);
    })();
    sessionRenewalRef.current = { projectKey, promise };
    try {
      return await promise;
    } finally {
      if (sessionRenewalRef.current?.promise === promise) {
        sessionRenewalRef.current = null;
      }
    }
  }, [isScopeCurrent, projectId, projectKey, rootPath]);

  const addPaths = useCallback((paths: string[], largeDataConfirmed = false) => enqueueSourceAddition(projectKey, async () => {
    const sourcePaths = paths.map((path) => path.trim()).filter(Boolean);
    if (sourcePaths.length === 0 || !(await ensureActiveSession())) return;
    const current = useImportStore.getState();
    if (!current.session || current.projectKey !== projectKey) return;
    const epoch = current.sessionEpoch;
    const existingItemIds = new Set(current.session.items.map((item) => item.itemId));
    const mutationKey = `add-paths:${projectKey}:${epoch}:${++sourceAdditionRevisionRef.current}`;
    nextSessionMutationRevision();
    current.beginMutation(mutationKey);
    let taskStarted = false;
    try {
      const task = await importV2Api.addPaths({
        projectId,
        projectRootPath: rootPath,
        sessionId: current.session!.sessionId,
        sourcePaths,
        ...(largeDataConfirmed ? { largeDataConfirmed: true } : {}),
      });
      taskStarted = true;
      await trackPathTask(task, { projectKey, epoch, existingItemIds, mutationKey });
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch)) pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
      throw error;
    } finally {
      if (!taskStarted) useImportStore.getState().endMutation(mutationKey);
    }
  }), [enqueueSourceAddition, ensureActiveSession, isScopeCurrent, nextSessionMutationRevision, projectId, projectKey, pushToast, rootPath, t, trackPathTask]);

  const addText = useCallback((content: string, sourceName: string) => enqueueSourceAddition(projectKey, async () => {
    const value = content.trim();
    if (!value || !(await ensureActiveSession())) return;
    const current = useImportStore.getState();
    if (!current.session || current.projectKey !== projectKey) return;
    const epoch = current.sessionEpoch;
    const existingItemIds = new Set(current.session.items.map((item) => item.itemId));
    const mutationKey = `add-text:${projectKey}:${epoch}`;
    if (current.mutationKeys.has(mutationKey)) return;
    const mutationRevision = nextSessionMutationRevision();
    current.beginMutation(mutationKey);
    try {
      const nextSession = await importV2Api.addText({
        projectId,
        projectRootPath: rootPath,
        sessionId: current.session.sessionId,
        sourceName: sourceName.trim() || "clipboard.md",
        content,
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
      if (isScopeCurrent(projectKey, epoch)) {
        pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
      }
      throw error;
    } finally {
      useImportStore.getState().endMutation(mutationKey);
    }
  }), [
    enqueueSourceAddition,
    ensureActiveSession,
    isScopeCurrent,
    nextSessionMutationRevision,
    projectId,
    projectKey,
    pushToast,
    reconcileMutationSession,
    rootPath,
    startNewQueuedItems,
    t,
  ]);

  const addUrl = useCallback((url: string, mediaSaveMode?: MediaSaveMode) => enqueueSourceAddition(projectKey, async () => {
    if (!url.trim() || !(await ensureActiveSession())) return;
    const current = useImportStore.getState();
    if (!current.session || current.projectKey !== projectKey) return;
    const epoch = current.sessionEpoch;
    const existingItemIds = new Set(current.session.items.map((item) => item.itemId));
    const mutationKey = `add-url:${projectKey}:${epoch}`;
    if (current.mutationKeys.has(mutationKey)) return;
    const mutationRevision = nextSessionMutationRevision();
    current.beginMutation(mutationKey);
    try {
      const collection = await importV2Api.discoverCollection({
        projectId,
        projectRootPath: rootPath,
        sessionId: current.session.sessionId,
        url: url.trim(),
      });
      if (collection) {
        if (isScopeCurrent(projectKey, epoch, current.session.sessionId)) {
          setPendingCollection({
            preview: collection,
            mediaSaveMode: mediaSaveMode ?? "extract_only",
          });
        }
        return;
      }
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
  }), [enqueueSourceAddition, ensureActiveSession, isScopeCurrent, nextSessionMutationRevision, projectId, projectKey, pushToast, reconcileMutationSession, rootPath, startNewQueuedItems, t]);

  const confirmCollection = useCallback(async (itemRefs: readonly string[]) => {
    const current = useImportStore.getState();
    const pending = pendingCollection;
    const refs = [...new Set(itemRefs)];
    if (!pending || refs.length === 0 || !current.session || current.projectKey !== projectKey) return;
    const epoch = current.sessionEpoch;
    const existingItemIds = new Set(current.session.items.map((item) => item.itemId));
    const mutationKey = `add-collection:${projectKey}:${epoch}`;
    if (current.mutationKeys.has(mutationKey)) return;
    const mutationRevision = nextSessionMutationRevision();
    current.beginMutation(mutationKey);
    try {
      const nextSession = await importV2Api.addCollectionItems({
        projectId,
        projectRootPath: rootPath,
        sessionId: current.session.sessionId,
        collectionRef: pending.preview.collectionRef,
        itemRefs: refs,
        mediaSaveMode: pending.mediaSaveMode,
      });
      if (await reconcileMutationSession(
        nextSession,
        projectKey,
        epoch,
        mutationRevision,
        current.session.sessionId,
      )) {
        setPendingCollection(null);
        await startNewQueuedItems(projectKey, epoch, existingItemIds);
      }
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch)) {
        pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
      }
      throw error;
    } finally {
      useImportStore.getState().endMutation(mutationKey);
    }
  }, [isScopeCurrent, nextSessionMutationRevision, pendingCollection, projectId, projectKey, pushToast, reconcileMutationSession, rootPath, startNewQueuedItems, t]);

  const loadCollectionPage = useCallback(async (loadAll = false) => {
    const current = useImportStore.getState();
    const pending = pendingCollection;
    if (
      !pending?.preview.nextCursor
      || !current.session
      || current.projectKey !== projectKey
    ) return;
    const epoch = current.sessionEpoch;
    const page = await importV2Api.loadCollectionPage({
      projectId,
      projectRootPath: rootPath,
      sessionId: current.session.sessionId,
      collectionRef: pending.preview.collectionRef,
      cursor: pending.preview.nextCursor,
      loadAll,
    });
    if (!isScopeCurrent(projectKey, epoch, current.session.sessionId)) return;
    setPendingCollection((value) => {
      if (!value || value.preview.collectionRef !== pending.preview.collectionRef) return value;
      const known = new Set(value.preview.items.map((item) => item.itemRef));
      return {
        ...value,
        preview: {
          ...value.preview,
          ...page,
          items: [
            ...value.preview.items,
            ...page.items.filter((item) => !known.has(item.itemRef)),
          ],
        },
      };
    });
  }, [isScopeCurrent, pendingCollection, projectId, projectKey, rootPath]);

  const dismissCollection = useCallback(() => setPendingCollection(null), []);

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
      const task = await importV2Api.startBatch({
        projectId,
        projectRootPath: rootPath,
        sessionId: current.session.sessionId,
        itemIds: acceptedIds,
        ...(recoveryAction ? { recoveryAction } : {}),
      });
      trackStartedItems([task], acceptedIds, projectKey, epoch, current.session.sessionId);
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch)) pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
      endPendingItems(acceptedIds, projectKey, epoch);
    }
  }, [beginPendingItems, endPendingItems, isScopeCurrent, nextSessionMutationRevision, projectId, projectKey, pushToast, rootPath, t, trackStartedItems]);

  const retryItem = useCallback(async (itemId: string, recoveryAction: ImportRecoveryAction | null = null) => startItems([itemId], recoveryAction), [startItems]);

  const planRemoteMediaRetention = useCallback(async (itemId: string) => {
    const current = useImportStore.getState();
    if (!current.session || current.projectKey !== projectKey) return;
    const epoch = current.sessionEpoch;
    try {
      const plan = await importV2Api.getRemoteMediaRetentionPlan({
        projectId,
        projectRootPath: rootPath,
        sessionId: current.session.sessionId,
        itemId,
      });
      if (isScopeCurrent(projectKey, epoch, current.session.sessionId)) {
        setRemoteMediaRetentionPlan(plan);
      }
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch, current.session.sessionId)) {
        pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
      }
      throw error;
    }
  }, [isScopeCurrent, projectId, projectKey, pushToast, rootPath, t]);

  const confirmRemoteMediaRetention = useCallback(async () => {
    const current = useImportStore.getState();
    const plan = remoteMediaRetentionPlan;
    if (!plan || !current.session || current.projectKey !== projectKey) return;
    const epoch = current.sessionEpoch;
    const mutationRevision = nextSessionMutationRevision();
    const nextSession = await importV2Api.confirmRemoteMediaRetention({
      projectId,
      projectRootPath: rootPath,
      sessionId: current.session.sessionId,
      itemId: plan.itemId,
      acknowledgeSizeAndDisk: true,
    });
    if (await reconcileMutationSession(
      nextSession,
      projectKey,
      epoch,
      mutationRevision,
      current.session.sessionId,
    )) {
      setRemoteMediaRetentionPlan(null);
      await startItems([plan.itemId]);
    }
  }, [nextSessionMutationRevision, projectId, projectKey, reconcileMutationSession, remoteMediaRetentionPlan, rootPath, startItems]);

  const dismissRemoteMediaRetention = useCallback(() => setRemoteMediaRetentionPlan(null), []);

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

  const authorizeLocalAsrGroup = useCallback(async (itemIds: readonly string[], options: AsrAuthorizationOptions) => {
    const current = useImportStore.getState();
    if (!current.session || current.projectKey !== projectKey) return;
    const ids = [...new Set(itemIds)].filter((itemId) =>
      current.session?.items.some((item) => item.itemId === itemId));
    if (ids.length === 0) return;
    const epoch = current.sessionEpoch;
    nextSessionMutationRevision();
    try {
      for (const itemId of ids) {
        await importV2Api.authorizeLocalAsr({
          projectId,
          projectRootPath: rootPath,
          sessionId: current.session.sessionId,
          itemId,
          profile: options.profile,
          language: options.language,
        });
      }
      if (!isScopeCurrent(projectKey, epoch, current.session.sessionId)) return;
      await refreshForScope(projectKey, epoch);
      await startItems(ids);
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch)) pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
    }
  }, [isScopeCurrent, nextSessionMutationRevision, projectId, projectKey, pushToast, refreshForScope, rootPath, startItems, t]);

  const authorizeLocalAsr = useCallback(
    async (itemId: string, options: AsrAuthorizationOptions) =>
      authorizeLocalAsrGroup([itemId], options),
    [authorizeLocalAsrGroup],
  );

  const authorizeLocalOcrGroup = useCallback(async (itemIds: readonly string[]) => {
    const current = useImportStore.getState();
    if (!current.session || current.projectKey !== projectKey) return;
    const ids = [...new Set(itemIds)].filter((itemId) =>
      current.session?.items.some((item) => item.itemId === itemId));
    if (ids.length === 0) return;
    const epoch = current.sessionEpoch;
    nextSessionMutationRevision();
    try {
      for (const itemId of ids) {
        await importV2Api.authorizeLocalOcr({
          projectId,
          projectRootPath: rootPath,
          sessionId: current.session.sessionId,
          itemId,
        });
      }
      if (!isScopeCurrent(projectKey, epoch, current.session.sessionId)) return;
      await refreshForScope(projectKey, epoch);
      await startItems(ids);
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch)) pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
    }
  }, [isScopeCurrent, nextSessionMutationRevision, projectId, projectKey, pushToast, refreshForScope, rootPath, startItems, t]);

  const authorizeLocalOcr = useCallback(
    async (itemId: string) => authorizeLocalOcrGroup([itemId]),
    [authorizeLocalOcrGroup],
  );

  const selectSubtitle = useCallback(async (itemId: string, fileName: string) => {
    const current = useImportStore.getState();
    if (!current.session || current.projectKey !== projectKey || !current.session.items.some((item) => item.itemId === itemId)) return;
    const epoch = current.sessionEpoch;
    nextSessionMutationRevision();
    try {
      await importV2Api.selectSubtitle({
        projectId,
        projectRootPath: rootPath,
        sessionId: current.session.sessionId,
        itemId,
        fileName,
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
  }, [beginPendingItems, endPendingItems, isScopeCurrent, nextSessionMutationRevision, projectId, projectKey, pushToast, reconcileMutationSession, rootPath, t]);

  const startConfirmation = useCallback(async (
    decisions: CommitItemDecision[],
    acknowledgeRestrictedContent: boolean,
  ) => {
    const current = useImportStore.getState();
    if (!current.session || current.projectKey !== projectKey || decisions.length === 0 || current.isConfirming) return;
    const epoch = current.sessionEpoch;
    current.setIsConfirming(true);
    try {
      const task = await importV2Api.confirmSession({
        projectId,
        projectRootPath: rootPath,
        sessionId: current.session.sessionId,
        decisions,
        ...(acknowledgeRestrictedContent ? { acknowledgeRestrictedContent: true } : {}),
      });
      setRestrictedCommitDecisions(null);
      trackConfirmationTask(task, { projectKey, epoch });
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch)) {
        useImportStore.getState().setIsConfirming(false);
        pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
      }
    }
  }, [isScopeCurrent, projectId, projectKey, pushToast, rootPath, t, trackConfirmationTask]);

  const confirm = useCallback(async (decisions: CommitItemDecision[]) => {
    const current = useImportStore.getState();
    if (!current.session || current.projectKey !== projectKey || decisions.length === 0 || current.isConfirming) return;
    const selectedIds = new Set(decisions.map((decision) => decision.itemId));
    const includesRestricted = current.session.items.some(
      (item) => selectedIds.has(item.itemId) && item.restrictedContent,
    );
    if (includesRestricted) {
      try {
        const status = await importV2Api.getRestrictedContentStatus({
          projectId,
          projectRootPath: rootPath,
        });
        if (status.confirmationRequired) {
          setRestrictedCommitDecisions([...decisions]);
          return;
        }
      } catch (error) {
        pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
        return;
      }
    }
    await startConfirmation(decisions, false);
  }, [projectId, projectKey, pushToast, rootPath, startConfirmation, t]);

  const confirmRestrictedContent = useCallback(async () => {
    const decisions = restrictedCommitDecisions;
    if (!decisions) return;
    await startConfirmation(decisions, true);
  }, [restrictedCommitDecisions, startConfirmation]);

  const dismissRestrictedContent = useCallback(() => setRestrictedCommitDecisions(null), []);

  const refreshSession = useCallback(() => {
    const current = useImportStore.getState();
    return current.session && current.projectKey === projectKey
      ? refreshForScope(projectKey, current.sessionEpoch, current.session.sessionId)
      : Promise.resolve();
  }, [projectKey, refreshForScope]);

  const viewImportedSources = useCallback(async (
    selectedCompletion: ImportCompletion | null = completion,
    preferredWikiPath?: string,
  ) => {
    const importedSources = [
      ...(selectedCompletion?.newSources ?? []),
      ...(selectedCompletion?.updatedSources ?? []),
    ];
    const first = importedSources.find((source) => source.wikiPath === preferredWikiPath)
      ?? importedSources[0];
    if (!first || !isProjectCurrent(projectKey)) return;
    await useWikiStore.getState().scan(projectId, rootPath);
    if (!isProjectCurrent(projectKey)) return;
    useNavigationStore.getState().setActiveView("wiki");
    await useWikiStore.getState().openPage(projectId, rootPath, first.wikiPath);
  }, [completion, isProjectCurrent, projectId, projectKey, rootPath]);

  const updateWiki = useCallback(async (
    selectedCompletion: ImportCompletion | null = completion,
  ) => {
    const changes = [
      ...(selectedCompletion?.newSources ?? []),
      ...(selectedCompletion?.updatedSources ?? []),
    ];
    if (changes.length === 0 || !isProjectCurrent(projectKey)) return;
    useNavigationStore.getState().requestWorkflowLaunch({
      projectId,
      projectRootPath: rootPath,
      kind: "update_wiki",
      origin: "import",
      scopePreset: {
        kind: "update_wiki",
        mode: "changed_sources",
        sourceVersions: changes.map(({ sourceId, versionId }) => ({ sourceId, versionId })),
      },
    });
  }, [completion, isProjectCurrent, projectId, projectKey, rootPath]);

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
  }, [isProjectCurrent, projectId, projectKey, pushToast, rootPath, t]);

  const loadSession = useCallback(async (sessionId: string, historyBatchId: string | null = null) => {
    if (!sessionId || !isProjectCurrent(projectKey)) return null;
    try {
      const result = await importV2Api.getHistorySession({ projectId, projectRootPath: rootPath, sessionId, historyBatchId });
      return isProjectCurrent(projectKey) && useImportStore.getState().projectKey === projectKey ? result : null;
    } catch (error) {
      if (isProjectCurrent(projectKey)) pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
      return null;
    }
  }, [isProjectCurrent, projectId, projectKey, pushToast, rootPath, t]);

  const loadMergeContext = useCallback(async (itemId: string) => {
    const current = useImportStore.getState();
    if (!current.session || current.projectKey !== projectKey) {
      throw new Error("Merge context is no longer current");
    }
    const context = await importV2Api.getMergeContext({
      projectId,
      projectRootPath: rootPath,
      sessionId: current.session.sessionId,
      itemId,
    });
    if (!isProjectCurrent(projectKey)) throw new Error("Merge context is no longer current");
    return context;
  }, [isProjectCurrent, projectId, projectKey, rootPath]);

  const setItemResolution = useCallback(async (
    itemId: string,
    resolution: ImportItemResolution,
  ) => {
    const current = useImportStore.getState();
    if (!current.session || current.projectKey !== projectKey) return;
    const epoch = current.sessionEpoch;
    const acceptedIds = beginPendingItems([itemId], projectKey, epoch);
    if (acceptedIds.length === 0) return;
    nextSessionMutationRevision();
    try {
      await importV2Api.setItemResolution({
        projectId,
        projectRootPath: rootPath,
        sessionId: current.session.sessionId,
        itemId,
        resolution,
      });
      if (isScopeCurrent(projectKey, epoch, current.session.sessionId)) {
        await refreshForScope(projectKey, epoch, current.session.sessionId);
      }
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch)) {
        pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
      }
      throw error;
    } finally {
      endPendingItems(acceptedIds, projectKey, epoch);
    }
  }, [beginPendingItems, endPendingItems, isScopeCurrent, nextSessionMutationRevision, projectId, projectKey, pushToast, refreshForScope, rootPath, t]);

  const stageManualMerge = useCallback(async (itemId: string, mergedMarkdown: string) => {
    const current = useImportStore.getState();
    if (!current.session || current.projectKey !== projectKey) return;
    const epoch = current.sessionEpoch;
    const acceptedIds = beginPendingItems([itemId], projectKey, epoch);
    if (acceptedIds.length === 0) return;
    nextSessionMutationRevision();
    try {
      await importV2Api.stageManualMerge({
        projectId,
        projectRootPath: rootPath,
        sessionId: current.session.sessionId,
        itemId,
        mergedMarkdown,
      });
      if (isScopeCurrent(projectKey, epoch, current.session.sessionId)) {
        await refreshForScope(projectKey, epoch, current.session.sessionId);
      }
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch)) {
        pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
      }
      throw error;
    } finally {
      endPendingItems(acceptedIds, projectKey, epoch);
    }
  }, [beginPendingItems, endPendingItems, isScopeCurrent, nextSessionMutationRevision, projectId, projectKey, pushToast, refreshForScope, rootPath, t]);

  const loadCompletion = useCallback(async (sessionId: string, historyBatchId: string) => {
    if (!sessionId || !historyBatchId || !isProjectCurrent(projectKey)) return null;
    try {
      const result = await importV2Api.getCompletion({
        projectId,
        projectRootPath: rootPath,
        sessionId,
        historyBatchId,
      });
      return isProjectCurrent(projectKey) ? result : null;
    } catch (error) {
      if (isProjectCurrent(projectKey)) {
        pushToast("error", t("importV2.workflow.error", { message: errorMessage(error) }));
      }
      return null;
    }
  }, [isProjectCurrent, projectId, projectKey, pushToast, rootPath, t]);

  const loadWorkbenchPreferences = useCallback(
    async () => importV2Api.getWorkbenchPreferences({
      projectId,
      projectRootPath: rootPath,
    }),
    [projectId, rootPath],
  );

  const saveWorkbenchPreferences = useCallback(
    async (preferences: ImportWorkbenchPreferences) =>
      importV2Api.saveWorkbenchPreferences({
        projectId,
        projectRootPath: rootPath,
        preferences,
      }),
    [projectId, rootPath],
  );

  const { visibleItems, counts, progress } = useMemo(
    () => selectImportViewModel(session, filter),
    [filter, session],
  );
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
    invokeLocalAgent,
    acceptAgentCandidate,
    selectAgentCandidate,
    discardAgentCandidate,
    beginLogin,
    completeLogin,
    revokeLogin,
    authorizePrivateTarget,
    getCapabilityRequirement,
    getAsrEnablementPlan,
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
    trackStartedItems,
    trackCapabilityTask,
  });

  return {
    projectKey,
    session,
    completion,
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
    isAddingText,
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
    addText,
    addUrl,
    collectionPreview: pendingCollection?.preview ?? null,
    loadCollectionPage,
    confirmCollection,
    dismissCollection,
    remoteMediaRetentionPlan,
    planRemoteMediaRetention,
    confirmRemoteMediaRetention,
    dismissRemoteMediaRetention,
    cancelDiscovery,
    confirmDiscovery: acceptDiscovery,
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
    authorizeLocalAsrGroup,
    authorizeLocalOcr,
    authorizeLocalOcrGroup,
    selectSubtitle,
    confirm,
    restrictedCommitPending: restrictedCommitDecisions !== null,
    confirmRestrictedContent,
    dismissRestrictedContent,
    viewImportedSources,
    updateWiki,
    isConfirming,
    refreshSession,
    selectItem,
    setFilter,
    requestClipboard,
    loadPreview,
    loadMergeContext,
    setItemResolution,
    stageManualMerge,
    loadSession,
    loadCompletion,
    invokeLocalAgent,
    acceptAgentCandidate,
    selectAgentCandidate,
    discardAgentCandidate,
    beginLogin,
    completeLogin,
    revokeLogin,
    authorizePrivateTarget,
    getCapabilityRequirement,
    getAsrEnablementPlan,
    installCapability,
    scanMigration,
    planMigration,
    applyMigration,
    getMigrationStatus,
    resumeMigration,
    listHistory,
    loadWorkbenchPreferences,
    saveWorkbenchPreferences,
  };
}
