import { beforeEach, describe, expect, it, vi } from "vitest";

import type { BackendTask } from "../types/task";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

import { fetchTasks, handleTaskEvent, recoverTasksForProject, useTaskStore } from "./taskStore";

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
  useTaskStore.getState().setTasks([]);
  Object.defineProperty(window, "__TAURI_INTERNALS__", { value: {}, configurable: true });
});

describe("recoverTasksForProject", () => {
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

  it("keeps the backend global task list when switching projects", async () => {
    const tasks = [task("task-a", "project-a"), task("task-b", "project-b")];
    invokeMock.mockResolvedValueOnce(tasks);

    await recoverTasksForProject("project-b", "D:/project-b");

    expect(invokeMock).toHaveBeenCalledWith("set_active_project", {
      request: { projectId: "project-b", rootPath: "D:/project-b" },
    });
    expect(useTaskStore.getState().tasks).toEqual(tasks);
  });

  it("propagates task list and recovery failures so the UI can report them", async () => {
    invokeMock.mockRejectedValueOnce(new Error("task registry unavailable"));
    await expect(fetchTasks()).rejects.toThrow("task registry unavailable");

    invokeMock.mockRejectedValueOnce(new Error("recovery failed"));
    await expect(recoverTasksForProject("project-b", "D:/project-b")).rejects.toThrow(
      "recovery failed",
    );
  });
});
