import { beforeEach, describe, expect, it } from "vitest";
import type { BackendTask } from "../../types/task";
import {
  DEFAULT_TASK_SORT_MODE,
  readTaskSortModePreference,
  sortTasks,
  writeTaskSortModePreference,
} from "./taskSort";

const baseTask = (overrides: Partial<BackendTask>): BackendTask => ({
  id: overrides.id ?? "task",
  taskType: "import",
  projectId: "project-1",
  title: overrides.title ?? "Task",
  status: overrides.status ?? "succeeded",
  progress: null,
  startedAt: overrides.startedAt ?? "2026-07-04T00:00:00Z",
  updatedAt: overrides.updatedAt ?? "2026-07-04T00:00:00Z",
  completedAt: overrides.completedAt ?? null,
  cancellable: true,
  logPath: null,
  result: null,
  error: null,
});

describe("sortTasks", () => {
  it("defaults to latest execution time using startedAt before updatedAt", () => {
    const tasks = [
      baseTask({
        id: "old-running",
        status: "running",
        startedAt: "2026-07-04T01:00:00Z",
        updatedAt: "2026-07-04T05:00:00Z",
      }),
      baseTask({
        id: "new-failed",
        status: "failed",
        startedAt: "2026-07-04T03:00:00Z",
        updatedAt: "2026-07-04T03:01:00Z",
      }),
      baseTask({
        id: "legacy",
        status: "succeeded",
        startedAt: "",
        updatedAt: "2026-07-04T04:00:00Z",
      }),
    ];

    expect(sortTasks(tasks, "execution_time").map((task) => task.id)).toEqual([
      "legacy",
      "new-failed",
      "old-running",
    ]);
  });

  it("can sort by latest update time", () => {
    const tasks = [
      baseTask({
        id: "started-new",
        startedAt: "2026-07-04T05:00:00Z",
        updatedAt: "2026-07-04T05:00:00Z",
      }),
      baseTask({
        id: "updated-new",
        startedAt: "2026-07-04T01:00:00Z",
        updatedAt: "2026-07-04T06:00:00Z",
      }),
    ];

    expect(sortTasks(tasks, "updated_time").map((task) => task.id)).toEqual([
      "updated-new",
      "started-new",
    ]);
  });

  it("keeps status sorting as an explicit mode with updated time as tie-breaker", () => {
    const tasks = [
      baseTask({ id: "failed", status: "failed", updatedAt: "2026-07-04T06:00:00Z" }),
      baseTask({
        id: "running-old",
        status: "running",
        updatedAt: "2026-07-04T01:00:00Z",
      }),
      baseTask({
        id: "running-new",
        status: "running",
        updatedAt: "2026-07-04T02:00:00Z",
      }),
    ];

    expect(sortTasks(tasks, "status").map((task) => task.id)).toEqual([
      "running-new",
      "running-old",
      "failed",
    ]);
  });
});

describe("task sort preference", () => {
  beforeEach(() => window.localStorage.clear());

  it("falls back to execution time for missing or corrupt stored values", () => {
    window.localStorage.setItem("llm-wiki-desktop.taskSortMode.v1", "bad");
    expect(readTaskSortModePreference()).toBe(DEFAULT_TASK_SORT_MODE);
  });

  it("round-trips a valid mode", () => {
    writeTaskSortModePreference("status");
    expect(readTaskSortModePreference()).toBe("status");
  });
});
