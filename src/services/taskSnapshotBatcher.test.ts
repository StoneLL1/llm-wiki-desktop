import { afterEach, describe, expect, it, vi } from "vitest";

import type { BackendEvent, BackendTask } from "../types/task";
import { TaskSnapshotBatcher } from "./taskSnapshotBatcher";

function task(progress: number, status: BackendTask["status"] = "running"): BackendTask {
  const terminal = status === "succeeded" || status === "failed" || status === "cancelled" || status === "interrupted";
  return {
    id: "task-a",
    taskType: "import",
    projectId: "project-a",
    title: "Import",
    status,
    progress: terminal ? null : { current: progress, total: 100, label: "Import" },
    startedAt: "2026-08-27T00:00:00Z",
    updatedAt: `2026-08-27T00:00:${String(progress).padStart(2, "0")}Z`,
    completedAt: terminal ? `2026-08-27T00:00:${String(progress).padStart(2, "0")}Z` : null,
    cancellable: !terminal,
    logPath: null,
    result: null,
    error: null,
  };
}

function event(progress: number, status: BackendTask["status"] = "running"): BackendEvent<BackendTask> {
  const payload = task(progress, status);
  return {
    eventId: `event-${progress}-${status}`,
    eventType: status === "succeeded" ? "task_completed" : "task_updated",
    projectId: "project-a",
    taskId: payload.id,
    timestamp: payload.updatedAt,
    payload,
  };
}

afterEach(() => {
  vi.useRealTimers();
});

describe("TaskSnapshotBatcher", () => {
  it("coalesces a sustained 10 Hz progress stream to at most 5 Hz", () => {
    vi.useFakeTimers();
    const delivered: BackendEvent<BackendTask>[] = [];
    const batcher = new TaskSnapshotBatcher((next) => delivered.push(next));

    for (let progress = 1; progress <= 10; progress += 1) {
      batcher.enqueue(event(progress));
      vi.advanceTimersByTime(100);
    }
    vi.advanceTimersByTime(250);

    expect(delivered.length).toBeLessThanOrEqual(5);
    expect((delivered.at(-1)?.payload as BackendTask).progress?.current).toBe(10);
  });

  it("drops pending progress and delivers terminal state immediately", () => {
    vi.useFakeTimers();
    const delivered: BackendEvent<BackendTask>[] = [];
    const batcher = new TaskSnapshotBatcher((next) => delivered.push(next));

    batcher.enqueue(event(1));
    batcher.enqueue(event(2));
    batcher.enqueue(event(3, "succeeded"));

    expect(delivered.map((next) => (next.payload as BackendTask).status)).toEqual([
      "running",
      "succeeded",
    ]);
    expect((delivered.at(-1)?.payload as BackendTask).progress).toBeNull();
    vi.runAllTimers();
    expect(delivered).toHaveLength(2);
  });

  it("drops exact duplicate snapshots before scheduling a flush", () => {
    vi.useFakeTimers();
    const delivered = vi.fn<(next: BackendEvent<BackendTask>) => void>();
    const batcher = new TaskSnapshotBatcher(delivered);
    const first = event(1);

    batcher.enqueue(first);
    batcher.enqueue({ ...first, eventId: "duplicate" });
    vi.runAllTimers();

    expect(delivered).toHaveBeenCalledTimes(1);
  });

  it("does not let a late progress snapshot replace newer pending progress", () => {
    vi.useFakeTimers();
    const delivered: BackendEvent<BackendTask>[] = [];
    const batcher = new TaskSnapshotBatcher((next) => delivered.push(next));

    batcher.enqueue(event(1));
    batcher.enqueue(event(3));
    batcher.enqueue(event(2));
    vi.runAllTimers();

    expect(delivered.map((next) => (next.payload as BackendTask).progress?.current)).toEqual([1, 3]);
  });

  it("does not deliver stale running state after a terminal boundary", () => {
    vi.useFakeTimers();
    const delivered: BackendEvent<BackendTask>[] = [];
    const batcher = new TaskSnapshotBatcher((next) => delivered.push(next));

    batcher.enqueue(event(3, "succeeded"));
    batcher.enqueue(event(2));
    vi.runAllTimers();

    expect(delivered).toHaveLength(1);
    expect((delivered[0].payload as BackendTask).status).toBe("succeeded");
  });
});
