import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { useProjectStore } from "../stores/projectStore";
import { useToastStore } from "../stores/toastStore";
import i18next from "i18next";
import {
  notifyTaskEvent,
  registerNotificationActionListener,
} from "../services/notifications";
import {
  handleTaskEvent,
  recoverTasksForProject,
} from "../stores/taskStore";
import type { BackendEvent } from "../types/task";

const hasTauri = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

const TASK_EVENT_CHANNELS = [
  "task://updated",
  "task://log",
  "task://completed",
  "task://failed",
  "task://cancelled",
  "task://activity",
  "task://stream-output",
  "workflow://updated",
  "confirmation://requested",
  "project://refreshed",
  "wiki://changed",
  "graph://updated",
  "agent://output",
] as const;

type TaskEventListener = (event: BackendEvent) => void;
const taskEventListeners = new Set<TaskEventListener>();

export function registerTaskEventListener(listener: TaskEventListener): () => void {
  taskEventListeners.add(listener);
  return () => taskEventListeners.delete(listener);
}

export function notifyTaskEventListeners(event: BackendEvent): void {
  for (const listener of taskEventListeners) listener(event);
}

export function isTaskEventForProject(event: BackendEvent, projectId: string): boolean {
  return event.projectId === projectId;
}

/**
 * Subscribes to all backend task/event channels and keeps the task store in sync.
 * Also recovers persisted tasks when the active project root changes, so background
 * work survives view switches and app restarts, and fires OS notifications for
 * completion/failure/confirmation events.
 */
export function useTaskEvents(): void {
  const currentProject = useProjectStore((state) => state.currentProject);
  const pushToast = useToastStore((state) => state.pushToast);

  useEffect(() => {
    if (!hasTauri()) return;
    const unlisteners: Array<() => void> = [];
    let cancelled = false;

    for (const channel of TASK_EVENT_CHANNELS) {
      listen<BackendEvent>(channel, (evt) => {
        const event = evt.payload as BackendEvent;
        if (event.eventType === "workflow_updated") void notifyTaskEvent(event);
        if (!isTaskEventForProject(event, currentProject.projectId)) return;
        handleTaskEvent(event);
        notifyTaskEventListeners(event);
        if (event.eventType !== "workflow_updated") void notifyTaskEvent(event);
      })
        .then((unlisten) => {
          if (cancelled) {
            unlisten();
          } else {
            unlisteners.push(unlisten);
          }
        })
        .catch(() => {
          // Tauri event system unavailable (browser-only dev)
        });
    }

    registerNotificationActionListener()
      .then((unlisten) => {
        if (cancelled) unlisten();
        else unlisteners.push(unlisten);
      })
      .catch(() => {
        // Notification actions are unavailable in browser-only development.
      });

    return () => {
      cancelled = true;
      unlisteners.forEach((fn) => fn());
    };
  }, [currentProject.projectId]);

  // Recover persisted tasks whenever the active project root changes.
  useEffect(() => {
    if (currentProject.rootPath) {
      recoverTasksForProject(currentProject.projectId, currentProject.rootPath).catch((error) => {
        pushToast("error", i18next.t("task.recoverError", { message: errorMessage(error) }));
      });
    }
  }, [currentProject.projectId, currentProject.rootPath, pushToast]);
}
