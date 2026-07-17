import { create } from "zustand";
import type {
  BackendTask,
  BackendEvent,
  LogLine,
} from "../types/task";
import { isTerminalStatus } from "../types/task";

interface TaskState {
  tasks: BackendTask[];
  logs: Record<string, LogLine[]>;
  drawerOpen: boolean;
  selectedTaskId: string | null;
  runningCount: number;
  tasksHydrated: boolean;

  setTasks: (tasks: BackendTask[]) => void;
  upsertTask: (task: BackendTask) => void;
  appendLog: (taskId: string, line: LogLine) => void;
  setLogs: (taskId: string, lines: LogLine[]) => void;
  openDrawer: (taskId?: string) => void;
  closeDrawer: () => void;
  selectTask: (taskId: string | null) => void;
}

function countRunning(tasks: BackendTask[]): number {
  return tasks.filter(
    (t) => t.status === "running" || t.status === "cancelling" || t.status === "queued"
  ).length;
}

function applyBackendEvent(state: TaskState, event: BackendEvent): TaskState {
  const { taskId, eventType } = event;

  switch (eventType) {
    case "task_updated":
    case "task_completed":
    case "task_failed":
    case "task_cancelled": {
      if (!taskId) return state;
      const task = event.payload as BackendTask;
      const existingIdx = state.tasks.findIndex((t) => t.id === taskId);
      const tasks =
        existingIdx >= 0
          ? state.tasks.map((t, i) => (i === existingIdx ? preferFreshTask(t, task) : t))
          : [...state.tasks, task];
      return { ...state, tasks, runningCount: countRunning(tasks) };
    }
    case "task_log": {
      if (!taskId) return state;
      const line = event.payload as LogLine;
      const existing = state.logs[taskId] || [];
      return {
        ...state,
        logs: { ...state.logs, [taskId]: [...existing, line] },
      };
    }
    case "confirmation_requested": {
      if (!taskId) return state;
      const task = event.payload as BackendTask;
      const existingIdx = state.tasks.findIndex((t) => t.id === taskId);
      const tasks =
        existingIdx >= 0
          ? state.tasks.map((t, i) => (i === existingIdx ? preferFreshTask(t, task) : t))
          : [...state.tasks, task];
      return { ...state, tasks, runningCount: countRunning(tasks) };
    }
    default:
      return state;
  }
}

export const useTaskStore = create<TaskState>((set, get) => ({
  tasks: [],
  logs: {},
  drawerOpen: false,
  selectedTaskId: null,
  runningCount: 0,
  tasksHydrated: false,

  setTasks: (tasks) =>
    set((state) => {
      const mergedTasks = mergeTaskSnapshots(state.tasks, tasks);
      return { tasks: mergedTasks, runningCount: countRunning(mergedTasks) };
    }),
  upsertTask: (task) =>
    set((state) => {
      const idx = state.tasks.findIndex((t) => t.id === task.id);
      const tasks =
        idx >= 0
          ? state.tasks.map((t, i) => (i === idx ? preferFreshTask(t, task) : t))
          : [...state.tasks, task];
      return { tasks, runningCount: countRunning(tasks) };
    }),
  appendLog: (taskId, line) =>
    set((state) => {
      const existing = state.logs[taskId] || [];
      return {
        logs: { ...state.logs, [taskId]: [...existing, line] },
      };
    }),
  setLogs: (taskId, lines) =>
    set((state) => ({
      logs: { ...state.logs, [taskId]: lines },
    })),
  openDrawer: (taskId) =>
    set({ drawerOpen: true, selectedTaskId: taskId || get().selectedTaskId }),
  closeDrawer: () => set({ drawerOpen: false, selectedTaskId: null }),
  selectTask: (taskId) => set({ selectedTaskId: taskId }),
}));

