import { afterEach, describe, expect, it, vi } from "vitest";

import type { BackendEvent } from "../types/task";
import {
  isTaskEventForProject,
  notifyTaskEventListeners,
  registerTaskEventListener,
} from "./useTaskEvents";

const event: BackendEvent = {
  eventId: "event-1",
  eventType: "task_completed",
  projectId: "project-a",
  taskId: "task-1",
  timestamp: "2026-07-13T00:00:00Z",
  payload: {},
};

afterEach(() => vi.restoreAllMocks());

describe("task event listener bridge", () => {
  it("accepts only events owned by the active project", () => {
    expect(isTaskEventForProject(event, "project-a")).toBe(true);
    expect(isTaskEventForProject(event, "project-b")).toBe(false);
    expect(isTaskEventForProject({ ...event, projectId: null }, "project-a")).toBe(false);
  });
  it("returns an unsubscribe handle for workflow-owned listeners", () => {
    const listener = vi.fn();
    const unsubscribe = registerTaskEventListener(listener);
    notifyTaskEventListeners(event);
    expect(listener).toHaveBeenCalledWith(event);
    expect(typeof unsubscribe).toBe("function");
    unsubscribe();
    notifyTaskEventListeners(event);
    expect(listener).toHaveBeenCalledTimes(1);
  });

  it("keeps independent listeners isolated", () => {
    const first = vi.fn();
    const second = vi.fn();
    const removeFirst = registerTaskEventListener(first);
    const removeSecond = registerTaskEventListener(second);
    notifyTaskEventListeners(event);
    expect(first).toHaveBeenCalledTimes(1);
    expect(second).toHaveBeenCalledTimes(1);
    removeFirst();
    removeSecond();
    notifyTaskEventListeners(event);
    expect(first).toHaveBeenCalledTimes(1);
    expect(second).toHaveBeenCalledTimes(1);
  });
});
