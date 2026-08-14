import { create } from "zustand";
import { invoke as invokeCommand } from "@tauri-apps/api/core";
import type {
  BackendTask,
  BackendEvent,
  TaskActivity,
  StreamDelta,
  LogLine,
  SetActiveProjectResult,
  TaskProjectPersistenceReason,
} from "../types/task";
import type { WorkflowPersistenceMode } from "../types/workflow";
import { isTerminalStatus } from "../types/task";
import { useProjectStore } from "./projectStore";

interface TaskState {
  activeProjectId: string | null;
  activeProjectRootPath: string | null;
  /** Global task facts received from authoritative backend responses. */
  taskFacts: Record<string, BackendTask>;
  tasks: BackendTask[];
  logs: Record<string, LogLine[]>;
  activities: Record<string, TaskActivity[]>;
  taskOutputs: Record<string, string>;
  drawerOpen: boolean;
  selectedTaskId: string | null;
  runningCount: number;
  tasksHydrated: boolean;
  projectPersistence: WorkflowPersistenceMode | null;
  projectPersistenceReason: TaskProjectPersistenceReason | null;

  setTasks: (tasks: BackendTask[]) => void;
  recordTaskFact: (task: BackendTask) => void;
  upsertTask: (task: BackendTask) => void;
  upsertTasks: (tasks: readonly BackendTask[]) => void;
  appendLog: (taskId: string, line: LogLine) => void;
  setLogs: (taskId: string, lines: LogLine[]) => void;
  appendActivity: (taskId: string, activity: TaskActivity) => void;
  setActivities: (taskId: string, activities: TaskActivity[]) => void;
  appendTaskOutput: (taskId: string, delta: string) => void;
  openDrawer: (taskId?: string) => void;
  closeDrawer: () => void;
  selectTask: (taskId: string | null) => void;
}

function countRunning(tasks: BackendTask[]): number {
  return tasks.filter(
    (t) => t.status === "running" || t.status === "cancelling" || t.status === "queued"
  ).length;
}

function activityKey(activity: TaskActivity): string {
  return JSON.stringify(activity);
}

