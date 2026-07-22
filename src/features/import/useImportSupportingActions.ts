import { useCallback } from "react";
import { useTranslation } from "react-i18next";

import { importV2Api } from "../../services/importV2Api";
import { useImportStore } from "../../stores/importStore";
import { useTaskStore } from "../../stores/taskStore";
import { useToastStore } from "../../stores/toastStore";
import type { AgentKind } from "../../types/agent";
import type { LlmProviderKind } from "../../types/llm";
import type { AgentAssistancePolicy, AgentAssistanceTrigger } from "../../types/importV2Agent";
import type { LegacyInventory, MigrationConfirmation, MigrationPlan } from "../../types/importV2Migration";
import type { ImportWorkflow } from "./importWorkflow";
import type { ImportTaskCoordinator } from "./useImportTaskCoordinator";
import { importWorkflowErrorMessage } from "./useImportSessionScope";

type SupportingActions = Pick<ImportWorkflow,
  | "getAgentPolicy"
  | "setAgentPolicy"
  | "invokeLocalAgent"
  | "previewByokScope"
  | "approveByokAssistance"
  | "acceptAgentCandidate"
  | "selectAgentCandidate"
  | "discardAgentCandidate"
  | "beginLogin"
  | "completeLogin"
  | "revokeLogin"
  | "authorizePrivateTarget"
  | "getCapabilityRequirement"
  | "installCapability"
  | "scanMigration"
  | "planMigration"
  | "applyMigration"
  | "getMigrationStatus"
  | "resumeMigration"
  | "listHistory"
>;

interface ImportSupportingActionsOptions {
  projectId: string;
  rootPath: string;
  projectKey: string;
  isProjectCurrent: (requestKey: string) => boolean;
  isScopeCurrent: (requestKey: string, epoch: number, expectedSessionId?: string) => boolean;
  refreshForScope: (requestKey: string, epoch: number, expectedSessionId?: string) => Promise<void>;
  startItems: ImportWorkflow["startItems"];
  trackCapabilityTask: ImportTaskCoordinator["trackCapabilityTask"];
}

export function useImportSupportingActions({
  projectId,
  rootPath,
  projectKey,
  isProjectCurrent,
  isScopeCurrent,
  refreshForScope,
  startItems,
  trackCapabilityTask,
}: ImportSupportingActionsOptions): SupportingActions {
  const { t } = useTranslation();
  const pushToast = useToastStore((state) => state.pushToast);
  const selectedTaskUpsert = useTaskStore((state) => state.upsertTask);
  const openTaskDrawer = useTaskStore((state) => state.openDrawer);
  const showError = useCallback((error: unknown) => {
    pushToast("error", t("importV2.workflow.error", { message: importWorkflowErrorMessage(error) }));
  }, [pushToast, t]);

  const getAgentPolicy = useCallback(async () => {
    try {
      const result = await importV2Api.getAgentPolicy({ projectId, projectRootPath: rootPath });
      return isProjectCurrent(projectKey) ? result : null;
    } catch (error) {
      if (isProjectCurrent(projectKey)) showError(error);
      return null;
    }
  }, [isProjectCurrent, projectId, projectKey, rootPath, showError]);

  const setAgentPolicy = useCallback(async (
    policy: AgentAssistancePolicy,
    localAgentKind: AgentKind | null,
  ) => {
    try {
      const result = await importV2Api.setAgentPolicy({
        projectId,
        projectRootPath: rootPath,
        policy,
        localAgentKind,
      });
      return isProjectCurrent(projectKey) ? result : null;
    } catch (error) {
      if (isProjectCurrent(projectKey)) showError(error);
      throw error;
    }
  }, [isProjectCurrent, projectId, projectKey, rootPath, showError]);

  const invokeLocalAgent = useCallback(async (
    itemId: string,
    trigger: AgentAssistanceTrigger,
    agentKind: AgentKind,
  ) => {
    const current = useImportStore.getState();
    if (!current.session || current.projectKey !== projectKey
      || !current.session.items.some((item) => item.itemId === itemId)) return null;
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

  const previewByokScope = useCallback(async (
    itemId: string,
    trigger: AgentAssistanceTrigger,
    provider: LlmProviderKind,
  ) => {
    const current = useImportStore.getState();
    if (!current.session || current.projectKey !== projectKey
      || !current.session.items.some((item) => item.itemId === itemId)) return null;
    const epoch = current.sessionEpoch;
    try {
      const scope = await importV2Api.previewByokScope({
        projectId,
        projectRootPath: rootPath,
        sessionId: current.session.sessionId,
        itemId,
        trigger,
        provider,
      });
      return isScopeCurrent(projectKey, epoch) ? scope : null;
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch)) showError(error);
      throw error;
    }
  }, [isScopeCurrent, projectId, projectKey, rootPath, showError]);

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
    if (!current.session || current.projectKey !== projectKey
      || !current.session.items.some((item) => item.itemId === request.itemId)) return null;
    const epoch = current.sessionEpoch;
    try {
      const task = await importV2Api.approveByokAssistance({
        projectId,
        projectRootPath: rootPath,
        sessionId: current.session.sessionId,
        ...request,
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
      || !current.session.items.some((item) => item.itemId === itemId)) return null;
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
      || !current.session.items.some((item) => item.itemId === request.itemId)) return null;
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
      || !current.session.items.some((item) => item.itemId === itemId)) return null;
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
      || !current.session.items.some((item) => item.itemId === itemId)) return null;
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
      || !current.session.items.some((item) => item.itemId === itemId)) return null;
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
      await refreshForScope(projectKey, epoch);
      await startItems([itemId]);
      return result;
    } catch (error) {
      if (isScopeCurrent(projectKey, epoch)) showError(error);
      throw error;
    }
  }, [isScopeCurrent, projectId, projectKey, refreshForScope, rootPath, showError, startItems]);

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
      || !current.session.items.some((item) => item.itemId === itemId)) return null;
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
      || !current.session.items.some((item) => item.itemId === itemId)) return null;
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

  const installCapability = useCallback(async (itemId: string, capabilityId: string) => {
    const current = useImportStore.getState();
    if (!current.session || current.projectKey !== projectKey
      || !current.session.items.some((item) => item.itemId === itemId)) return null;
    const epoch = current.sessionEpoch;
    try {
      const task = await importV2Api.installCapability({
        projectId,
        projectRootPath: rootPath,
        sessionId: current.session.sessionId,
        itemId,
        capabilityId,
        acknowledgeInstall: true,
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
      return isProjectCurrent(projectKey) ? page : null;
    } catch (error) {
      if (isProjectCurrent(projectKey)) showError(error);
      throw error;
    }
  }, [isProjectCurrent, projectId, projectKey, rootPath, showError]);

  return {
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
