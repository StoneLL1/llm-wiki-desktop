import { beforeEach, describe, expect, it, vi } from "vitest";

import type { BackendTask } from "../types/task";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

import { recoverTasksForProject, useTaskStore } from "./taskStore";

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

beforeEach(() => {
  invokeMock.mockReset();
  useTaskStore.getState().setTasks([]);
  Object.defineProperty(window, "__TAURI_INTERNALS__", { value: {}, configurable: true });
});

describe("recoverTasksForProject", () => {
  it("keeps the backend global task list when switching projects", async () => {
    const tasks = [task("task-a", "project-a"), task("task-b", "project-b")];
    invokeMock.mockResolvedValueOnce(tasks);

    await recoverTasksForProject("project-b", "D:/project-b");

    expect(invokeMock).toHaveBeenCalledWith("set_active_project", {
      request: { projectId: "project-b", rootPath: "D:/project-b" },
    });
    expect(useTaskStore.getState().tasks).toEqual(tasks);
  });
});
