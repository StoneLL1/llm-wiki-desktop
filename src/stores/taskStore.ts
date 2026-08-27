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
import {
  isBackendTaskSnapshot,
  isProgressOnlyTaskSnapshot,
  taskSnapshotsEqual,
} from "../services/taskSnapshotSemantics";

export interface TaskState {
  activeProjectId: string | null;
  activeProjectRootPath: string | null;
  /** Canonical normalized facts, partitioned by the indexes below. */
  taskById: Record<string, BackendTask>;
  taskIdsByProject: Record<string, readonly string[]>;
  runningCountByProject: Record<string, number>;
  /** @deprecated Batch 3 compatibility alias for taskById. */
  taskFacts: Record<string, BackendTask>;
  /** @deprecated Active-project selector adapter; not a canonical writer. */
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
  taskById: {},
  taskIdsByProject: {},
  runningCountByProject: {},
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
    const replacedTasks = replaceTaskSnapshot(state.tasks, tasks);
    const normalized = mergeNormalizedTaskFacts(
      state.taskById,
      state.taskIdsByProject,
      state.runningCountByProject,
      replacedTasks,
    );
    const resolvedTasks = replacedTasks.map((task) => normalized.taskById[task.id] ?? task);
    const nextTasks = resolvedTasks.length === state.tasks.length
      && resolvedTasks.every((task, index) => task === state.tasks[index])
      ? state.tasks
      : resolvedTasks;
    const activeKey = taskProjectKey(state.activeProjectId);
    let taskIdsByProject = normalized.taskIdsByProject;
    const nextIds = nextTasks.map((task) => task.id);
    const currentIds = taskIdsByProject[activeKey] ?? [];
    if (currentIds.length !== nextIds.length || currentIds.some((id, index) => id !== nextIds[index])) {
      taskIdsByProject = { ...taskIdsByProject, [activeKey]: nextIds };
    }
    const nextRunningCount = countRunning(nextTasks);
    const runningCountByProject = normalized.runningCountByProject[activeKey] === nextRunningCount
      ? normalized.runningCountByProject
      : { ...normalized.runningCountByProject, [activeKey]: nextRunningCount };
    if (
      nextTasks === state.tasks
      && !normalized.changed
      && taskIdsByProject === state.taskIdsByProject
      && runningCountByProject === state.runningCountByProject
    ) return state;
    return {
      taskById: normalized.taskById,
      taskIdsByProject,
      runningCountByProject,
      taskFacts: normalized.taskById,
      tasks: nextTasks,
      runningCount: nextRunningCount,
    };
  }),
  recordTaskFact: (task) =>
    set((state) => {
      const normalized = mergeNormalizedTaskFacts(
        state.taskById,
        state.taskIdsByProject,
        state.runningCountByProject,
        [task],
      );
      if (!normalized.changed) return state;
      return {
        taskById: normalized.taskById,
        taskIdsByProject: normalized.taskIdsByProject,
        runningCountByProject: normalized.runningCountByProject,
        taskFacts: normalized.taskById,
      };
    }),
  upsertTask: (task) =>
    set((state) => {
      const normalized = mergeNormalizedTaskFacts(
        state.taskById,
        state.taskIdsByProject,
        state.runningCountByProject,
        [task],
      );
      const resolvedTask = normalized.taskById[task.id] ?? task;
      const idx = state.tasks.findIndex((t) => t.id === task.id);
      if (resolvedTask.projectId !== state.activeProjectId && idx < 0) {
        if (!normalized.changed) return state;
        return {
          taskById: normalized.taskById,
          taskIdsByProject: normalized.taskIdsByProject,
          runningCountByProject: normalized.runningCountByProject,
          taskFacts: normalized.taskById,
        };
      }
      const tasks =
        idx >= 0
          ? state.tasks.map((t, i) => (i === idx ? resolvedTask : t))
          : [...state.tasks, resolvedTask];
      const tasksChanged = idx < 0 || tasks[idx] !== state.tasks[idx];
      if (!normalized.changed && !tasksChanged) return state;
      const runningCount = countRunning(tasks);
      return {
        taskById: normalized.taskById,
        taskIdsByProject: normalized.taskIdsByProject,
        runningCountByProject: normalized.runningCountByProject,
        taskFacts: normalized.taskById,
        tasks: tasksChanged ? tasks : state.tasks,
        runningCount,
      };
    }),
  upsertTasks: (incoming) =>
    set((state) => {
      if (incoming.length === 0) return state;
      const normalized = mergeNormalizedTaskFacts(
        state.taskById,
        state.taskIdsByProject,
        state.runningCountByProject,
        incoming,
      );
      const incomingById = new Map(
        incoming
          .map((task) => normalized.taskById[task.id] ?? task)
          .filter((task) => task.projectId === state.activeProjectId)
          .map((task) => [task.id, task]),
      );
      const tasks = state.tasks.map((task) => {
        const next = incomingById.get(task.id);
        if (!next) return task;
        incomingById.delete(task.id);
        return next;
      });
      tasks.push(...incomingById.values());
      const tasksChanged = tasks.length !== state.tasks.length
        || tasks.some((task, index) => task !== state.tasks[index]);
      if (!normalized.changed && !tasksChanged) return state;
      return {
        taskById: normalized.taskById,
        taskIdsByProject: normalized.taskIdsByProject,
        runningCountByProject: normalized.runningCountByProject,
        taskFacts: normalized.taskById,
        tasks: tasksChanged ? tasks : state.tasks,
        runningCount: normalized.runningCountByProject[taskProjectKey(state.activeProjectId)] ?? 0,
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
  if (
    event.taskId
    && (event.eventType === "task_updated"
      || event.eventType === "task_completed"
      || event.eventType === "task_failed"
      || event.eventType === "task_cancelled"
      || event.eventType === "confirmation_requested")
    && isBackendTaskSnapshot(event.payload)
  ) {
    useTaskStore.getState().upsertTask(event.payload);
    return;
  }
  useTaskStore.setState((state) => {
    if (!state.activeProjectId || event.projectId !== state.activeProjectId) return state;
    return applyBackendEvent(state, event);
  });
}

export function mergeTaskSnapshot(
  current: BackendTask | undefined,
  incoming: BackendTask,
): { task: BackendTask; changed: boolean } {
  if (!current) return { task: incoming, changed: true };
  const currentUpdated = Date.parse(current.updatedAt);
  const incomingUpdated = Date.parse(incoming.updatedAt);
  if (!Number.isNaN(currentUpdated) && !Number.isNaN(incomingUpdated) && currentUpdated > incomingUpdated) {
    return { task: current, changed: false };
  }
  if (isTerminalStatus(current.status) && !isTerminalStatus(incoming.status)) {
    return { task: current, changed: false };
  }
  // Waiting for user confirmation is also a settled worker state. Preserve
  // it when a stale creation response still says queued; explicit user
  // confirmation may legitimately move the same task to running/cancelled.
  if (current.status === "waiting_for_confirmation" && incoming.status === "queued") {
    return { task: current, changed: false };
  }
  // Older task snapshots/events may not carry the optional grouping field.
  // Preserve it when the task is otherwise newer so a refresh cannot make a
  // visible batch disappear from the Import view.
  const withBatch = incoming.batchId === undefined && current.batchId !== undefined
    ? { ...incoming, batchId: current.batchId }
    : incoming;
  const candidate = withBatch.operation === undefined && current.operation !== undefined
    ? { ...withBatch, operation: current.operation }
    : withBatch;
  return taskSnapshotsEqual(current, candidate)
    ? { task: current, changed: false }
    : { task: candidate, changed: true };
}

function preferFreshTask(current: BackendTask, incoming: BackendTask): BackendTask {
  return mergeTaskSnapshot(current, incoming).task;
}

const GLOBAL_TASK_PROJECT = "__global__";

function taskProjectKey(projectId: string | null): string {
  return projectId ?? GLOBAL_TASK_PROJECT;
}

function isRunningTask(task: BackendTask | undefined): boolean {
  return task !== undefined
    && (task.status === "running" || task.status === "cancelling" || task.status === "queued");
}

interface NormalizedTaskMerge {
  taskById: Record<string, BackendTask>;
  taskIdsByProject: Record<string, readonly string[]>;
  runningCountByProject: Record<string, number>;
  changed: boolean;
}

function mergeNormalizedTaskFacts(
  currentById: Readonly<Record<string, BackendTask>>,
  currentIdsByProject: Readonly<Record<string, readonly string[]>>,
  currentRunningByProject: Readonly<Record<string, number>>,
  incoming: readonly BackendTask[],
): NormalizedTaskMerge {
  let taskById = currentById as Record<string, BackendTask>;
  let taskIdsByProject = currentIdsByProject as Record<string, readonly string[]>;
  let runningCountByProject = currentRunningByProject as Record<string, number>;
  let changed = false;

  for (const task of incoming) {
    const previous = taskById[task.id];
    const merged = mergeTaskSnapshot(previous, task);
    if (!merged.changed) continue;
    if (!changed) taskById = { ...taskById };
    taskById[task.id] = merged.task;
    changed = true;

    const previousKey = previous ? taskProjectKey(previous.projectId) : null;
    const nextKey = taskProjectKey(merged.task.projectId);
    if (previousKey !== nextKey) {
      if (taskIdsByProject === currentIdsByProject) taskIdsByProject = { ...taskIdsByProject };
      if (previousKey) {
        taskIdsByProject[previousKey] = (taskIdsByProject[previousKey] ?? []).filter((id) => id !== task.id);
      }
      const nextIds = taskIdsByProject[nextKey] ?? [];
      if (!nextIds.includes(task.id)) taskIdsByProject[nextKey] = [...nextIds, task.id];
    } else if (!previous) {
      if (taskIdsByProject === currentIdsByProject) taskIdsByProject = { ...taskIdsByProject };
      taskIdsByProject[nextKey] = [...(taskIdsByProject[nextKey] ?? []), task.id];
    } else if (!isProgressOnlyTaskSnapshot(previous, merged.task)) {
      const currentIds = taskIdsByProject[nextKey] ?? [];
      if (taskIdsByProject === currentIdsByProject) taskIdsByProject = { ...taskIdsByProject };
      taskIdsByProject[nextKey] = currentIds.at(-1) === task.id
        ? [...currentIds]
        : [...currentIds.filter((id) => id !== task.id), task.id];
    }

    const previousRunning = isRunningTask(previous);
    const nextRunning = isRunningTask(merged.task);
    const runningContributionChanged = previousKey === nextKey
      ? previousRunning !== nextRunning
      : previousRunning || nextRunning;
    if (runningContributionChanged) {
      if (runningCountByProject === currentRunningByProject) {
        runningCountByProject = { ...runningCountByProject };
      }
      if (previousKey && previousRunning) {
        runningCountByProject[previousKey] = Math.max(0, (runningCountByProject[previousKey] ?? 0) - 1);
      }
      if (nextRunning) {
        runningCountByProject[nextKey] = (runningCountByProject[nextKey] ?? 0) + 1;
      }
    }
  }

  return { taskById, taskIdsByProject, runningCountByProject, changed };
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
    return current.some((task) => !isTerminalStatus(task.status)) ? current as BackendTask[] : [];
  }
  const currentById = new Map(current.map((task) => [task.id, task]));
  const incomingIds = new Set(incoming.map((task) => task.id));
  const merged = [
    ...incoming.map((task) => {
      const existing = currentById.get(task.id);
      return existing ? preferFreshTask(existing, task) : task;
    }),
    ...current.filter((task) => !incomingIds.has(task.id)),
  ];
  return merged.length === current.length && merged.every((task, index) => task === current[index])
    ? current as BackendTask[]
    : merged;
}

function replaceTaskSnapshot(current: readonly BackendTask[], incoming: readonly BackendTask[]): BackendTask[] {
  if (incoming.length === 0 && current.some((task) => !isTerminalStatus(task.status))) {
    return current as BackendTask[];
  }
  const currentById = new Map(current.map((task) => [task.id, task]));
  const replaced = incoming.map((task) => {
    const existing = currentById.get(task.id);
    return existing ? preferFreshTask(existing, task) : task;
  });
  return replaced.length === current.length && replaced.every((task, index) => task === current[index])
    ? current as BackendTask[]
    : replaced;
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
  useTaskStore.getState().setTasks(mergedTasks);
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

export function selectTaskById(state: TaskState, taskId: string | null | undefined): BackendTask | null {
  if (!taskId) return null;
  const legacyTask = state.tasks.find((task) => task.id === taskId);
  const normalizedTask = state.taskById[taskId];
  if (legacyTask && normalizedTask && legacyTask !== normalizedTask
    && !taskSnapshotsEqual(legacyTask, normalizedTask)) {
    return legacyTask;
  }
  return normalizedTask ?? legacyTask ?? null;
}

export function selectProjectTaskById(
  state: TaskState,
  projectId: string | null,
  taskId: string | null | undefined,
): BackendTask | null {
  const task = selectTaskById(state, taskId);
  return task?.projectId === projectId ? task : null;
}

export function selectTasksForProject(state: TaskState, projectId: string | null): readonly BackendTask[] {
  if (state.activeProjectId === projectId) return state.tasks;
  const indexed = (state.taskIdsByProject[taskProjectKey(projectId)] ?? [])
    .map((taskId) => state.taskById[taskId])
    .filter((task): task is BackendTask => task !== undefined);
  if (indexed.length > 0 || state.tasks.length === 0) return indexed;
  return state.tasks.filter((task) => task.projectId === projectId);
}

export function selectTaskIdsForProject(state: TaskState, projectId: string | null): readonly string[] {
  const indexed = state.taskIdsByProject[taskProjectKey(projectId)] ?? EMPTY_TASK_IDS;
  const legacyTasks = state.activeProjectId === projectId
    ? state.tasks
    : indexed.length === 0
      ? state.tasks.filter((task) => task.projectId === projectId)
      : [];
  const legacyTasksMatchIndex = indexed.length === legacyTasks.length
    && indexed.every((taskId, index) => taskId === legacyTasks[index]?.id);
  if (legacyTasksMatchIndex) return indexed;
  const projectKey = taskProjectKey(projectId);
  const cachedByProject = legacyTaskIdsCache.get(state.tasks);
  const cached = cachedByProject?.get(projectKey);
  if (cached) return cached;
  const taskIds = legacyTasks.map((task) => task.id);
  const nextCache = cachedByProject ?? new Map<string, readonly string[]>();
  nextCache.set(projectKey, taskIds);
  if (!cachedByProject) legacyTaskIdsCache.set(state.tasks, nextCache);
  return taskIds;
}

export function selectRunningCountForProject(state: TaskState, projectId: string | null): number {
  if (state.activeProjectId === projectId) return state.runningCount;
  return state.runningCountByProject[taskProjectKey(projectId)] ?? 0;
}

const EMPTY_TASK_IDS: readonly string[] = [];
const legacyTaskIdsCache = new WeakMap<readonly BackendTask[], Map<string, readonly string[]>>();
