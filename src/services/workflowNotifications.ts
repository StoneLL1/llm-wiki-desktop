import { sendNotification, type Options } from "@tauri-apps/plugin-notification";
import i18next from "i18next";
import { hydrateAndSelectWorkflowRun, openWorkflowResult } from "./workflowNavigation";
import { useNavigationStore } from "../stores/navigationStore";
import { useSettingsStore } from "../stores/settingsStore";
import { useTaskStore } from "../stores/taskStore";
import { useProjectStore } from "../stores/projectStore";
import type { BackendEvent } from "../types/task";
import type { WorkflowDisplayStatus, WorkflowRun, WorkflowRunSummary } from "../types/workflow";

const ALLOWED_WORKFLOW_STATUSES = new Set<WorkflowDisplayStatus>(["waiting_for_confirmation", "completed", "failed"]);

function safeWorkflowSummary(run: WorkflowRun | WorkflowRunSummary): string | null {
  if (!("scope" in run)) {
    const outcome = run.outcome;
    if (outcome?.kind === "health_check") {
      if (![outcome.errorCount, outcome.warningCount].every(Number.isFinite)) return null;
      return i18next.t("notification.workflow.healthSummary", { errors: outcome.errorCount, warnings: outcome.warningCount });
    }
    if (outcome?.kind === "generate_content") {
      if (!Number.isFinite(outcome.artifactCount)) return null;
      return i18next.t("notification.workflow.generateSummary", { count: outcome.artifactCount });
    }
    if (outcome?.kind === "update_wiki") {
      if (![outcome.created, outcome.updated, outcome.skipped].every(Number.isFinite)) return null;
      return i18next.t("notification.workflow.updateCounts", { created: outcome.created, updated: outcome.updated, skipped: outcome.skipped });
    }
    return i18next.t(`workflows.status.${run.displayStatus}`);
  }
  if (!run.result) return i18next.t(`workflows.status.${run.displayStatus}`);
  if (run.result.kind === "update_wiki") {
    if (![run.result.created, run.result.updated, run.result.deleted, run.result.conflicted].every(Number.isFinite)) return null;
    return i18next.t("notification.workflow.updateSummary", {
      changed: run.result.created + run.result.updated,
      deleted: run.result.deleted,
      conflicted: run.result.conflicted,
    });
  }
  if (run.result.kind === "health_check") {
    if (![run.result.errorCount, run.result.warningCount].every(Number.isFinite)) return null;
    return i18next.t("notification.workflow.healthSummary", {
      errors: run.result.errorCount,
      warnings: run.result.warningCount,
    });
  }
  if (run.result.kind !== "generate_content" || !Array.isArray(run.result.outputPaths)) return null;
  return i18next.t("notification.workflow.generateSummary", {
    count: run.result.outputPaths.length,
  });
}

export function workflowNotificationOptions(event: BackendEvent, run: WorkflowRun | WorkflowRunSummary): Options | null {
  if (
    !run
    || typeof run !== "object"
    || typeof run.taskId !== "string"
    || !run.taskId
    || typeof run.projectId !== "string"
    || !["update_wiki", "health_check", "generate_content"].includes(run.kind)
    || !ALLOWED_WORKFLOW_STATUSES.has(run.displayStatus)
    || (!("result" in run) && !("outcome" in run))
    || ("result" in run && (run.result !== null && typeof run.result !== "object"))
    || ("result" in run && run.result != null && run.result.kind !== run.kind)
    || ("outcome" in run && run.outcome != null && run.outcome.kind !== run.kind)
  ) return null;
  const body = safeWorkflowSummary(run);
  if (!body) return null;
  const projectName =
    useProjectStore.getState().currentProject.projectId === run.projectId
      ? useProjectStore.getState().currentProject.name
      : useProjectStore.getState().recentProjects.find((project) => project.projectId === run.projectId)?.name
        ?? i18next.t("notification.workflow.unknownProject");
  return {
    title: i18next.t("notification.workflow.title", {
      project: projectName,
      workflow: i18next.t(`workflows.kind.${run.kind}`),
    }),
    body,
    extra: workflowExtra(event, run),
  };
}


