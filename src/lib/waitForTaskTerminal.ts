import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { isTerminalStatus, type BackendEvent, type BackendTask } from "../types/task";

const TERMINAL_CHANNELS = ["task://completed", "task://failed", "task://cancelled"] as const;

/**
 * Resolves when the given task reaches a terminal status, driven by the backend
 * event bus instead of polling get_task. If the task is already terminal it
 * resolves immediately. Otherwise it subscribes to the terminal event channels
 * (race-safe: an initial get_task catches tasks that terminate before the
 * listeners attach) and unregisters every listener once it settles.
 */
export function waitForTaskTerminal(task: BackendTask): Promise<BackendTask> {
  if (isTerminalStatus(task.status)) return Promise.resolve(task);
  const taskId = task.id;

  return new Promise<BackendTask>((resolve) => {
    let settled = false;
    const unlisteners: UnlistenFn[] = [];

    const finish = (next: BackendTask): void => {
      if (settled) return;
      settled = true;
      for (const unlisten of unlisteners) unlisten();
      resolve(next);
    };

    for (const channel of TERMINAL_CHANNELS) {
      listen<BackendEvent>(channel, (evt) => {
        const event = evt.payload;
        const next = event?.payload as BackendTask | undefined;
        if (event?.taskId === taskId && next) {
          finish(next);
        }
      })
        .then((unlisten) => {
          if (settled) {
            unlisten();
          } else {
            unlisteners.push(unlisten);
          }
        })
        .catch(() => {
          // Browser-only dev has no event bus; the caller's invoke would have
          // rejected first, so this promise is never awaited there.
        });
    }

    invoke<BackendTask>("get_task", { request: { taskId } })
      .then((next) => {
        if (next && isTerminalStatus(next.status)) finish(next);
      })
      .catch(() => {
        // Ignore — the terminal event listeners are the authoritative source.
      });
  });
}
