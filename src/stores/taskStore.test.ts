import { beforeEach, describe, expect, it, vi } from "vitest";

import type { BackendTask } from "../types/task";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

import {
  fetchTaskById,
  fetchTasks,
  handleTaskEvent,
  selectProjectTaskById,
  selectTaskIdsForProject,
  useTaskStore,
} from "./taskStore";
import { recoverAppTasks, recoverTasksForProject } from "../services/taskRecovery";
import { defaultProject, useProjectStore } from "./projectStore";

function task(id: string, projectId: string): BackendTask {
  return {
    id,
    taskType: "import",
    projectId,
    title: id,
    status: "running",
    progress: null,
    startedAt: "2026-06-21T00:00:00Z",
    updatedAt: "2026-06-21T00:00:00Z",
    completedAt: null,
    cancellable: true,
    logPath: null,
    result: null,
    error: null,
  };
}

function succeededTask(id: string, projectId: string): BackendTask {
  return {
    ...task(id, projectId),
    status: "succeeded",
    updatedAt: "2026-06-21T00:01:00Z",
    completedAt: "2026-06-21T00:01:00Z",
    cancellable: false,
  };
}

function globalCapabilityTask(id: string): BackendTask {
  return {
    ...task(id, "project-a"),
    taskType: "capability_install",
    projectId: null,
    operation: {
      kind: "app_capability_install",
      capabilityId: "ocr-cjk-accurate",
      version: "1.0.0",
      targetTriple: "x86_64-pc-windows-msvc",
      archiveIdentity: "archive-a",
    },
  };
}

