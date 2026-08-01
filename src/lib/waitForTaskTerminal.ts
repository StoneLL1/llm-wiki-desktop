import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { isTerminalStatus, type BackendEvent, type BackendTask } from "../types/task";

const TERMINAL_CHANNELS = ["task://completed", "task://failed", "task://cancelled"] as const;

const DEFAULT_POLL_MS = 1000;
const DEFAULT_TIMEOUT_MS = 10 * 60 * 1000;

export interface WaitForTaskTerminalOptions {
  projectId: string;
  projectRootPath: string;
  pollMs?: number;
  timeoutMs?: number;
}

/**
 * Rejected by {@link waitForTaskTerminal} when the task has not reached a
 * terminal status within `timeoutMs`. Carries a stable `code` so callers can
 * branch on it (e.g. show "timed out" vs. a generic error).
 */
export class WaitForTaskTerminalTimeoutError extends Error {
  readonly code = "TASK_WAIT_TIMEOUT";
  readonly taskId: string;
  readonly timeoutMs: number;
  constructor(taskId: string, timeoutMs: number) {
    super(`Task ${taskId} did not reach a terminal status within ${timeoutMs}ms.`);
    this.name = "WaitForTaskTerminalTimeoutError";
    this.taskId = taskId;
    this.timeoutMs = timeoutMs;
  }
}

/**
 * Resolves when the given task reaches a terminal status. Driven by the
 * backend event bus with a `get_task` polling fallback so a missed event or a
 * failed listener registration can never leave the promise pending forever.
 *
 * - If `task` is already terminal, resolves immediately.
 * - Otherwise subscribes to the terminal event channels and polls `get_task`
 *   every `pollMs` (default 1s) as a race-safe fallback for events that arrive
 *   before listeners attach or are dropped by the bus.
 * - Rejects with {@link WaitForTaskTerminalTimeoutError} after `timeoutMs`
 *   (default 10 min) if no terminal signal arrives.
 * - Listener registration failures are non-fatal (browser-only dev has no
 *   event bus); polling is the authoritative fallback, and the timeout guards
 *   a permanently unavailable bus.
 * - Every listener and timer is released on resolve, reject, and timeout.
 */
export function waitForTaskTerminal(
  task: BackendTask,
  options: WaitForTaskTerminalOptions,
): Promise<BackendTask> {
  if (isTerminalStatus(task.status)) return Promise.resolve(task);
  const taskId = task.id;
  // Floor pollMs at 1ms: a 0/negative interval makes setTimeout fire
  // immediately, and the recursive reschedule would starve the macro-task
  // queue under real timers. timeoutMs is left as-is (0 just means "fail
  // fast", which is a valid caller choice and never starves).
  const pollMs = Math.max(options.pollMs ?? DEFAULT_POLL_MS, 1);
  const timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  const request = {
    taskId,
    projectId: options.projectId,
    projectRootPath: options.projectRootPath,
  };

  return new Promise<BackendTask>((resolve, reject) => {
    let settled = false;
    const unlisteners: UnlistenFn[] = [];
    let pollTimer: ReturnType<typeof setTimeout> | null = null;
    let deadlineTimer: ReturnType<typeof setTimeout> | null = null;

    const cleanup = (): void => {
      for (const unlisten of unlisteners) {
        try {
          unlisten();
        } catch {
          // Best-effort; a failing unlisten must not shadow the real result.
        }
      }
      unlisteners.length = 0;
      if (pollTimer !== null) {
        clearTimeout(pollTimer);
        pollTimer = null;
      }
      if (deadlineTimer !== null) {
        clearTimeout(deadlineTimer);
        deadlineTimer = null;
      }
    };

    const finish = (next: BackendTask): void => {
      if (settled) return;
      settled = true;
      cleanup();
      resolve(next);
    };

    const fail = (error: unknown): void => {
      if (settled) return;
      settled = true;
      cleanup();
      reject(error);
    };

    function schedulePoll(): void {
      if (settled) return;
      pollTimer = setTimeout(() => {
        pollTimer = null;
        checkOnce();
      }, pollMs);
    }

    function checkOnce(): void {
      invoke<BackendTask>("get_task", { request })
        .then((next) => {
          // Mirror the event path's taskId filter: get_task is a point query,
          // but a defensive id check prevents silently resolving on a wrong
          // task if the backend ever returns one.
          if (next && next.id === taskId && isTerminalStatus(next.status)) {
            finish(next);
          } else {
            schedulePoll();
          }
        })
        .catch(() => {
          // Transient get_task failure (e.g. browser-only dev without a real
          // invoke). Keep polling; the deadline guards a permanently dead bus.
          schedulePoll();
        });
    }

    for (const channel of TERMINAL_CHANNELS) {
      listen<BackendEvent>(channel, (evt) => {
        const event = evt.payload;
        const next = event?.payload as BackendTask | undefined;
        if (
          event?.taskId === taskId
          && event.projectId === options.projectId
          && next
        ) {
          finish(next);
        }
      })
        .then((unlisten) => {
          if (settled) {
            try {
              unlisten();
            } catch {
              // Already settled; just release the late-registered listener.
            }
          } else {
            unlisteners.push(unlisten);
          }
        })
        .catch(() => {
          // Browser-only dev has no event bus; polling get_task is the
          // authoritative fallback, so listener failure is non-fatal. The
          // overall timeout catches a totally-wedged bus.
        });
    }

    deadlineTimer = setTimeout(() => {
      fail(new WaitForTaskTerminalTimeoutError(taskId, timeoutMs));
    }, timeoutMs);

    checkOnce();
  });
}
