import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  isPermissionGranted,
  onAction,
  requestPermission,
  sendNotification,
  type Options,
} from "@tauri-apps/plugin-notification";
import i18next from "i18next";

import { useProjectStore } from "../stores/projectStore";
import { useSettingsStore } from "../stores/settingsStore";
import { useTaskStore } from "../stores/taskStore";
import { compareWorkflowRevision } from "./workflowTaskSnapshot";
import type { BackendEvent, BackendTask } from "../types/task";
import type { WorkflowDisplayStatus, WorkflowRun, WorkflowRunSummary } from "../types/workflow";

const loadWorkflowNotifications = () => import("./workflowNotifications");

type NotificationPermissionState = "unknown" | "granted" | "denied";

let permissionEpoch = 0;
let permissionState: { epoch: number; value: NotificationPermissionState } = {
  epoch: permissionEpoch,
  value: "unknown",
};
let permissionCheckInFlight: { epoch: number; promise: Promise<boolean> } | null = null;
let permissionRequestInFlight: { epoch: number; promise: Promise<boolean> } | null = null;
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

  const origin = useProjectStore.getState();
  try {
    const { openWorkflowNotification } = await loadWorkflowNotifications();
    const latest = useProjectStore.getState();
    if (latest.currentProject.projectId !== origin.currentProject.projectId
      || latest.currentProject.rootPath !== origin.currentProject.rootPath
      || latest.authority !== origin.authority) return;
    await openWorkflowNotification(projectId, taskId, workflowStatus);
  } catch {
    // A notification click can be retried if its optional Workflow chunk is unavailable.
  }
}

export async function registerNotificationActionListener(): Promise<() => void> {
  const listener = await onAction((notification) => void handleNotificationAction(notification));
  return () => listener.unregister();
}

function currentWorkflowNotification(
  event: BackendEvent,
  run: WorkflowRun | WorkflowRunSummary,
  projectRootPath: string,
): boolean {
  const tasks = useTaskStore.getState();
  const accepted = tasks.workflowById[run.taskId];
  const { currentProject, authority } = useProjectStore.getState();
  return event.taskId === run.taskId && event.projectId === run.projectId
    && currentProject.projectId === run.projectId && currentProject.rootPath === projectRootPath
    && authority?.projectId === run.projectId
    && authority.canonicalIdentityKey === run.canonicalIdentityKey
    && authority.identityRevision === run.identityRevision
    && !!accepted && accepted.projectId === run.projectId
    && accepted.canonicalIdentityKey === run.canonicalIdentityKey
    && accepted.identityRevision === run.identityRevision
    && accepted.sessionId === run.sessionId
    && (tasks.workflowSessionId === null ? !run.sessionId : run.sessionId === tasks.workflowSessionId)
    && (!run.sessionId || !tasks.retiredWorkflowSessions.includes(run.sessionId))
    && compareWorkflowRevision(accepted, run) === 0
    && accepted.displayStatus === run.displayStatus;
}

function workflowNotificationEnabled(status: WorkflowDisplayStatus): boolean {
  const settings = useSettingsStore.getState().settings.systemNotifications;
  return status === "completed" ? settings.onTaskCompleted
    : status === "failed" ? settings.onTaskFailed : settings.onConfirmationNeeded;
}

export async function notifyTaskEvent(event: BackendEvent): Promise<void> {
  const settings = useSettingsStore.getState().settings.systemNotifications;

  if (event.eventType === "workflow_updated") {
    const run = event.payload as WorkflowRun | WorkflowRunSummary;
    if (!run?.taskId || !ALLOWED_WORKFLOW_STATUSES.has(run.displayStatus)) return;
    const projectRootPath = useProjectStore.getState().currentProject.rootPath;
    if (!currentWorkflowNotification(event, run, projectRootPath)) return;
    if (!workflowNotificationEnabled(run.displayStatus)) return;
    try {
      const { notifyWorkflowEvent } = await loadWorkflowNotifications();
      await notifyWorkflowEvent(event, run, checkPermission, () => currentWorkflowNotification(event, run, projectRootPath)
        && workflowNotificationEnabled(run.displayStatus));
    } catch {
      // A repeated event may retry if the Workflow notification chunk could not load.
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
