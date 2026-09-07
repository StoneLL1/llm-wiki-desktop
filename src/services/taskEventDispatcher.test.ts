import { describe, expect, it } from "vitest";

import type { BackendEvent, BackendEventType, BackendTask, StreamDelta } from "../types/task";
import type { WorkflowRunSummary } from "../types/workflow";
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

function taskSnapshotEvent(progress = 1): BackendEvent<BackendTask> {
  return {
    eventId: `task-a-${progress}`,
    eventType: "task_updated",
    projectId: "project-a",
    taskId: "task-a",
    timestamp: `2026-08-16T00:00:0${progress}Z`,
    payload: {
      id: "task-a",
      taskType: "import",
      projectId: "project-a",
      title: "Import",
      status: "running",
      progress: { current: progress, total: 10, label: "Importing" },
      startedAt: "2026-08-16T00:00:00Z",
      updatedAt: `2026-08-16T00:00:0${progress}Z`,
      completedAt: null,
      cancellable: true,
      logPath: null,
      result: null,
      error: null,
    },
  };
}

function workflowEvent(overrides: Partial<WorkflowRunSummary> = {}): BackendEvent<WorkflowRunSummary> {
  const run: WorkflowRunSummary = {
    schemaVersion: 1, revision: "100", sessionId: "session-old", taskId: "workflow-a",
    projectId: "project-a", canonicalIdentityKey: "identity-a", identityRevision: "revision-a",
    kind: "update_wiki", operation: { kind: "built_in" }, displayStatus: "running",
    retry: null, outcome: null, startedAt: "2026-09-07T00:00:00Z",
    updatedAt: "2026-09-07T00:01:00Z", completedAt: null,
    ...overrides,
  };
  return { eventId: `${run.sessionId}-${run.revision}`, eventType: "workflow_updated", taskId: run.taskId, projectId: run.projectId, timestamp: run.updatedAt, payload: run };
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

  it("publishes one canonical snapshot before feature listeners inspect it", () => {
    const dispatcher = new TaskEventDispatcher();
    let canonicalTask: BackendTask | null = null;
    const observed: BackendTask[] = [];
    dispatcher.registerOwner((event) => { canonicalTask = event.payload as BackendTask; });
    dispatcher.register(() => {
      if (canonicalTask) observed.push(canonicalTask);
    });

    const event = taskSnapshotEvent();
    dispatcher.dispatch(event);
    dispatcher.dispatch({ ...event, eventId: "semantic-duplicate" });

    expect(observed).toEqual([event.payload]);
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

  it.each(["completed", "failed", "cancelled", "waiting_for_confirmation"] as const)(
    "immediately delivers a new-session %s below an old pending progress revision",
    (displayStatus) => {
      const dispatcher = new TaskEventDispatcher();
      const observed: BackendEvent[] = [];
      dispatcher.registerOwner((event) => observed.push(event));
      const oldProgress = workflowEvent();
      const restored = workflowEvent({ sessionId: "session-new", revision: "5", displayStatus });
      dispatcher.dispatch(oldProgress);
      expect(observed).toEqual([]);

      dispatcher.dispatch(restored);
      expect(observed).toEqual([restored]);
      dispatcher.clearPending();
      // The event owner, not the dispatcher, decides whether the old session is retired.
      expect(observed).toEqual([restored, oldProgress]);
    },
  );

  it.each([
    { projectId: "project-b" },
    { canonicalIdentityKey: "identity-b" },
    { identityRevision: "revision-b" },
  ])("does not compare or overwrite pending revisions from a different owner %o", (owner) => {
    const dispatcher = new TaskEventDispatcher();
    const observed: BackendEvent[] = [];
    dispatcher.registerOwner((event) => observed.push(event));
    const original = workflowEvent();
    const otherProgress = workflowEvent({ ...owner, revision: "3" });
    const otherTerminal = workflowEvent({ ...owner, revision: "5", displayStatus: "completed" });
    dispatcher.dispatch(original);
    dispatcher.dispatch(otherProgress);
    dispatcher.dispatch(otherTerminal);

    expect(observed).toEqual([otherTerminal]);
    dispatcher.clearPending();
    expect(observed).toEqual([otherTerminal, original]);
  });

  it("keeps revision ordering within the same session and owner without delaying its terminal", () => {
    const dispatcher = new TaskEventDispatcher();
    const observed: BackendEvent[] = [];
    dispatcher.registerOwner((event) => observed.push(event));
    dispatcher.dispatch(workflowEvent());
    dispatcher.dispatch(workflowEvent({ revision: "5", displayStatus: "completed" }));
    expect(observed).toEqual([]);

    const terminal = workflowEvent({ revision: "101", displayStatus: "completed" });
    dispatcher.dispatch(terminal);
    expect(observed).toEqual([terminal]);
    dispatcher.clearPending();
    expect(observed).toEqual([terminal]);
  });

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