function mergeActivities(current: readonly TaskActivity[], incoming: readonly TaskActivity[]): TaskActivity[] {
  const startsWith = (prefix: readonly TaskActivity[], full: readonly TaskActivity[]) =>
    prefix.every((activity, index) => activityKey(activity) === activityKey(full[index]));
  // TaskService persists activities append-only. Prefer whichever snapshot is
  // the longer prefix-compatible view so two legitimate identical events are
  // not collapsed merely because their payloads happen to match.
  if (current.length <= incoming.length && startsWith(current, incoming)) return [...incoming];
  if (incoming.length <= current.length && startsWith(incoming, current)) return [...current];
  return [...incoming, ...current];
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
    case "task_activity": {
      if (!taskId) return state;
      const activity = event.payload as TaskActivity;
      const existing = state.activities[taskId] || [];
      return {
        ...state,
        activities: { ...state.activities, [taskId]: [...existing, activity] },
      };
    }
    case "task_stream_output": {
      if (!taskId) return state;
      const delta = event.payload as StreamDelta;
      if (!delta || typeof delta.delta !== "string") return state;
      return {
        ...state,
        taskOutputs: {
          ...state.taskOutputs,
          [taskId]: `${state.taskOutputs[taskId] ?? ""}${delta.delta}`.slice(-512 * 1024),
        },
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
  activeProjectId: null,
  activeProjectRootPath: null,
  taskFacts: {},
  tasks: [],
  logs: {},
  activities: {},
  taskOutputs: {},
  drawerOpen: false,
  selectedTaskId: null,
  runningCount: 0,
  tasksHydrated: false,
  projectPersistence: null,
  projectPersistenceReason: null,

  // Explicit recovery/project snapshots replace the visible task set while
  // preserving newer terminal state for task ids present in both snapshots.
  // fetchTasks applies mergeTaskSnapshots below when it needs race tolerance
  // for a list request that started before task creation.
  setTasks: (tasks) => set((state) => {
    const nextTasks = replaceTaskSnapshot(state.tasks, tasks);
    return {
      taskFacts: mergeTaskFacts(state.taskFacts, tasks),
      tasks: nextTasks,
      runningCount: countRunning(nextTasks),
    };
  }),
  recordTaskFact: (task) =>
    set((state) => ({ taskFacts: mergeTaskFacts(state.taskFacts, [task]) })),
  upsertTask: (task) =>
    set((state) => {
      const idx = state.tasks.findIndex((t) => t.id === task.id);
      const tasks =
        idx >= 0
          ? state.tasks.map((t, i) => (i === idx ? preferFreshTask(t, task) : t))
          : [...state.tasks, task];
      return {
        taskFacts: mergeTaskFacts(state.taskFacts, [task]),
        tasks,
        runningCount: countRunning(tasks),
      };
    }),
  upsertTasks: (incoming) =>
    set((state) => {
      if (incoming.length === 0) return state;
      const incomingById = new Map(incoming.map((task) => [task.id, task]));
      const tasks = state.tasks.map((task) => {
        const next = incomingById.get(task.id);
        if (!next) return task;
        incomingById.delete(task.id);
        return preferFreshTask(task, next);
      });
      tasks.push(...incomingById.values());
      return {
        taskFacts: mergeTaskFacts(state.taskFacts, incoming),
        tasks,
        runningCount: countRunning(tasks),
      };
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
  appendActivity: (taskId, activity) =>
    set((state) => {
      const existing = state.activities[taskId] || [];
      return {
        activities: {
          ...state.activities,
          [taskId]: [...existing, activity],
        },
      };
    }),
  setActivities: (taskId, activities) =>
    set((state) => ({
      activities: { ...state.activities, [taskId]: mergeActivities(state.activities[taskId] || [], activities) },
    })),
  appendTaskOutput: (taskId, delta) =>
    set((state) => ({
      taskOutputs: {
        ...state.taskOutputs,
        [taskId]: `${state.taskOutputs[taskId] ?? ""}${delta}`.slice(-512 * 1024),
      },
    })),
  openDrawer: (taskId) =>
    set({ drawerOpen: true, selectedTaskId: taskId || get().selectedTaskId }),
  closeDrawer: () => set({ drawerOpen: false, selectedTaskId: null }),
  selectTask: (taskId) => set({ selectedTaskId: taskId }),
}));

export function handleTaskEvent(event: BackendEvent): void {
  useTaskStore.setState((state) => {
    if (!state.activeProjectId || event.projectId !== state.activeProjectId) return state;
    return applyBackendEvent(state, event);
  });
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
  const withBatch = incoming.batchId === undefined && current.batchId !== undefined
    ? { ...incoming, batchId: current.batchId }
    : incoming;
  return withBatch.operation === undefined && current.operation !== undefined
    ? { ...withBatch, operation: current.operation }
    : withBatch;
}

function mergeTaskFacts(
  current: Readonly<Record<string, BackendTask>>,
  incoming: readonly BackendTask[],
): Record<string, BackendTask> {
  const next = { ...current };
  for (const task of incoming) {
    next[task.id] = next[task.id] ? preferFreshTask(next[task.id], task) : task;
  }
  return next;
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

function replaceTaskSnapshot(current: readonly BackendTask[], incoming: readonly BackendTask[]): BackendTask[] {
  if (incoming.length === 0 && current.some((task) => !isTerminalStatus(task.status))) {
    return [...current];
  }
  const currentById = new Map(current.map((task) => [task.id, task]));
  return incoming.map((task) => {
    const existing = currentById.get(task.id);
    return existing ? preferFreshTask(existing, task) : task;
  });
}

const hasTauri = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export async function fetchTasks(projectId: string, rootPath: string): Promise<void> {
  if (!hasTauri()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  const tasks = await invoke<BackendTask[]>("list_tasks", {
    request: { projectId, projectRootPath: rootPath, statusFilter: null },
  });
  const state = useTaskStore.getState();
  if (state.activeProjectId !== projectId || state.activeProjectRootPath !== rootPath) return;
  const current = state.tasks;
  const mergedTasks = mergeTaskSnapshots(current, tasks);
  useTaskStore.setState({ tasks: mergedTasks, runningCount: countRunning(mergedTasks) });
}

export async function cancelTaskRequest(taskId: string): Promise<void> {
  if (!hasTauri()) return;
  const { activeProjectId: projectId, activeProjectRootPath: projectRootPath } = useTaskStore.getState();
  if (!projectId || !projectRootPath) return;
  const task = await invokeCommand<BackendTask>("cancel_task", {
    request: { taskId, projectId, projectRootPath },
  });
  const current = useTaskStore.getState();
  if (current.activeProjectId !== projectId || current.activeProjectRootPath !== projectRootPath) return;
  useTaskStore.getState().upsertTask(task);
}

export async function fetchTaskById(taskId: string): Promise<BackendTask | null> {
  if (!hasTauri()) return null;
  const { activeProjectId: projectId, activeProjectRootPath: projectRootPath } = useTaskStore.getState();
  if (!projectId || !projectRootPath) return null;
  const { invoke } = await import("@tauri-apps/api/core");
  const task = await invoke<BackendTask | null>("get_task", {
    request: { taskId, projectId, projectRootPath },
  });
  const current = useTaskStore.getState();
  if (current.activeProjectId !== projectId || current.activeProjectRootPath !== projectRootPath) return null;
  if (task) current.upsertTask(task);
  return task;
}

export async function fetchTaskLogs(taskId: string): Promise<void> {
  if (!hasTauri()) return;
  const { activeProjectId: projectId, activeProjectRootPath: projectRootPath } = useTaskStore.getState();
  if (!projectId || !projectRootPath) return;
  const { invoke } = await import("@tauri-apps/api/core");
  const lines = await invoke<LogLine[]>("get_task_logs", {
    request: { taskId, projectId, projectRootPath },
  });
  const current = useTaskStore.getState();
  if (current.activeProjectId !== projectId || current.activeProjectRootPath !== projectRootPath) return;
  useTaskStore.getState().setLogs(taskId, lines);
}

export async function fetchTaskActivities(taskId: string): Promise<void> {
  if (!hasTauri()) return;
  const { activeProjectId: projectId, activeProjectRootPath: projectRootPath } = useTaskStore.getState();
  if (!projectId || !projectRootPath) return;
  const { invoke } = await import("@tauri-apps/api/core");
  const activities = await invoke<TaskActivity[]>("get_task_activities", {
    request: { taskId, projectId, projectRootPath },
  });
  const current = useTaskStore.getState();
  if (current.activeProjectId !== projectId || current.activeProjectRootPath !== projectRootPath) return;
  useTaskStore.getState().setActivities(taskId, Array.isArray(activities) ? activities : []);
}

export async function removeCompletedTasks(): Promise<void> {
  if (!hasTauri()) return;
  const { activeProjectId, activeProjectRootPath } = useTaskStore.getState();
  if (!activeProjectId || !activeProjectRootPath) return;
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke<number>("remove_completed_tasks", {
    request: { projectId: activeProjectId, projectRootPath: activeProjectRootPath },
  });
  await fetchTasks(activeProjectId, activeProjectRootPath);
}

export async function recoverTasksForProject(projectId: string, rootPath: string): Promise<void> {
  if (!hasTauri()) return;
  const recoveryId = ++recoveryEpoch;
  useTaskStore.setState({
    activeProjectId: projectId,
    activeProjectRootPath: rootPath,
    tasks: [],
    logs: {},
    activities: {},
    taskOutputs: {},
    selectedTaskId: null,
    drawerOpen: false,
    runningCount: 0,
    tasksHydrated: false,
    projectPersistence: null,
    projectPersistenceReason: null,
  });
  useProjectStore.getState().setTaskPersistence(projectId, rootPath, null, null);
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    const result = await invoke<SetActiveProjectResult>("set_active_project", {
      request: { projectId, rootPath },
    });
    const state = useTaskStore.getState();
    if (
      recoveryId !== recoveryEpoch ||
      state.activeProjectId !== projectId ||
      state.activeProjectRootPath !== rootPath
    ) return;
    useTaskStore.setState({
      projectPersistence: result.persistence,
      projectPersistenceReason: result.persistenceReason ?? null,
    });
    useProjectStore.getState().setTaskPersistence(
      projectId,
      rootPath,
      result.persistence,
      result.persistenceReason ?? null,
    );
    useTaskStore.getState().setTasks(result.tasks);
  } finally {
    // Unknown task cards are only dismissible after the project task registry
    // has had a chance to hydrate; otherwise a restart race can hide a live
    // batch before its task snapshot arrives.
    const state = useTaskStore.getState();
    if (
      recoveryId === recoveryEpoch &&
      state.activeProjectId === projectId &&
      state.activeProjectRootPath === rootPath
    ) {
      useTaskStore.setState({ tasksHydrated: true });
    }
  }
}

let recoveryEpoch = 0;
