import { useCallback } from "react";
import { useTranslation } from "react-i18next";

import { importV2Api } from "../../services/importV2Api";
import { useImportStore } from "../../stores/importStore";
import { useTaskStore } from "../../stores/taskStore";
import { useToastStore } from "../../stores/toastStore";
import type { AgentKind } from "../../types/agent";
import type { AgentAssistanceTrigger } from "../../types/importV2Agent";
import type { LegacyInventory, MigrationConfirmation, MigrationPlan } from "../../types/importV2Migration";
import { useWikiStore } from "../wiki/wikiStore";
import type { ImportWorkflow } from "./importWorkflow";
import type { ImportTaskCoordinator } from "./useImportTaskCoordinator";
import { importWorkflowErrorMessage } from "./useImportSessionScope";

type SupportingActions = Pick<ImportWorkflow,
  | "invokeLocalAgent"
  | "acceptAgentCandidate"
  | "selectAgentCandidate"
  | "discardAgentCandidate"
  | "beginLogin"
  | "completeLogin"
  | "revokeLogin"
  | "authorizePrivateTarget"
  | "getCapabilityRequirement"
  | "getAsrEnablementPlan"
  | "installCapability"
  | "scanMigration"
  | "planMigration"
  | "applyMigration"
  | "getMigrationStatus"
  | "resumeMigration"
  | "listHistory"
  | "loadHistoryDetail"
>;

interface ImportSupportingActionsOptions {
  projectId: string;
  rootPath: string;
  projectKey: string;
  isProjectCurrent: (requestKey: string) => boolean;
  isScopeCurrent: (requestKey: string, epoch: number, expectedSessionId?: string) => boolean;
  refreshForScope: (requestKey: string, epoch: number, expectedSessionId?: string) => Promise<void>;
  trackStartedItems: ImportTaskCoordinator["trackStartedItems"];
  trackCapabilityTask: ImportTaskCoordinator["trackCapabilityTask"];
}

