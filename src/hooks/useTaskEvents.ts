import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { useProjectStore } from "../stores/projectStore";
import { notifyTaskEvent } from "../services/notifications";
import {
  fetchTasks,
  handleTaskEvent,
  recoverTasksForProject,
} from "../stores/taskStore";
import type { BackendEvent } from "../types/task";

const hasTauri = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const TASK_EVENT_CHANNELS = [
  "task://updated",
  "task://log",
  "task://completed",
  "task://failed",
  "task://cancelled",
  "confirmation://requested",
  "project://refreshed",
  "wiki://changed",
  "graph://updated",
  "agent://output",
] as const;

/**
 * Subscribes to all backend task/event channels and keeps the task store in sync.
 * Also recovers persisted tasks when the active project root changes, so background
 * work survives view switches and app restarts, and fires OS notifications for
 * completion/failure/confirmation events.
 */
export function useTaskEvents(): void {
  const currentProject = useProjectStore((state) => state.currentProject);

  useEffect(() => {
    if (!hasTauri()) return;

    const unlisteners: Array<() => void> = [];
    let cancelled = false;

    for (const channel of TASK_EVENT_CHANNELS) {
      listen<BackendEvent>(channel, (evt) => {
        const event = evt.payload as BackendEvent;
        handleTaskEvent(event);
        void notifyTaskEvent(event);
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

    fetchTasks().catch(() => {});

    return () => {
      cancelled = true;
      unlisteners.forEach((fn) => fn());
    };
  }, []);

  // Recover persisted tasks whenever the active project root changes.
  useEffect(() => {
    if (currentProject.rootPath) {
      recoverTasksForProject(currentProject.rootPath).catch(() => {});
    }
  }, [currentProject.rootPath]);
}
