import { describe, expect, it } from "vitest";

import type { BackendEvent, BackendEventType, StreamDelta } from "../types/task";
import { TaskEventDispatcher } from "./taskEventDispatcher";

function streamEvent(projectId: string, taskId: string, delta: string): BackendEvent<StreamDelta> {
  return {
    eventId: `${taskId}-${delta}`,
    eventType: "task_stream_output",
    projectId,
    taskId,
    timestamp: "2026-08-16T00:00:00Z",
    payload: { delta, route: "chat-agent" },
  };
}

function terminalEvent(
  projectId: string,
  taskId: string,
  eventType: Extract<BackendEventType, "task_completed" | "task_failed" | "task_cancelled">,
): BackendEvent {
  return {
    eventId: `${taskId}-${eventType}`,
    eventType,
    projectId,
    taskId,
    timestamp: "2026-08-16T00:00:01Z",
    payload: {},
  };
}

describe("TaskEventDispatcher", () => {
  it("always runs the event owner before feature listeners", () => {
    const dispatcher = new TaskEventDispatcher();
    const order: string[] = [];
    dispatcher.register(() => order.push("feature-first-registered"));
    dispatcher.registerOwner(() => order.push("owner"));
    dispatcher.register(() => order.push("feature-second"));

    dispatcher.dispatch(terminalEvent("project-a", "task-a", "task_completed"));

    expect(order).toEqual(["owner", "feature-first-registered", "feature-second"]);
  });

  it.each(["task_completed", "task_failed", "task_cancelled"] as const)(
    "flushes the remaining delta before %s",
    (eventType) => {
      const dispatcher = new TaskEventDispatcher();
      const observed: BackendEvent[] = [];
      dispatcher.register((event) => observed.push(event));

      dispatcher.dispatch(streamEvent("project-a", "task-a", "tail"));
      dispatcher.dispatch(terminalEvent("project-a", "task-a", eventType));

      expect(observed.map((event) => event.eventType)).toEqual([
        "task_stream_output",
        eventType,
      ]);
      expect((observed[0]?.payload as StreamDelta).delta).toBe("tail");
    },
  );

  it("drops project A presentation buffers when project B becomes active", () => {
    const dispatcher = new TaskEventDispatcher();
    const observed: BackendEvent[] = [];
    dispatcher.register((event) => observed.push(event));

    dispatcher.dispatch(streamEvent("project-a", "task-a", "must-not-leak"));
    dispatcher.retainProject("project-b");
    dispatcher.dispatch(streamEvent("project-b", "task-b", "visible"));
    dispatcher.dispatch(terminalEvent("project-b", "task-b", "task_completed"));

    expect(observed.map((event) => event.projectId)).toEqual(["project-b", "project-b"]);
    expect((observed[0]?.payload as StreamDelta).delta).toBe("visible");
  });
});