export function useImportSupportingActions({
  projectId,
  rootPath,
  projectKey,
  isProjectCurrent,
  isScopeCurrent,
  refreshForScope,
  trackStartedItems,
  trackCapabilityTask,
}: ImportSupportingActionsOptions): SupportingActions {
  const { t } = useTranslation();
  const pushToast = useToastStore((state) => state.pushToast);
  const selectedTaskUpsert = useTaskStore((state) => state.upsertTask);
  const openTaskDrawer = useTaskStore((state) => state.openDrawer);
  const showError = useCallback((error: unknown) => {
    pushToast("error", t("importV2.workflow.error", { message: importWorkflowErrorMessage(error) }));
  }, [pushToast, t]);

  const invokeLocalAgent = useCallback(async (
    itemId: string,
    trigger: AgentAssistanceTrigger,
    agentKind: AgentKind,
  ) => {
    const current = useImportStore.getState();
    if (!current.session || current.projectKey !== projectKey
      || !current.knownItemIds.has(itemId)) return null;
    const epoch = current.sessionEpoch;
    try {
      const task = await importV2Api.startAgentAssistance({
        projectId,
        projectRootPath: rootPath,
        sessionId: current.session.sessionId,
        itemId,
        trigger,
        agentKind,
      });
      selectedTaskUpsert(task);
      if (!isScopeCurrent(projectKey, epoch)) return null;
      openTaskDrawer(task.id);
      return task;
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch)) showError(error);
      throw error;
    }
  }, [isScopeCurrent, openTaskDrawer, projectId, projectKey, rootPath, selectedTaskUpsert, showError]);

  const acceptAgentCandidate = useCallback(async (itemId: string, taskId: string) => {
    const current = useImportStore.getState();
    if (!current.session || current.projectKey !== projectKey
      || !current.knownItemIds.has(itemId)) return null;
    const epoch = current.sessionEpoch;
    try {
      const view = await importV2Api.acceptAgentCandidate({
        projectId,
        projectRootPath: rootPath,
        sessionId: current.session.sessionId,
        itemId,
        taskId,
      });
      return isScopeCurrent(projectKey, epoch) ? view : null;
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch)) showError(error);
      throw error;
    }
  }, [isScopeCurrent, projectId, projectKey, rootPath, showError]);

  const selectAgentCandidate = useCallback(async (request: {
    itemId: string;
    candidateId: string;
    mergedMarkdown: string | null;
    expectedCurrentWikiSha256: string | null;
  }) => {
    const current = useImportStore.getState();
    if (!current.session || current.projectKey !== projectKey
      || !current.knownItemIds.has(request.itemId)) return null;
    const epoch = current.sessionEpoch;
    try {
      const result = await importV2Api.selectAgentCandidate({
        projectId,
        projectRootPath: rootPath,
        sessionId: current.session.sessionId,
        ...request,
      });
      if (isScopeCurrent(projectKey, epoch)) {
        useImportStore.getState().replaceItem(projectKey, result.item, epoch);
        if (result.completion) {
          useImportStore.getState().setCompletion(projectKey, result.completion, epoch);
          void useWikiStore.getState().scan(projectId, rootPath);
        }
        return result;
      }
      return null;
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch)) showError(error);
      throw error;
    }
  }, [isScopeCurrent, projectId, projectKey, rootPath, showError]);

  const discardAgentCandidate = useCallback(async (itemId: string, candidateId: string) => {
    const current = useImportStore.getState();
    if (!current.session || current.projectKey !== projectKey
      || !current.knownItemIds.has(itemId)) return null;
    const epoch = current.sessionEpoch;
    try {
      const result = await importV2Api.discardAgentCandidate({
        projectId,
        projectRootPath: rootPath,
        sessionId: current.session.sessionId,
        itemId,
        candidateId,
      });
      if (isScopeCurrent(projectKey, epoch)) {
        useImportStore.getState().replaceItem(projectKey, result.item, epoch);
        return result;
      }
      return null;
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch)) showError(error);
      throw error;
    }
  }, [isScopeCurrent, projectId, projectKey, rootPath, showError]);

  const beginLogin = useCallback(async (itemId: string, platform: string) => {
    const current = useImportStore.getState();
    if (!current.session || current.projectKey !== projectKey
      || !current.knownItemIds.has(itemId)) return null;
    const epoch = current.sessionEpoch;
    try {
      const result = await importV2Api.beginLogin({
        projectId,
        projectRootPath: rootPath,
        sessionId: current.session.sessionId,
        itemId,
        platform,
      });
      return isScopeCurrent(projectKey, epoch) ? result : null;
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch)) showError(error);
      throw error;
    }
  }, [isScopeCurrent, projectId, projectKey, rootPath, showError]);

  const completeLogin = useCallback(async (itemId: string, connectorSessionId: string) => {
    const current = useImportStore.getState();
    if (!current.session || current.projectKey !== projectKey
      || !current.knownItemIds.has(itemId)) return null;
    const epoch = current.sessionEpoch;
    try {
      const result = await importV2Api.completeLogin({
        projectId,
        projectRootPath: rootPath,
        importSessionId: current.session.sessionId,
        itemId,
        connectorSessionId,
      });
      if (!isScopeCurrent(projectKey, epoch)) return null;
      trackStartedItems(
        result.tasks,
        result.resumedItemIds,
        projectKey,
        epoch,
        current.session.sessionId,
      );
      await refreshForScope(projectKey, epoch);
      return result.connectorSession;
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch)) showError(error);
      throw error;
    }
  }, [isScopeCurrent, projectId, projectKey, refreshForScope, rootPath, showError, trackStartedItems]);

  const revokeLogin = useCallback(async (connectorSessionId: string, platform: string | null = null) => {
    if (!isProjectCurrent(projectKey)) return false;
    try {
      await importV2Api.revokeLogin({ sessionId: connectorSessionId, platform });
      return isProjectCurrent(projectKey);
    } catch (error) {
      if (isProjectCurrent(projectKey)) showError(error);
      throw error;
    }
  }, [isProjectCurrent, projectKey, showError]);

  const authorizePrivateTarget = useCallback(async (itemId: string, url: string) => {
    const current = useImportStore.getState();
    if (!current.session || current.projectKey !== projectKey
      || !current.knownItemIds.has(itemId)) return null;
    const epoch = current.sessionEpoch;
    try {
      const grant = await importV2Api.authorizePrivateTarget({
        projectId,
        projectRootPath: rootPath,
        sessionId: current.session.sessionId,
        itemId,
        url,
      });
      return isScopeCurrent(projectKey, epoch) ? grant : null;
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch)) showError(error);
      throw error;
    }
  }, [isScopeCurrent, projectId, projectKey, rootPath, showError]);

  const getCapabilityRequirement = useCallback(async (itemId: string) => {
    const current = useImportStore.getState();
    if (!current.session || current.projectKey !== projectKey
      || !current.knownItemIds.has(itemId)) return null;
    const epoch = current.sessionEpoch;
    try {
      const result = await importV2Api.getCapabilityRequirement({
        projectId,
        projectRootPath: rootPath,
        sessionId: current.session.sessionId,
        itemId,
      });
      return isScopeCurrent(projectKey, epoch) ? result : null;
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch)) showError(error);
      throw error;
    }
  }, [isScopeCurrent, projectId, projectKey, rootPath, showError]);

  const getAsrEnablementPlan = useCallback(async (itemId: string) => {
    const current = useImportStore.getState();
    if (!current.session || current.projectKey !== projectKey
      || !current.knownItemIds.has(itemId)) return null;
    const epoch = current.sessionEpoch;
    try {
      const result = await importV2Api.getAsrEnablementPlan({
        projectId,
        projectRootPath: rootPath,
        sessionId: current.session.sessionId,
        itemId,
      });
      return isScopeCurrent(projectKey, epoch) ? result : null;
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch)) showError(error);
      throw error;
    }
  }, [isScopeCurrent, projectId, projectKey, rootPath, showError]);

  const installCapability = useCallback(async (
    itemId: string,
    capabilityId: string,
    requirementRevision: string,
    asrOptions?: import("./importWorkflow").AsrAuthorizationOptions,
  ) => {
    const current = useImportStore.getState();
    if (!current.session || current.projectKey !== projectKey
      || !current.knownItemIds.has(itemId)) return null;
    const epoch = current.sessionEpoch;
    try {
      const task = await importV2Api.installCapability({
        projectId,
        projectRootPath: rootPath,
        sessionId: current.session.sessionId,
        itemId,
        capabilityId,
        requirementRevision,
        acknowledgeInstall: true,
        asrProfile: asrOptions?.profile,
        recognitionLanguage: asrOptions?.language,
      });
      trackCapabilityTask(task, { projectKey, epoch });
      return isScopeCurrent(projectKey, epoch) ? task : null;
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch)) showError(error);
      throw error;
    }
  }, [isScopeCurrent, projectId, projectKey, rootPath, showError, trackCapabilityTask]);

  const scanMigration = useCallback(async () => {
    if (!isProjectCurrent(projectKey)) return null;
    try {
      const inventory = await importV2Api.scanMigration({ projectId, projectRootPath: rootPath });
      return isProjectCurrent(projectKey) ? inventory : null;
    } catch (error) {
      if (isProjectCurrent(projectKey)) showError(error);
      throw error;
    }
  }, [isProjectCurrent, projectId, projectKey, rootPath, showError]);

  const planMigration = useCallback(async (inventory: LegacyInventory) => {
    if (!isProjectCurrent(projectKey)) return null;
    try {
      const plan = await importV2Api.planMigration({ projectId, projectRootPath: rootPath, inventory });
      return isProjectCurrent(projectKey) ? plan : null;
    } catch (error) {
      if (isProjectCurrent(projectKey)) showError(error);
      throw error;
    }
  }, [isProjectCurrent, projectId, projectKey, rootPath, showError]);

  const applyMigration = useCallback(async (
    plan: MigrationPlan,
    confirmation: MigrationConfirmation,
  ) => {
    if (!isProjectCurrent(projectKey)) return null;
    try {
      const task = await importV2Api.applyMigration({
        projectId,
        projectRootPath: rootPath,
        plan,
        confirmation,
      });
      selectedTaskUpsert(task);
      if (!isProjectCurrent(projectKey)) return null;
      openTaskDrawer(task.id);
      return task;
    } catch (error) {
      if (isProjectCurrent(projectKey)) showError(error);
      throw error;
    }
  }, [isProjectCurrent, openTaskDrawer, projectId, projectKey, rootPath, selectedTaskUpsert, showError]);

  const getMigrationStatus = useCallback(async () => {
    if (!isProjectCurrent(projectKey)) return null;
    try {
      const status = await importV2Api.getMigrationStatus({ projectId, projectRootPath: rootPath });
      return isProjectCurrent(projectKey) ? status : null;
    } catch (error) {
      if (isProjectCurrent(projectKey)) showError(error);
      throw error;
    }
  }, [isProjectCurrent, projectId, projectKey, rootPath, showError]);

  const resumeMigration = useCallback(async (
    plan: MigrationPlan,
    confirmation: MigrationConfirmation,
  ) => {
    if (!isProjectCurrent(projectKey)) return null;
    try {
      const task = await importV2Api.resumeMigration({
        projectId,
        projectRootPath: rootPath,
        plan,
        confirmation,
      });
      selectedTaskUpsert(task);
      if (!isProjectCurrent(projectKey)) return null;
      openTaskDrawer(task.id);
      return task;
    } catch (error) {
      if (isProjectCurrent(projectKey)) showError(error);
      throw error;
    }
  }, [isProjectCurrent, openTaskDrawer, projectId, projectKey, rootPath, selectedTaskUpsert, showError]);

  const listHistory = useCallback(async (cursor: string | null = null) => {
    if (!isProjectCurrent(projectKey)) return null;
    try {
      const page = await importV2Api.listHistory({
        projectId,
        projectRootPath: rootPath,
        cursor,
        limit: 50,
      });
      if (cursor === null && page.warnings.some((warning) => warning.code === "IMPORT_V2_HISTORY_INDEX_REBUILD_REQUIRED")) {
        try {
          const task = await importV2Api.rebuildHistoryIndex({ projectId, projectRootPath: rootPath });
          if (!isProjectCurrent(projectKey)) return null;
          selectedTaskUpsert(task);
        } catch {
          // Restricted/read-only projects retain the bounded compatibility
          // page and rebuild warning instead of turning a readable history
          // response into a fatal error.
        }
      }
      return isProjectCurrent(projectKey) ? page : null;
    } catch (error) {
      if (isProjectCurrent(projectKey)) showError(error);
      throw error;
    }
  }, [isProjectCurrent, projectId, projectKey, rootPath, selectedTaskUpsert, showError]);

  const loadHistoryDetail = useCallback(async (batchId: string, cursor: string | null = null) => {
    if (!isProjectCurrent(projectKey)) return null;
    try {
      const page = await importV2Api.getHistoryDetail({
        projectId,
        projectRootPath: rootPath,
        batchId,
        cursor,
        limit: 50,
      });
      return isProjectCurrent(projectKey) ? page : null;
    } catch (error) {
      if (isProjectCurrent(projectKey)) showError(error);
      throw error;
    }
  }, [isProjectCurrent, projectId, projectKey, rootPath, showError]);

  return {
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
    loadHistoryDetail,
  };
}