function workflowExtra(event: BackendEvent, run: WorkflowRun | WorkflowRunSummary): Record<string, string> {
  return {
    taskId: run.taskId,
    projectId: event.projectId ?? run.projectId,
    eventType: event.eventType,
    workflowKind: run.kind,
    workflowStatus: run.displayStatus,
  };
}


/** The ordinary Task drawer route remains in notifications.ts; only Workflow navigation loads here. */
export async function openWorkflowNotification(
  projectId: string,
  taskId: string,
  workflowStatus: unknown,
): Promise<void> {
  const projects = useProjectStore.getState();
  let project = projects.currentProject.projectId === projectId
    ? { rootPath: projects.currentProject.rootPath }
    : projects.recentProjects.find((candidate) => candidate.projectId === projectId);
  if (!project?.rootPath) return;
  if (projects.currentProject.projectId !== projectId) {
    const assessment = await projects.assessProject(project.rootPath);
    const canOpen =
      assessment.health !== "unreadable" &&
      !["ambiguous_markdown", "ordinary_materials", "unknown"].includes(assessment.format);
    if (!canOpen) return;
    await projects.openAssessedProject(assessment.assessmentId);
    project = { rootPath: project.rootPath };
  }

  const behavior = useSettingsStore.getState().settings.notificationClickBehavior;
  if (behavior === "activate_window_only") return;
  if (behavior === "error_log") {
    useTaskStore.getState().openDrawer(taskId);
    return;
  }
  const workflowProject = { projectId, rootPath: project.rootPath };
  try {
    const run = await hydrateAndSelectWorkflowRun(workflowProject, taskId);
    if (behavior === "result_page" && workflowStatus === "completed") {
      await openWorkflowResult(workflowProject, run);
      return;
    }
    useNavigationStore.getState().setActiveView("workflows");
  } catch (error) {
    if (error instanceof Error && ["WORKFLOW_NAVIGATION_SUPERSEDED", "WORKFLOW_PROJECT_CHANGED"].includes(error.message)) return;
    throw error;
  }
}

const notifiedWorkflowStatus = new Map<string, WorkflowDisplayStatus>();
const notifyingWorkflowStatus = new Set<string>();

export async function notifyWorkflowEvent(
  event: BackendEvent,
  run: WorkflowRun | WorkflowRunSummary,
  checkPermission: () => Promise<boolean>,
  isCurrent: () => boolean,
): Promise<void> {
  if (!isCurrent()) return;
  const taskKey = `${run.sessionId ?? "legacy"}\0${run.taskId}`;
  if (notifiedWorkflowStatus.get(taskKey) === run.displayStatus) return;
  const notificationKey = `${taskKey}\0${run.displayStatus}\0${run.revision ?? run.updatedAt}`;
  if (notifyingWorkflowStatus.has(notificationKey)) return;
  const options = workflowNotificationOptions(event, run);
  if (!options) return;
  notifyingWorkflowStatus.add(notificationKey);
  try {
    if (!(await checkPermission().catch(() => false))) return;
    if (!isCurrent() || notifiedWorkflowStatus.get(taskKey) === run.displayStatus) return;
    const previousStatus = notifiedWorkflowStatus.get(taskKey);
    notifiedWorkflowStatus.set(taskKey, run.displayStatus);
    try {
      await sendNotification(options);
    } catch (error) {
      if (notifiedWorkflowStatus.get(taskKey) === run.displayStatus) {
        if (previousStatus) notifiedWorkflowStatus.set(taskKey, previousStatus);
        else notifiedWorkflowStatus.delete(taskKey);
      }
      throw error;
    }
  } finally {
    notifyingWorkflowStatus.delete(notificationKey);
  }
}
