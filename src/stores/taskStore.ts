import { create } from "zustand";
import type {
  BackendTask,
  BackendEvent,
  LogLine,
} from "../types/task";

interface TaskState {
  tasks: BackendTask[];
  logs: Record<string, LogLine[]>;
  drawerOpen: boolean;
  selectedTaskId: string | null;
  runningCount: number;

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
          ? state.tasks.map((t, i) => (i === existingIdx ? task : t))
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
          ? state.tasks.map((t, i) => (i === existingIdx ? task : t))
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

  setTasks: (tasks) => set({ tasks, runningCount: countRunning(tasks) }),
  upsertTask: (task) =>
    set((state) => {
      const idx = state.tasks.findIndex((t) => t.id === task.id);
      const tasks =
        idx >= 0
          ? state.tasks.map((t, i) => (i === idx ? task : t))
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

export function handleTaskEvent(event: BackendEvent): void {
  useTaskStore.setState((state) => applyBackendEvent(state, event));
}

const hasTauri = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export async function fetchTasks(): Promise<void> {
  if (!hasTauri()) return;
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    const tasks = await invoke<BackendTask[]>("list_tasks", {
      request: { statusFilter: null },
    });
    useTaskStore.getState().setTasks(tasks);
  } catch {
    // Backend not available (e.g. running in browser-only dev)
  }
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
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    const lines = await invoke<LogLine[]>("get_task_logs", {
      request: { taskId },
    });
    useTaskStore.getState().setLogs(taskId, lines);
  } catch {
    // Backend not available
  }
}

export async function removeCompletedTasks(): Promise<void> {
  if (!hasTauri()) return;
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke<number>("remove_completed_tasks");
    await fetchTasks();
  } catch {
    // Backend not available
  }
}

export async function recoverTasksForProject(projectId: string, rootPath: string): Promise<void> {
  if (!hasTauri()) return;
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    const tasks = await invoke<BackendTask[]>("set_active_project", {
      request: { projectId, rootPath },
    });
    useTaskStore.getState().setTasks(tasks);
  } catch {
    // Backend not available (e.g. browser-only dev)
  }
}
