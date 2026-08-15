import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { useProjectStore } from "../stores/projectStore";
import { useToastStore } from "../stores/toastStore";
import i18next from "i18next";
import {
  invalidateNotificationPermissionEpoch,
  notifyTaskEvent,
  registerNotificationActionListener,
} from "../services/notifications";
import {
  clearPendingTaskEvents,
  dispatchTaskEvent,
  registerTaskEventOwner,
  retainTaskEventProject,
} from "../services/taskEventDispatcher";
import {
  handleTaskEvent,
  recoverTasksForProject,
} from "../stores/taskStore";
import type { BackendEvent } from "../types/task";
import type { ProjectSummary } from "../types/project";

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
  "import://session-patch",
] as const;

function isProjectSummary(payload: unknown): payload is ProjectSummary {
  return typeof payload === "object"
    && payload !== null
    && "projectId" in payload
    && "rootPath" in payload
    && "inventoryState" in payload;
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

    const unregisterStoreListener = registerTaskEventOwner((event) => {
      const activeProject = useProjectStore.getState().currentProject;
      if (!isTaskEventForProject(event, activeProject.projectId)) return;
      if (event.eventType === "project_refreshed" && isProjectSummary(event.payload)) {
        useProjectStore.getState().setCurrentProject(event.payload);
      }
      handleTaskEvent(event);
      if (event.eventType !== "workflow_updated") void notifyTaskEvent(event);
    });

    for (const channel of TASK_EVENT_CHANNELS) {
      listen<BackendEvent>(channel, (evt) => {
        if (cancelled) return;
        const event = evt.payload as BackendEvent;
        if (event.eventType === "workflow_updated") void notifyTaskEvent(event);
        const activeProject = useProjectStore.getState().currentProject;
        if (!isTaskEventForProject(event, activeProject.projectId)) return;
        dispatchTaskEvent(event);
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

    const refreshPermissionEpoch = () => invalidateNotificationPermissionEpoch();
    window.addEventListener("focus", refreshPermissionEpoch);

    return () => {
      cancelled = true;
      const activeProjectId = useProjectStore.getState().currentProject.projectId;
      clearPendingTaskEvents((event) => event.projectId === activeProjectId);
      unregisterStoreListener();
      window.removeEventListener("focus", refreshPermissionEpoch);
      unlisteners.forEach((fn) => fn());
    };
  }, []);

  useEffect(() => {
    retainTaskEventProject(currentProject.projectId);
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