export function handleTaskEvent(event: BackendEvent, activeProjectId: string | null = null): void {
  const payload = typeof event.payload === "object" && event.payload !== null ? event.payload as Partial<BackendTask> : null;
  const eventProjectId = event.projectId ?? payload?.projectId ?? null;
  if (activeProjectId && eventProjectId && eventProjectId !== activeProjectId) return;
  useTaskStore.setState((state) => applyBackendEvent(state, event));
}

function preferFreshTask(current: BackendTask, incoming: BackendTask): BackendTask {
  const currentUpdated = Date.parse(current.updatedAt);
  const incomingUpdated = Date.parse(incoming.updatedAt);
  if (!Number.isNaN(currentUpdated) && !Number.isNaN(incomingUpdated) && currentUpdated > incomingUpdated) return current;
  if (isTerminalStatus(current.status) && !isTerminalStatus(incoming.status)) return current;
  // Waiting for user confirmation is also a settled worker state. Preserve
  // it when a stale creation response still says queued; explicit user
  // confirmation may legitimately move the same task to running/cancelled.
  if (current.status === "waiting_for_confirmation" && incoming.status === "queued") return current;
  // Older task snapshots/events may not carry the optional grouping field.
  // Preserve it when the task is otherwise newer so a refresh cannot make a
  // visible batch disappear from the Import view.
  return incoming.batchId === undefined && current.batchId !== undefined
    ? { ...incoming, batchId: current.batchId }
    : incoming;
}

/**
 * IPC snapshots can be older than an event that arrived while the request was
 * in flight. Merge by id so a late list_tasks/set_active_project response
 * cannot make a completed task look queued or running again.
 */
function mergeTaskSnapshots(current: readonly BackendTask[], incoming: readonly BackendTask[]): BackendTask[] {
  // A fetch started before a task was created can legitimately return an
  // empty list. Keep active in-memory work until a later snapshot observes it;
  // terminal-only lists may still be cleared by an intentional cleanup.
  if (incoming.length === 0) {
    return current.some((task) => !isTerminalStatus(task.status)) ? [...current] : [];
  }
  const currentById = new Map(current.map((task) => [task.id, task]));
  const incomingIds = new Set(incoming.map((task) => task.id));
  return [
    ...incoming.map((task) => {
      const existing = currentById.get(task.id);
      return existing ? preferFreshTask(existing, task) : task;
    }),
    ...current.filter((task) => !incomingIds.has(task.id)),
  ];
}

const hasTauri = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export async function fetchTasks(): Promise<void> {
  if (!hasTauri()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  const tasks = await invoke<BackendTask[]>("list_tasks", {
    request: { statusFilter: null },
  });
  useTaskStore.getState().setTasks(tasks);
}

export async function cancelTaskRequest(taskId: string): Promise<void> {
  if (!hasTauri()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  const task = await invoke<BackendTask>("cancel_task", {
    request: { taskId },
  });
  useTaskStore.getState().upsertTask(task);
}

export async function fetchTaskLogs(taskId: string): Promise<void> {
  if (!hasTauri()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  const lines = await invoke<LogLine[]>("get_task_logs", {
    request: { taskId },
  });
  useTaskStore.getState().setLogs(taskId, lines);
}

export async function removeCompletedTasks(): Promise<void> {
  if (!hasTauri()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke<number>("remove_completed_tasks");
  await fetchTasks();
}

export async function recoverTasksForProject(projectId: string, rootPath: string): Promise<void> {
  if (!hasTauri()) return;
  useTaskStore.setState({ tasksHydrated: false });
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    const tasks = await invoke<BackendTask[]>("set_active_project", {
      request: { projectId, rootPath },
    });
    useTaskStore.getState().setTasks(tasks);
  } finally {
    // Unknown task cards are only dismissible after the project task registry
    // has had a chance to hydrate; otherwise a restart race can hide a live
    // batch before its task snapshot arrives.
    useTaskStore.setState({ tasksHydrated: true });
  }
}
