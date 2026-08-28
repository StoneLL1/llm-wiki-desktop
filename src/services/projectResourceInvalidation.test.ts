import { describe, expect, it, vi } from "vitest";

import type { BackendEvent, BackendTask, TaskType } from "../types/task";
import { createProjectResourceController } from "../lib/projectResourceFreshness";
import {
  invalidateProjectResources,
  observeProjectResources,
  registerProjectResource,
} from "../stores/projectScope";
import {
  gitFactsChangedForBackendEvent,
  projectResourcesForBackendEvent,
} from "./projectResourceInvalidation";

const scope = { projectId: "project-a", rootPath: "D:/知识库" };

function terminalEvent(taskType: TaskType): BackendEvent<BackendTask> {
  return {
    eventId: `event-${taskType}`,
    eventType: "task_completed",
    projectId: scope.projectId,
    taskId: `task-${taskType}`,
    timestamp: "2026-08-16T00:00:00Z",
    payload: {
      id: `task-${taskType}`,
      taskType,
      projectId: scope.projectId,
      title: taskType,
      status: "succeeded",
      progress: null,
      startedAt: "2026-08-16T00:00:00Z",
      updatedAt: "2026-08-16T00:00:01Z",
      completedAt: "2026-08-16T00:00:01Z",
      cancellable: false,
      logPath: null,
      result: null,
      error: null,
    },
  };
}

describe("project resource invalidation", () => {
  it("revalidates only while the matching project resource is observed", async () => {
    const invalidate = vi.fn();
    const revalidate = vi.fn();
    const unregister = registerProjectResource("wiki", { invalidate }, revalidate);
    const unobserve = observeProjectResources(scope, ["wiki"]);

    invalidateProjectResources(scope, ["wiki"], true);
    expect(invalidate).toHaveBeenCalledTimes(1);
    expect(revalidate).toHaveBeenCalledWith(scope);

    unobserve();
    invalidateProjectResources(scope, ["wiki"], true);
    expect(invalidate).toHaveBeenCalledTimes(2);
    expect(revalidate).toHaveBeenCalledTimes(1);
    unregister();
  });

  it("maps backend events to conservative feature-level invalidations", () => {
    expect(projectResourcesForBackendEvent({
      ...terminalEvent("wiki_compile"),
      eventType: "wiki_changed",
    })).toEqual(["wiki", "graph"]);
    expect(projectResourcesForBackendEvent(terminalEvent("export"))).toEqual(["exports"]);
    expect(projectResourcesForBackendEvent(terminalEvent("deep_lint"))).toEqual([
      "lint-history",
      "lint-ignores",
    ]);
    expect(projectResourcesForBackendEvent(terminalEvent("llm_request"))).toEqual([
      "chat-sessions",
    ]);
  });

  it("invalidates Git facts only when a terminal task reports real affected paths", () => {
    const completed = terminalEvent("workflow");
    completed.payload.result = { summary: "changed", affectedPaths: ["wiki/page.md"] };
    expect(gitFactsChangedForBackendEvent(completed)).toBe(true);
    expect(gitFactsChangedForBackendEvent({
      ...completed,
      eventType: "task_failed",
    })).toBe(true);
    expect(gitFactsChangedForBackendEvent({
      ...completed,
      eventType: "task_cancelled",
      payload: { ...completed.payload, result: null },
    })).toBe(false);
    expect(gitFactsChangedForBackendEvent({
      ...completed,
      eventType: "task_updated",
    })).toBe(false);
  });

  it("coalesces repeated invalidations during one in-flight refresh into one follow-up", async () => {
    let resolveFirst!: () => void;
    const first = new Promise<void>((resolve) => { resolveFirst = resolve; });
    const resource = createProjectResourceController<void>("wiki");
    const load = vi.fn().mockReturnValueOnce(first).mockResolvedValue(undefined);
    const revalidate = vi.fn(() => resource.ensure(scope, load));
    const unregister = registerProjectResource("wiki", resource, revalidate);
    const unobserve = observeProjectResources(scope, ["wiki"]);

    invalidateProjectResources(scope, ["wiki"], true);
    invalidateProjectResources(scope, ["wiki"], true);
    invalidateProjectResources(scope, ["wiki"], true);
    expect(load).toHaveBeenCalledTimes(1);

    resolveFirst();
    await vi.waitFor(() => expect(load).toHaveBeenCalledTimes(2));

    unobserve();
    unregister();
  });
});
