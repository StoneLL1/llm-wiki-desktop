import { beforeEach, describe, expect, it, vi } from "vitest";

import type { BackendTask } from "../types/task";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

import {
  fetchTaskById,
  fetchTasks,
  handleTaskEvent,
  recoverTasksForProject,
  useTaskStore,
} from "./taskStore";
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

beforeEach(() => {
  invokeMock.mockReset();
  useTaskStore.setState({
    activeProjectId: "project-a",
    activeProjectRootPath: "D:/project-a",
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

  it("records one store publication per current task update as the Batch A expected-red baseline", () => {
    let publications = 0;
    const unsubscribe = useTaskStore.subscribe(() => { publications += 1; });
    for (let index = 0; index < 100; index += 1) {
      useTaskStore.getState().upsertTask({ ...task(`task-${index}`, "project-a"), updatedAt: `2026-06-21T00:00:${index % 60}Z` });
    }
    unsubscribe();
    expect(publications).toBe(100);
    expect(useTaskStore.getState().tasks).toHaveLength(100);
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