beforeEach(() => {
  invokeMock.mockReset();
  useTaskStore.setState({
    activeProjectId: "project-a",
    activeProjectRootPath: "D:/project-a",
    taskById: {},
    taskIdsByProject: {},
    runningCountByProject: {},
    taskFacts: {},
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
  useProjectStore.setState({
    currentProject: {
      ...defaultProject,
      projectId: "project-a",
      rootPath: "D:/project-a",
    },
    taskPersistence: null,
    taskPersistenceReason: null,
  });
  Object.defineProperty(window, "__TAURI_INTERNALS__", { value: {}, configurable: true });
});

describe("recoverTasksForProject", () => {
  it("sends explicit project context for task-id reads", async () => {
    const current = task("task-a", "project-a");
    invokeMock.mockResolvedValueOnce(current);

    await expect(fetchTaskById("task-a")).resolves.toEqual(current);

    expect(invokeMock).toHaveBeenCalledWith("get_task", {
      request: {
        taskId: "task-a",
        projectId: "project-a",
        projectRootPath: "D:/project-a",
      },
    });
  });
  it("does not let a late queued snapshot overwrite a terminal event", () => {
    useTaskStore.getState().setTasks([succeededTask("task-a", "project-a")]);

    handleTaskEvent({
      eventId: "event-1",
      eventType: "task_completed",
      projectId: "project-a",
      taskId: "task-a",
      timestamp: "2026-06-21T00:01:00Z",
      payload: succeededTask("task-a", "project-a"),
    });
    useTaskStore.getState().setTasks([task("task-a", "project-a")]);

    expect(useTaskStore.getState().tasks[0]).toMatchObject({
      id: "task-a",
      status: "succeeded",
    });
  });

  it("keeps active tasks when a stale empty snapshot arrives", () => {
    useTaskStore.getState().setTasks([task("task-a", "project-a")]);

    useTaskStore.getState().setTasks([]);

    expect(useTaskStore.getState().tasks).toHaveLength(1);
    expect(useTaskStore.getState().tasks[0].status).toBe("running");
  });

  it("preserves a batch identity when a newer legacy snapshot omits it", () => {
    useTaskStore.getState().setTasks([{
      ...task("task-a", "project-a"),
      batchId: "batch-a",
      operation: {
        kind: "import_batch",
        sessionId: "session-a",
        itemCount: 1,
      },
    }]);

    useTaskStore.getState().setTasks([{
      ...task("task-a", "project-a"),
      status: "succeeded",
      updatedAt: "2026-06-21T00:01:00Z",
      completedAt: "2026-06-21T00:01:00Z",
      cancellable: false,
    }]);

    expect(useTaskStore.getState().tasks[0]).toMatchObject({
      status: "succeeded",
      batchId: "batch-a",
      operation: {
        kind: "import_batch",
        sessionId: "session-a",
        itemCount: 1,
      },
    });
  });

  it("replaces the visible task list with only the selected project snapshot", async () => {
    const tasks = [task("task-b", "project-b")];
    useProjectStore.setState({
      currentProject: {
        ...defaultProject,
        projectId: "project-b",
        rootPath: "D:/project-b",
      },
    });
    invokeMock.mockResolvedValueOnce({
      tasks,
      persistence: "persistent",
    });

    await recoverTasksForProject("project-b", "D:/project-b");

    expect(invokeMock).toHaveBeenCalledWith("set_active_project", {
      request: { projectId: "project-b", rootPath: "D:/project-b" },
    });
    expect(useTaskStore.getState().tasks).toEqual(tasks);
    expect(useTaskStore.getState().selectedTaskId).toBeNull();
    expect(useTaskStore.getState().projectPersistence).toBe("persistent");
    expect(useTaskStore.getState().projectPersistenceReason).toBeNull();
    expect(useProjectStore.getState().taskPersistence).toBe("persistent");
  });

  it("keeps a typed memory-only reason without inventing recovered tasks", async () => {
    useProjectStore.setState({
      currentProject: {
        ...defaultProject,
        projectId: "project-b",
        rootPath: "D:/project-b",
      },
    });
    invokeMock.mockResolvedValueOnce({
      tasks: [],
      persistence: "memory_only",
      persistenceReason: "project_untrusted",
    });

    await recoverTasksForProject("project-b", "D:/project-b");

    expect(useTaskStore.getState().tasks).toEqual([]);
    expect(useTaskStore.getState().projectPersistence).toBe("memory_only");
    expect(useTaskStore.getState().projectPersistenceReason).toBe("project_untrusted");
    expect(useProjectStore.getState().taskPersistenceReason).toBe("project_untrusted");
  });

  it("ignores task events from background projects", () => {
    handleTaskEvent({
      eventId: "event-project-b",
      eventType: "task_updated",
      projectId: "project-b",
      taskId: "task-b",
      timestamp: "2026-06-21T00:00:30Z",
      payload: task("task-b", "project-b"),
    });

    expect(useTaskStore.getState().tasks).toEqual([]);
  });

  it("upserts a task cohort with one store publication", () => {
    let publications = 0;
    const unsubscribe = useTaskStore.subscribe(() => { publications += 1; });
    useTaskStore.getState().upsertTasks(Array.from({ length: 100 }, (_, index) => ({
      ...task(`task-${index}`, "project-a"),
      updatedAt: `2026-06-21T00:00:${index % 60}Z`,
    })));
    unsubscribe();
    expect(publications).toBe(1);
    expect(useTaskStore.getState().tasks).toHaveLength(100);
  });

  it("shows app-global tasks with the selected project and excludes other projects", async () => {
    useTaskStore.getState().recordTaskFact(task("task-a", "project-a"));
    invokeMock
      .mockResolvedValueOnce({
        tasks: [task("task-b", "project-b")],
        persistence: "persistent",
      })
      .mockResolvedValueOnce([globalCapabilityTask("global-install")]);

    await recoverTasksForProject("project-b", "D:/project-b");

    expect(useTaskStore.getState().tasks.map((item) => item.id)).toEqual([
      "global-install",
      "task-b",
    ]);
    expect(selectTaskIdsForProject(useTaskStore.getState(), "project-b")).toEqual(["task-b"]);
    expect(selectTaskIdsForProject(useTaskStore.getState(), "project-a")).toEqual(["task-a"]);
  });

  it("does not let a late global-task snapshot overwrite a newer project recovery", async () => {
    let resolveFirstGlobal!: (tasks: BackendTask[]) => void;
    const firstGlobal = new Promise<BackendTask[]>((resolve) => {
      resolveFirstGlobal = resolve;
    });
    let globalCalls = 0;
    invokeMock.mockImplementation((command: string, args?: { request?: { projectId?: string } }) => {
      if (command === "set_active_project") {
        const projectId = args?.request?.projectId ?? "unknown";
        return Promise.resolve({
          tasks: [task(`task-${projectId}`, projectId)],
          persistence: "persistent",
        });
      }
      if (command === "list_app_tasks_v1") {
        globalCalls += 1;
        return globalCalls === 1
          ? firstGlobal
          : Promise.resolve([globalCapabilityTask("global-current")]);
      }
      return Promise.resolve(undefined);
    });

    const firstRecovery = recoverTasksForProject("project-a", "D:/project-a");
    await vi.waitFor(() => expect(globalCalls).toBe(1));
    await recoverTasksForProject("project-b", "D:/project-b");
    resolveFirstGlobal([globalCapabilityTask("global-stale")]);
    await firstRecovery;

    expect(useTaskStore.getState().activeProjectId).toBe("project-b");
    expect(useTaskStore.getState().tasks.map((item) => item.id)).toEqual([
      "global-current",
      "task-project-b",
    ]);
  });

  it("does not let no-project app recovery overwrite a newly selected project", async () => {
    let resolveAppTasks!: (tasks: BackendTask[]) => void;
    const appTasks = new Promise<BackendTask[]>((resolve) => {
      resolveAppTasks = resolve;
    });
    useTaskStore.setState({ activeProjectId: null, activeProjectRootPath: null, tasks: [] });
    invokeMock
      .mockReturnValueOnce(appTasks)
      .mockResolvedValueOnce({
        tasks: [task("task-project-b", "project-b")],
        persistence: "persistent",
      })
      .mockResolvedValueOnce([globalCapabilityTask("global-current")]);

    const appRecovery = recoverAppTasks();
    await vi.waitFor(() => expect(invokeMock).toHaveBeenCalledWith("list_app_tasks_v1"));
    await recoverTasksForProject("project-b", "D:/project-b");
    resolveAppTasks([globalCapabilityTask("global-stale")]);
    await appRecovery;

    expect(useTaskStore.getState().tasks.map((item) => item.id)).toEqual([
      "global-current",
      "task-project-b",
    ]);
  });

  it("drops the previous project's tasks when entering no-project recovery", async () => {
    useProjectStore.setState({ currentProject: defaultProject });
    useTaskStore.setState({
      activeProjectId: "project-a",
      activeProjectRootPath: "D:/project-a",
      tasks: [task("task-project-a", "project-a")],
    });
    invokeMock.mockResolvedValueOnce([globalCapabilityTask("global-current")]);

    await recoverAppTasks();

    expect(useTaskStore.getState()).toMatchObject({
      activeProjectId: null,
      activeProjectRootPath: null,
      tasksHydrated: true,
    });
    expect(useTaskStore.getState().tasks.map((item) => item.id)).toEqual(["global-current"]);
  });

  it("does not publish a semantically identical task snapshot twice", () => {
    const current = task("task-a", "project-a");
    useTaskStore.getState().upsertTask(current);
    let publications = 0;
    const unsubscribe = useTaskStore.subscribe(() => { publications += 1; });

    useTaskStore.getState().upsertTask({
      ...current,
      progress: current.progress,
    });

    unsubscribe();
    expect(publications).toBe(0);
    expect(useTaskStore.getState().tasks[0]).toBe(current);
  });

  it("does not publish an older snapshot over a newer terminal fact", () => {
    const completed = succeededTask("task-a", "project-a");
    useTaskStore.getState().upsertTask(completed);
    let publications = 0;
    const unsubscribe = useTaskStore.subscribe(() => { publications += 1; });

    useTaskStore.getState().upsertTask(task("task-a", "project-a"));

    unsubscribe();
    expect(publications).toBe(0);
    expect(useTaskStore.getState().taskById[completed.id]).toBe(completed);
  });

  it("keeps project ids and running counts referentially stable for progress-only updates", () => {
    const running = task("task-a", "project-a");
    useTaskStore.getState().upsertTask(running);
    const before = useTaskStore.getState();

    useTaskStore.getState().upsertTask({
      ...running,
      progress: { current: 1, total: 2, label: "Importing" },
      updatedAt: "2026-06-21T00:00:01Z",
    });

    const after = useTaskStore.getState();
    expect(after.taskIdsByProject).toBe(before.taskIdsByProject);
    expect(after.taskIdsByProject["project-a"]).toBe(before.taskIdsByProject["project-a"]);
    expect(after.runningCountByProject).toBe(before.runningCountByProject);
    expect(after.runningCount).toBe(1);
  });

  it("keeps running-count indexes stable when adding a non-running task", () => {
    const before = useTaskStore.getState().runningCountByProject;

    useTaskStore.getState().upsertTask(succeededTask("task-a", "project-a"));

    expect(useTaskStore.getState().runningCountByProject).toBe(before);
  });

  it("provides stable project ids and rejects retained facts from another project", () => {
    const retained = task("task-a", "project-a");
    useTaskStore.getState().upsertTask(retained);
    const firstIds = selectTaskIdsForProject(useTaskStore.getState(), "project-a");

    expect(selectTaskIdsForProject(useTaskStore.getState(), "project-a")).toBe(firstIds);
    expect(selectProjectTaskById(useTaskStore.getState(), "project-b", retained.id)).toBeNull();
    expect(selectProjectTaskById(useTaskStore.getState(), "project-a", retained.id)).toBe(retained);
  });

  it("records background-project facts without changing current-project presentation", () => {
    const background = task("task-b", "project-b");
    useTaskStore.setState({ drawerOpen: true, selectedTaskId: "task-a" });

    handleTaskEvent({
      eventId: "event-project-b",
      eventType: "task_updated",
      projectId: "project-b",
      taskId: background.id,
      timestamp: background.updatedAt,
      payload: background,
    });

    const state = useTaskStore.getState();
    expect(state.taskFacts[background.id]).toEqual(background);
    expect(state.tasks).toEqual([]);
    expect(state.drawerOpen).toBe(true);
    expect(state.selectedTaskId).toBe("task-a");
  });

  it("publishes one exact bounded tail for an aggregated stream delta", () => {
    const fullText = "x".repeat(600 * 1024);
    let publications = 0;
    const unsubscribe = useTaskStore.subscribe(() => { publications += 1; });

    handleTaskEvent({
      eventId: "stream-batch",
      eventType: "task_stream_output",
      projectId: "project-a",
      taskId: "task-a",
      timestamp: "2026-08-16T00:00:00Z",
      payload: { delta: fullText, route: "chat-byok" },
    });

    unsubscribe();
    expect(publications).toBe(1);
    expect(useTaskStore.getState().taskOutputs["task-a"]).toBe(fullText.slice(-512 * 1024));
  });

  it("propagates task list and recovery failures so the UI can report them", async () => {
    invokeMock.mockRejectedValueOnce(new Error("task registry unavailable"));
    await expect(fetchTasks("project-a", "D:/project-a")).rejects.toThrow(
      "task registry unavailable",
    );

    invokeMock.mockRejectedValueOnce(new Error("recovery failed"));
    await expect(recoverTasksForProject("project-b", "D:/project-b")).rejects.toThrow(
      "recovery failed",
    );
  });
});
