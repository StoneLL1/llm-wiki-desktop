import { describe, expect, it } from "vitest";

import type { BackendTask } from "../../types/task";
import { collectUpdateInstallGuard, type UpdateInstallGuardInput } from "./installGuard";

function input(): UpdateInstallGuardInput {
  return {
    editor: { mode: "read", saveState: "idle", draft: "", savedMarkdown: null },
    importSession: null,
    importConfirming: false,
    workflowRuns: [],
    tasks: [],
    projectPendingAction: false,
    lintPendingConfirmation: false,
  };
}

function task(cancellable: boolean): BackendTask {
  return {
    id: cancellable ? "safe" : "critical",
    taskType: "workflow",
    projectId: "project-a",
    title: "task",
    status: "running",
    progress: null,
    startedAt: "2026-08-21T00:00:00Z",
    updatedAt: "2026-08-21T00:00:00Z",
    completedAt: null,
    cancellable,
    logPath: null,
    result: null,
    error: null,
  };
}

describe("collectUpdateInstallGuard", () => {
  it("blocks an unsaved wiki draft", () => {
    const value = input();
    value.editor = {
      mode: "edit",
      saveState: "idle",
      draft: "changed",
      savedMarkdown: "saved",
    };

    expect(collectUpdateInstallGuard(value)).toMatchObject({
      blockers: ["unsaved_editor"],
      request: { unsavedEditor: true },
    });
  });

  it.each(["read", "preview"] as const)(
    "keeps blocking a changed draft after switching to %s mode",
    (mode) => {
      const value = input();
      value.editor = {
        mode,
        saveState: "idle",
        draft: "changed",
        savedMarkdown: "saved",
      };

      expect(collectUpdateInstallGuard(value).blockers).toContain("unsaved_editor");
    },
  );

  it("blocks non-interruptible tasks but only advises for ordinary cancellable tasks", () => {
    const critical = input();
    critical.tasks = [task(false)];
    expect(collectUpdateInstallGuard(critical).blockers).toContain("critical_task");

    const ordinary = input();
    ordinary.tasks = [task(true)];
    expect(collectUpdateInstallGuard(ordinary)).toMatchObject({
      blockers: [],
      safeRunningTaskCount: 1,
    });
  });

  it("blocks a persisted Import commit from another project", () => {
    const value = input();
    value.tasks = [{
      ...task(true),
      projectId: "project-a",
      operation: { kind: "import_commit", sessionId: "session-a" },
    }];

    expect(collectUpdateInstallGuard(value)).toMatchObject({
      blockers: ["critical_task"],
      safeRunningTaskCount: 0,
    });
  });

  it("does not describe an applying workflow task as safe", () => {
    const value = input();
    value.tasks = [task(true)];
    value.workflowRuns = [{
      taskId: "safe",
      displayStatus: "running",
      currentStageId: "apply_changes",
    } as UpdateInstallGuardInput["workflowRuns"][number]];

    expect(collectUpdateInstallGuard(value)).toMatchObject({
      blockers: ["workflow_apply"],
      safeRunningTaskCount: 0,
    });
  });

  it("allows a safe idle runtime without inventing a Git checkpoint", () => {
    expect(collectUpdateInstallGuard(input())).toEqual({
      blockers: [],
      safeRunningTaskCount: 0,
      request: {
        unsavedEditor: false,
        importCommitActive: false,
        pendingUserConfirmation: false,
      },
    });
  });
});
