import { describe, expect, it } from "vitest";

import type { ImportItem, ImportSession } from "../../types/importV2";
import type { BackendTask } from "../../types/task";
import { buildImportBatchProgress } from "./useImportBatchController";

function task(id: string, status: BackendTask["status"], cancellable = true): BackendTask {
  return {
    id,
    taskType: "import",
    projectId: "project-a",
    title: `Task ${id}`,
    status,
    progress: null,
    startedAt: "2026-07-22T00:00:00Z",
    updatedAt: "2026-07-22T00:00:00Z",
    completedAt: null,
    cancellable,
    logPath: null,
    result: null,
    error: null,
  };
}

function item(itemId: string, status: ImportItem["status"]): ImportItem {
  return {
    itemId,
    input: {
      kind: "file",
      displayName: itemId,
      locator: `C:\\sources\\${itemId}`,
      normalizedLocator: null,
    },
    status,
    selected: status === "preview_ready",
    taskId: null,
    progress: null,
    attempts: [],
    preview: null,
    issue: null,
  };
}

describe("buildImportBatchProgress", () => {
  it("derives review-ready, failed, active, and unknown counts from task snapshots", () => {
    const records = [{
      id: "batch-a",
      sessionId: "session-a",
      projectKey: "project-a\0D:/wiki",
      epoch: 2,
      tasks: [
        { taskId: "done", itemId: "done.md", title: "Done" },
        { taskId: "waiting", itemId: "waiting.md", title: "Waiting" },
        { taskId: "failed", itemId: "failed.md", title: "Failed" },
        { taskId: "running", itemId: "running.md", title: "Running" },
        { taskId: "missing", itemId: "missing.md", title: "Missing snapshot" },
      ],
    }];
    const session = {
      schemaVersion: 2,
      sessionId: "session-a",
      projectId: "project-a",
      status: "draft",
      resourceMode: "balanced",
      createdAt: "2026-07-22T00:00:00Z",
      updatedAt: "2026-07-22T00:00:00Z",
      items: [item("waiting.md", "preview_ready")],
    } satisfies ImportSession;

    const [progress] = buildImportBatchProgress(records, [
      task("done", "succeeded"),
      task("waiting", "waiting_for_confirmation", false),
      task("failed", "failed"),
      task("running", "running", false),
    ], session);

    expect(progress).toMatchObject({
      total: 5,
      processed: 3,
      active: 1,
      completed: 1,
      waitingForConfirmation: 1,
      reviewReady: 1,
      failed: 1,
      unknown: 1,
      nonCancellable: 1,
      failedItemIds: ["failed.md"],
    });
    expect(progress?.tasks.find((entry) => entry.id === "missing")?.status).toBe("unknown");
  });

  it("shows one operation task while item facts retain partial, waiting, preview, failure, skip, and cancel states", () => {
    const operation = {
      ...task("operation-a", "running"),
      batchId: "import-v2-operation:session-a",
    };
    const statuses: Array<[string, ImportItem["status"]]> = [
      ["active.md", "extracting"],
      ["preview.md", "preview_ready"],
      ["waiting.md", "waiting_login"],
      ["failed.md", "failed"],
      ["skipped.md", "skipped"],
      ["cancelled.md", "cancelled"],
      ["completed.md", "completed"],
    ];
    const items = statuses.map(([itemId, status]) => ({
      ...item(itemId, status),
      taskId: operation.id,
    }));
    const session = {
      schemaVersion: 2,
      sessionId: "session-a",
      projectId: "project-a",
      status: "processing",
      resourceMode: "balanced",
      createdAt: "2026-07-22T00:00:00Z",
      updatedAt: "2026-07-22T00:00:00Z",
      items,
    } satisfies ImportSession;

    const [progress] = buildImportBatchProgress([{
      id: operation.id,
      sessionId: session.sessionId,
      projectKey: "project-a\0D:/wiki",
      epoch: 2,
      tasks: [{ taskId: operation.id, itemId: "", title: operation.title }],
      itemIds: items.map((value) => value.itemId),
      operationTaskId: operation.id,
    }], [operation], session);

    expect(progress).toMatchObject({
      total: 7,
      processed: 6,
      active: 1,
      completed: 2,
      waitingForConfirmation: 2,
      reviewReady: 1,
      failed: 1,
      cancelled: 2,
      failedItemIds: ["failed.md"],
    });
    expect(progress?.tasks).toHaveLength(1);
    expect(progress?.tasks[0]?.id).toBe(operation.id);
  });
});
