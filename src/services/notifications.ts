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

type NotificationPermissionState = "unknown" | "granted" | "denied";

let permissionEpoch = 0;
let permissionState: { epoch: number; value: NotificationPermissionState } = {
  epoch: permissionEpoch,
  value: "unknown",
};
let permissionCheckInFlight: { epoch: number; promise: Promise<boolean> } | null = null;
let permissionRequestInFlight: { epoch: number; promise: Promise<boolean> } | null = null;
const notifiedWorkflowStatus = new Map<string, WorkflowDisplayStatus>();
const notifyingWorkflowStatus = new Set<string>();
const ALLOWED_WORKFLOW_STATUSES = new Set<WorkflowDisplayStatus>([
  "waiting_for_confirmation",
  "completed",
  "failed",
]);

export function invalidateNotificationPermissionEpoch(): void {
  permissionEpoch += 1;
  permissionState = { epoch: permissionEpoch, value: "unknown" };
  permissionCheckInFlight = null;
}

async function checkPermission(): Promise<boolean> {
  if (permissionState.epoch === permissionEpoch && permissionState.value !== "unknown") {
    return permissionState.value === "granted";
  }
  if (permissionCheckInFlight?.epoch === permissionEpoch) {
    return permissionCheckInFlight.promise;
  }
  const epoch = permissionEpoch;
  const promise = isPermissionGranted()
    .then((granted) => {
      if (permissionEpoch === epoch) {
        permissionState = { epoch, value: granted ? "granted" : "denied" };
      }
      return permissionEpoch === epoch && granted;
    })
    .finally(() => {
      if (permissionCheckInFlight?.epoch === epoch) permissionCheckInFlight = null;
    });
  permissionCheckInFlight = { epoch, promise };
  return promise;
}

export async function requestNotificationPermissionFromUser(): Promise<boolean> {
  const callerEpoch = permissionEpoch;
  const joinRequest = async (
    activeRequest: NonNullable<typeof permissionRequestInFlight>,
  ): Promise<boolean> => {
    const granted = await activeRequest.promise.catch(() => false);
    if (permissionEpoch !== callerEpoch) return false;
    if (activeRequest.epoch === callerEpoch) return granted;
    permissionState = { epoch: callerEpoch, value: "unknown" };
    permissionCheckInFlight = null;
    return checkPermission().catch(() => false);
  };
  if (permissionRequestInFlight) return joinRequest(permissionRequestInFlight);
  if (await checkPermission().catch(() => false)) return true;
  if (permissionEpoch !== callerEpoch) return false;
  if (permissionRequestInFlight) return joinRequest(permissionRequestInFlight);
  const epoch = permissionEpoch;
  const promise = Promise.resolve()
    .then(() => requestPermission())
    .then((result) => {
      const granted = result === "granted";
      if (permissionEpoch === epoch) {
        permissionState = { epoch, value: granted ? "granted" : "denied" };
      }
      return permissionEpoch === epoch && granted;
    })
    .finally(() => {
      if (permissionRequestInFlight?.promise === promise) permissionRequestInFlight = null;
    });
  permissionRequestInFlight = { epoch, promise };
  return promise;
}

function taskFromPayload(payload: unknown, taskId: string): BackendTask | null {
  if (!payload || typeof payload !== "object") return null;
  const task = payload as Partial<BackendTask>;
  return task.id === taskId
    && typeof task.taskType === "string"
    && typeof task.status === "string"
    ? task as BackendTask
    : null;
}

function safeWorkflowSummary(run: WorkflowRun): string | null {
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

function workflowNotificationOptions(event: BackendEvent, run: WorkflowRun): Options | null {
  if (
    !run
    || typeof run !== "object"
    || typeof run.taskId !== "string"
    || !run.taskId
    || typeof run.projectId !== "string"
    || !["update_wiki", "health_check", "generate_content"].includes(run.kind)
    || !ALLOWED_WORKFLOW_STATUSES.has(run.displayStatus)
    || !("result" in run)
    || (run.result !== null && typeof run.result !== "object")
    || (run.result != null && run.result.kind !== run.kind)
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

function taskNotificationOptions(event: BackendEvent, task: BackendTask | null): Options | null {
  if (!event.taskId) return null;
  if (event.payload !== null && !task) return null;
  if (task?.taskType === "workflow") return null;
  if (event.eventType === "task_completed") {
    return {
      title: i18next.t("notification.taskCompleted.title"),
      body: i18next.t("notification.taskCompleted.bodyGeneric"),
      extra: { taskId: event.taskId, eventType: event.eventType },
    };
  }
  if (event.eventType === "task_failed") {
    return {
      title: i18next.t("notification.taskFailed.title"),
      body: i18next.t("notification.taskFailed.bodyGeneric", {
        reason: i18next.t("notification.taskFailed.unknown"),
      }),
      extra: { taskId: event.taskId, eventType: event.eventType },
    };
  }
  if (event.eventType === "confirmation_requested") {
    return {
      title: i18next.t("notification.confirmationNeeded.title"),
      body: i18next.t("notification.confirmationNeeded.bodyGeneric"),
      extra: { taskId: event.taskId, eventType: event.eventType },
    };
  }
  return null;
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

  if (event.eventType === "workflow_updated") {
    const run = event.payload as WorkflowRun;
    if (!run?.taskId || !ALLOWED_WORKFLOW_STATUSES.has(run.displayStatus)) return;
    if (notifiedWorkflowStatus.get(run.taskId) === run.displayStatus) return;
    const enabled = run.displayStatus === "completed"
      ? settings.onTaskCompleted
      : run.displayStatus === "failed"
        ? settings.onTaskFailed
        : settings.onConfirmationNeeded;
    if (!enabled) return;
    const notificationKey = `${run.taskId}\0${run.displayStatus}`;
    if (notifyingWorkflowStatus.has(notificationKey)) return;
    const options = workflowNotificationOptions(event, run);
    if (!options) return;
    notifyingWorkflowStatus.add(notificationKey);
    if (!(await checkPermission().catch(() => false))) {
      notifyingWorkflowStatus.delete(notificationKey);
      return;
    }
    notifiedWorkflowStatus.set(run.taskId, run.displayStatus);
    try {
      await sendNotification(options);
    } finally {
      notifyingWorkflowStatus.delete(notificationKey);
    }
    return;
  }

  if (!event.taskId) return;
  const task = taskFromPayload(event.payload, event.taskId);
  const eligible = (event.eventType === "task_completed" && settings.onTaskCompleted)
    || (event.eventType === "task_failed" && settings.onTaskFailed)
    || (event.eventType === "confirmation_requested" && settings.onConfirmationNeeded);
  if (!eligible) return;
  const options = taskNotificationOptions(event, task);
  if (!options || !(await checkPermission().catch(() => false))) return;
  await sendNotification(options);
}
