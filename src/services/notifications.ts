import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import i18next from "i18next";
import type { BackendEvent, BackendTask } from "../types/task";

let permissionGranted: boolean | null = null;

async function ensurePermission(): Promise<boolean> {
  if (permissionGranted === true) return true;
  let granted = await isPermissionGranted();
  if (!granted) {
    const permission = await requestPermission();
    granted = permission === "granted";
  }
  permissionGranted = granted;
  return granted;
}

function taskFromPayload(payload: unknown): BackendTask | null {
  if (payload && typeof payload === "object") {
    return payload as BackendTask;
  }
  return null;
}

/**
 * Fire an OS notification for a task-related backend event. No-ops silently when
 * permission has not been granted or the notification plugin is unavailable.
 */
export async function notifyTaskEvent(event: BackendEvent): Promise<void> {
  if (!event.taskId) return;
  if (!(await ensurePermission().catch(() => false))) return;

  const task = taskFromPayload(event.payload);
  const t = i18next.t;

  switch (event.eventType) {
    case "task_completed": {
      await sendNotification({
        title: t("notification.taskCompleted.title"),
        body: task
          ? t("notification.taskCompleted.body", { title: task.title })
          : t("notification.taskCompleted.bodyGeneric"),
      });
      break;
    }
    case "task_failed": {
      const reason = task?.error?.message ?? t("notification.taskFailed.unknown");
      await sendNotification({
        title: t("notification.taskFailed.title"),
        body: task
          ? t("notification.taskFailed.body", { title: task.title, reason })
          : t("notification.taskFailed.bodyGeneric", { reason }),
      });
      break;
    }
    case "confirmation_requested": {
      await sendNotification({
        title: t("notification.confirmationNeeded.title"),
        body: task
          ? t("notification.confirmationNeeded.body", { title: task.title })
          : t("notification.confirmationNeeded.bodyGeneric"),
      });
      break;
    }
    default:
      break;
  }
}
