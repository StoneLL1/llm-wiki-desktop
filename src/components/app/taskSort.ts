import type { BackendTask, TaskStatus } from "../../types/task";
import { TASK_STATUS_ORDER } from "../../types/task";

export type TaskSortMode = "execution_time" | "updated_time" | "status";

export const DEFAULT_TASK_SORT_MODE: TaskSortMode = "execution_time";
export const TASK_SORT_STORAGE_KEY = "llm-wiki-desktop.taskSortMode.v1";

const TASK_SORT_MODES: TaskSortMode[] = ["execution_time", "updated_time", "status"];

function timeValue(value: string | null | undefined): number {
  if (!value) return 0;
  const parsed = Date.parse(value);
  return Number.isFinite(parsed) ? parsed : 0;
}

function executionTime(task: BackendTask): number {
  return timeValue(task.startedAt) || timeValue(task.updatedAt);
}

function updatedTime(task: BackendTask): number {
  return timeValue(task.updatedAt);
}

function statusOrder(status: TaskStatus): number {
  return TASK_STATUS_ORDER[status] ?? 99;
}

export function isTaskSortMode(value: string | null): value is TaskSortMode {
  return TASK_SORT_MODES.includes(value as TaskSortMode);
}

export function readTaskSortModePreference(): TaskSortMode {
  try {
    const stored = window.localStorage.getItem(TASK_SORT_STORAGE_KEY);
    return isTaskSortMode(stored) ? stored : DEFAULT_TASK_SORT_MODE;
  } catch {
    return DEFAULT_TASK_SORT_MODE;
  }
}

export function writeTaskSortModePreference(mode: TaskSortMode): void {
  try {
    window.localStorage.setItem(TASK_SORT_STORAGE_KEY, mode);
  } catch {
    /* Preference persistence is best-effort only. */
  }
}

export function sortTasks(tasks: BackendTask[], mode: TaskSortMode): BackendTask[] {
  return [...tasks].sort((a, b) => {
    if (mode === "status") {
      return (
        statusOrder(a.status) - statusOrder(b.status) ||
        updatedTime(b) - updatedTime(a) ||
        a.id.localeCompare(b.id)
      );
    }
    const left = mode === "execution_time" ? executionTime(a) : updatedTime(a);
    const right = mode === "execution_time" ? executionTime(b) : updatedTime(b);
    return right - left || updatedTime(b) - updatedTime(a) || a.id.localeCompare(b.id);
  });
}
