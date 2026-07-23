import { describe, expect, it } from "vitest";

import type { ImportItem } from "../../types/importV2";
import type { BackendTask } from "../../types/task";
import { mergeImportItemTask } from "./importTaskProgress";

function item(overrides: Partial<ImportItem> = {}): ImportItem {
  return {
    itemId: "item-1",
    input: { kind: "url", displayName: "Video", locator: "https://example.com", normalizedLocator: null },
    status: "queued",
    selected: true,
    taskId: null,
    progress: null,
    attempts: [],
    preview: null,
    issue: null,
    ...overrides,
  };
}

function task(overrides: Partial<BackendTask> = {}): BackendTask {
  return {
    id: "task-1",
    taskType: "import",
    projectId: "project-1",
    title: "Import Video",
    status: "running",
    progress: { current: 48, total: 100, label: "asr.recognizing" },
    startedAt: "2026-07-23T00:00:00Z",
    updatedAt: "2026-07-23T00:00:01Z",
    completedAt: null,
    cancellable: true,
    logPath: null,
    result: null,
    error: null,
    ...overrides,
  };
}

describe("mergeImportItemTask", () => {
  it("binds a newly started task and exposes measured ASR progress", () => {
    expect(mergeImportItemTask(item(), task(), true)).toMatchObject({
      taskId: "task-1",
      status: "extracting",
      progress: { current: 48, total: 100, label: "asr.recognizing" },
    });
  });

  it("replaces the waiting authorization task when an explicitly mapped retry starts", () => {
    expect(mergeImportItemTask(
      item({ taskId: "old-task", status: "waiting_authorization" }),
      task(),
      true,
    )).toMatchObject({
      taskId: "task-1",
      status: "extracting",
      progress: { current: 48, total: 100, label: "asr.recognizing" },
    });
  });

  it("clears progress inherited from the previous task when a queued retry is bound", () => {
    expect(mergeImportItemTask(
      item({
        taskId: "old-task",
        status: "failed",
        progress: { current: 99, total: 100, label: "asr.finalizing" },
      }),
      task({ status: "queued", progress: null }),
      true,
    )).toMatchObject({
      taskId: "task-1",
      status: "queued",
      progress: null,
    });
  });

  it("does not bind an unrelated task without an explicit start mapping", () => {
    const source = item();
    expect(mergeImportItemTask(source, task())).toBe(source);
  });

  it("does not regress validation to extraction when a late progress event arrives", () => {
    expect(mergeImportItemTask(
      item({ taskId: "task-1", status: "validating" }),
      task(),
    ).status).toBe("validating");
  });
});
