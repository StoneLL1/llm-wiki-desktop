import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  isPermissionGranted,
  onAction,
  requestPermission,
  sendNotification,
  type Options,
} from "@tauri-apps/plugin-notification";
import i18next from "i18next";

import { hydrateAndSelectWorkflowRun, openWorkflowResult } from "./workflowNavigation";
import { useNavigationStore } from "../stores/navigationStore";
import { useProjectStore } from "../stores/projectStore";
import { useSettingsStore } from "../stores/settingsStore";
import { useTaskStore } from "../stores/taskStore";
import type { BackendEvent, BackendTask } from "../types/task";
import type { WorkflowDisplayStatus, WorkflowRun } from "../types/workflow";

let permissionGranted: boolean | null = null;
const notifiedWorkflowStatus = new Map<string, WorkflowDisplayStatus>();
const ALLOWED_WORKFLOW_STATUSES = new Set<WorkflowDisplayStatus>([
  "waiting_for_confirmation",
  "completed",
  "failed",
]);

async function ensurePermission(): Promise<boolean> {
  if (permissionGranted === true) return true;
  let granted = await isPermissionGranted();
  if (!granted) granted = (await requestPermission()) === "granted";
  permissionGranted = granted;
  return granted;
}

function taskFromPayload(payload: unknown): BackendTask | null {
  return payload && typeof payload === "object" ? (payload as BackendTask) : null;
}

function safeWorkflowSummary(run: WorkflowRun): string {
  if (!run.result) return i18next.t(`workflows.status.${run.displayStatus}`);
  if (run.result.kind === "update_wiki") {
    return i18next.t("notification.workflow.updateSummary", {
      changed: run.result.created + run.result.updated,
      deleted: run.result.deleted,
      conflicted: run.result.conflicted,
    });
  }
  if (run.result.kind === "health_check") {
    return i18next.t("notification.workflow.healthSummary", {
      errors: run.result.errorCount,
      warnings: run.result.warningCount,
    });
  }
  return i18next.t("notification.workflow.generateSummary", {
    count: run.result.outputPaths.length,
  });
}

function workflowExtra(event: BackendEvent, run: WorkflowRun): Record<string, string> {
  return {
    taskId: run.taskId,
    projectId: event.projectId ?? run.projectId,
    eventType: event.eventType,
    workflowKind: run.kind,
    workflowStatus: run.displayStatus,
  };
}

export async function handleNotificationAction(
  notification: Pick<Options, "extra">,
): Promise<void> {
  const taskId = notification.extra?.taskId;
  const projectId = notification.extra?.projectId;
  const workflowKind = notification.extra?.workflowKind;
  const workflowStatus = notification.extra?.workflowStatus;
  const window = getCurrentWindow();
  await window.show();
  await window.setFocus();
  if (typeof taskId !== "string" || !taskId) return;

  if (typeof workflowKind !== "string" || typeof projectId !== "string") {
    useTaskStore.getState().openDrawer(taskId);
    return;
  }

  const projects = useProjectStore.getState();
  let project = projects.currentProject.projectId === projectId
    ? { rootPath: projects.currentProject.rootPath }
    : projects.recentProjects.find((candidate) => candidate.projectId === projectId);
  if (!project?.rootPath) return;
  if (projects.currentProject.projectId !== projectId) {
    const response = await projects.openProject(project.rootPath);
    if (response.kind !== "opened") return;
    project = { rootPath: project.rootPath };
  }

  const behavior = useSettingsStore.getState().settings.notificationClickBehavior;
  if (behavior === "activate_window_only") return;
  if (behavior === "error_log") {
    useTaskStore.getState().openDrawer(taskId);
    return;
  }
  const workflowProject = { projectId, rootPath: project.rootPath };
  const run = await hydrateAndSelectWorkflowRun(workflowProject, taskId);
  if (behavior === "result_page" && workflowStatus === "completed") {
    await openWorkflowResult(workflowProject, run);
    return;
  }

  useNavigationStore.getState().setActiveView("workflows");
}

export async function registerNotificationActionListener(): Promise<() => void> {
  const listener = await onAction((notification) => void handleNotificationAction(notification));
  return () => listener.unregister();
}

export async function notifyTaskEvent(event: BackendEvent): Promise<void> {
  const settings = useSettingsStore.getState().settings.systemNotifications;
  const t = i18next.t;

  if (event.eventType === "workflow_updated") {
    const run = event.payload as WorkflowRun;
    if (!run?.taskId || !ALLOWED_WORKFLOW_STATUSES.has(run.displayStatus)) return;
    if (notifiedWorkflowStatus.get(run.taskId) === run.displayStatus) return;
    const enabled = run.displayStatus === "completed"
      ? settings.onTaskCompleted
      : run.displayStatus === "failed"
        ? settings.onTaskFailed
        : settings.onConfirmationNeeded;
    if (!enabled || !(await ensurePermission().catch(() => false))) return;
    notifiedWorkflowStatus.set(run.taskId, run.displayStatus);
    const projectName =
      useProjectStore.getState().currentProject.projectId === run.projectId
        ? useProjectStore.getState().currentProject.name
        : useProjectStore.getState().recentProjects.find((project) => project.projectId === run.projectId)?.name ?? t("notification.workflow.unknownProject");
    await sendNotification({
      title: t("notification.workflow.title", {
        project: projectName,
        workflow: t(`workflows.kind.${run.kind}`),
      }),
      body: safeWorkflowSummary(run),
      extra: workflowExtra(event, run),
    });
    return;
  }

  if (!event.taskId || !(await ensurePermission().catch(() => false))) return;
  const task = taskFromPayload(event.payload);
  if (task?.taskType === "workflow") return;
  if (event.eventType === "task_completed" && settings.onTaskCompleted) {
    await sendNotification({
      title: t("notification.taskCompleted.title"),
      body: task ? t("notification.taskCompleted.body", { title: task.title }) : t("notification.taskCompleted.bodyGeneric"),
      extra: { taskId: event.taskId, eventType: event.eventType },
    });
  } else if (event.eventType === "task_failed" && settings.onTaskFailed) {
    const reason = task?.error?.message ?? t("notification.taskFailed.unknown");
    await sendNotification({
      title: t("notification.taskFailed.title"),
      body: task ? t("notification.taskFailed.body", { title: task.title, reason }) : t("notification.taskFailed.bodyGeneric", { reason }),
      extra: { taskId: event.taskId, eventType: event.eventType },
    });
  } else if (event.eventType === "confirmation_requested" && settings.onConfirmationNeeded) {
    await sendNotification({
      title: t("notification.confirmationNeeded.title"),
      body: task ? t("notification.confirmationNeeded.body", { title: task.title }) : t("notification.confirmationNeeded.bodyGeneric"),
      extra: { taskId: event.taskId, eventType: event.eventType },
    });
  }
}
