import { beforeEach, describe, expect, it, vi } from "vitest";

const permissionMock = vi.hoisted(() => vi.fn());
const sendNotificationMock = vi.hoisted(() => vi.fn());
const showMock = vi.hoisted(() => vi.fn());
const focusMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: permissionMock,
  requestPermission: vi.fn().mockResolvedValue("granted"),
  sendNotification: sendNotificationMock,
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ show: showMock, setFocus: focusMock }),
}));

import { recordWorkflowFacts, useTaskStore } from "../stores/taskStore";
import { useSettingsStore } from "../stores/settingsStore";
import { defaultProject, useProjectStore } from "../stores/projectStore";
import type { WorkflowRun, WorkflowRunSummary } from "../types/workflow";
import type { BackendEvent } from "../types/task";
import { handleNotificationAction, invalidateNotificationPermissionEpoch, notifyTaskEvent } from "./notifications";

beforeEach(() => {
  invalidateNotificationPermissionEpoch();
  permissionMock.mockReset().mockResolvedValue(true);
  sendNotificationMock.mockReset();
  showMock.mockReset().mockResolvedValue(undefined);
  focusMock.mockReset().mockResolvedValue(undefined);
  useTaskStore.setState({ drawerOpen: false, selectedTaskId: null, workflowById: {}, workflowSessionId: null, retiredWorkflowSessions: [] });
  useSettingsStore.setState((state) => ({
    settings: {
      ...state.settings,
      systemNotifications: {
        onTaskCompleted: true,
        onTaskFailed: true,
        onConfirmationNeeded: true,
        onLongTaskProgress: false,
      },
    },
  }));
  useProjectStore.setState({
    currentProject: { ...defaultProject, projectId: "project-1", rootPath: "D:/知识库", name: "Project One" },
    authority: { projectId: "project-1", canonicalRootPath: "D:/知识库", canonicalIdentityKey: "identity", identityRevision: "revision" } as never,
  });
});

describe("task notification routing", () => {
  it("stores the task route in notifications and opens that task when clicked", async () => {
    await notifyTaskEvent({
      eventId: "event-42",
      eventType: "task_failed",
      taskId: "task-42",
      projectId: "project-1",
      timestamp: "2026-06-21T00:00:00Z",
      payload: null,
    });

    expect(sendNotificationMock).toHaveBeenCalledWith(
      expect.objectContaining({
        extra: { taskId: "task-42", eventType: "task_failed" },
      }),
    );

    await handleNotificationAction({
      extra: { taskId: "task-42", eventType: "task_failed" },
    });

    expect(useTaskStore.getState()).toMatchObject({
      drawerOpen: true,
      selectedTaskId: "task-42",
    });
    expect(showMock).toHaveBeenCalledOnce();
    expect(focusMock).toHaveBeenCalledOnce();
  });

  it("notifies workflows only on waiting, completed, and failed transitions", async () => {
    const run = {
      schemaVersion: 1,
      taskId: "workflow-notification-1",
      projectId: "project-1",
      canonicalIdentityKey: "identity",
      identityRevision: "revision",
      kind: "health_check",
      operation: { kind: "built_in" },
      displayStatus: "running",
      scope: { kind: "health_check", mode: "complete" },
      route: null,
      fingerprint: "fingerprint",
      baselineFingerprint: "baseline",
      stages: [],
      currentStageId: null,
      queuePosition: null,
      continuationRequired: false,
      retry: null,
      pendingAction: null,
      result: null,
      error: null,
      startedAt: "2026-08-02T00:00:00Z",
      updatedAt: "2026-08-02T00:00:00Z",
      completedAt: null,
    } satisfies WorkflowRun;
    const event: BackendEvent<WorkflowRun> = {
      eventId: "workflow-event-1",
      eventType: "workflow_updated",
      taskId: run.taskId,
      projectId: run.projectId,
      timestamp: run.updatedAt,
      payload: run,
    };

    await notifyTaskEvent(event);
    expect(sendNotificationMock).not.toHaveBeenCalled();

    for (const [revision, displayStatus] of [["1", "waiting_for_confirmation"], ["1", "waiting_for_confirmation"], ["2", "completed"], ["3", "cancelled"]] as const) {
      const next = { ...run, revision, displayStatus };
      recordWorkflowFacts([next]);
      await notifyTaskEvent({ ...event, payload: next });
    }

    expect(sendNotificationMock).toHaveBeenCalledTimes(2);
    expect(sendNotificationMock.mock.calls.map(([options]) => options.extra.workflowStatus)).toEqual([
      "waiting_for_confirmation",
      "completed",
    ]);
  });
});

function summary(taskId: string, overrides: Partial<WorkflowRunSummary> = {}): WorkflowRunSummary {
  return {
    schemaVersion: 1, revision: "1", sessionId: "session-1", taskId,
    projectId: "project-1", canonicalIdentityKey: "identity", identityRevision: "revision",
    kind: "health_check", operation: { kind: "built_in" }, displayStatus: "completed",
    retry: null, outcome: { kind: "health_check", errorCount: 2, warningCount: 3, infoCount: 0 },
    startedAt: "2026-09-07T00:00:00Z", updatedAt: "2026-09-07T00:01:00Z", completedAt: "2026-09-07T00:01:00Z",
    ...overrides,
  };
}

function workflowEvent(run: WorkflowRunSummary): BackendEvent<WorkflowRunSummary> {
  return { eventId: `${run.taskId}-${run.revision}`, eventType: "workflow_updated", projectId: run.projectId, taskId: run.taskId, timestamp: run.updatedAt, payload: run };
}

describe("accepted Workflow summary notifications", () => {
  it("waits for canonical ownership and handles bounded summary outcomes", async () => {
    const run = summary("owner-first");
    await notifyTaskEvent(workflowEvent(run));
    expect(permissionMock).not.toHaveBeenCalled();
    recordWorkflowFacts([run]);
    await notifyTaskEvent(workflowEvent(run));
    expect(sendNotificationMock).toHaveBeenCalledOnce();
    expect(sendNotificationMock).toHaveBeenCalledWith(expect.objectContaining({ extra: expect.objectContaining({ taskId: run.taskId, workflowStatus: "completed" }) }));
  });

  it("ignores stale revisions, retired sessions, envelope mismatches, and replaced identities", async () => {
    const old = summary("stale-event");
    const current = summary(old.taskId, { revision: "3", sessionId: "session-2" });
    recordWorkflowFacts([old]);
    recordWorkflowFacts([current]);
    for (const run of [old, { ...current, revision: "2" }, { ...current, canonicalIdentityKey: "replacement" }]) {
      await notifyTaskEvent(workflowEvent(run));
    }
    await notifyTaskEvent({ ...workflowEvent(current), taskId: "wrong-task" });
    await notifyTaskEvent({ ...workflowEvent(current), projectId: "wrong-project" });
    expect(permissionMock).not.toHaveBeenCalled();
    await notifyTaskEvent(workflowEvent(current));
    expect(sendNotificationMock).toHaveBeenCalledOnce();
  });

  it.each(["revision", "session", "project", "identity", "settings"])("revalidates %s after waiting for permission", async (change) => {
    let release!: (granted: boolean) => void;
    permissionMock.mockReturnValue(new Promise<boolean>((resolve) => { release = resolve; }));
    const run = summary(`permission-${change}`, { displayStatus: "waiting_for_confirmation", outcome: null });
    recordWorkflowFacts([run]);
    const notifying = notifyTaskEvent(workflowEvent(run));
    await vi.waitFor(() => expect(permissionMock).toHaveBeenCalledOnce());
    if (change === "revision") recordWorkflowFacts([{ ...run, revision: "2", displayStatus: "cancelled" }]);
    if (change === "session") recordWorkflowFacts([{ ...run, sessionId: "session-2" }]);
    if (change === "project") useProjectStore.setState({ currentProject: { ...defaultProject, projectId: "project-2", rootPath: "D:/other" } });
    if (change === "identity") useProjectStore.setState({ authority: { ...useProjectStore.getState().authority!, identityRevision: "replacement" } });
    if (change === "settings") useSettingsStore.setState((state) => ({ settings: { ...state.settings, systemNotifications: { ...state.settings.systemNotifications, onConfirmationNeeded: false } } }));
    release(true);
    await notifying;
    expect(sendNotificationMock).not.toHaveBeenCalled();
  });

  it("delivers the latest same-status revision when an earlier revision is waiting on permission", async () => {
    let release!: (granted: boolean) => void;
    permissionMock.mockReturnValue(new Promise<boolean>((resolve) => { release = resolve; }));
    const first = summary("permission-revision-advance", { displayStatus: "waiting_for_confirmation", outcome: null });
    recordWorkflowFacts([first]);
    const notifyingFirst = notifyTaskEvent(workflowEvent(first));
    await vi.waitFor(() => expect(permissionMock).toHaveBeenCalledOnce());
    const latest = { ...first, revision: "2" };
    recordWorkflowFacts([latest]);
    const notifyingLatest = notifyTaskEvent(workflowEvent(latest));
    release(true);
    await Promise.all([notifyingFirst, notifyingLatest]);
    expect(sendNotificationMock).toHaveBeenCalledOnce();
    expect(permissionMock).toHaveBeenCalledOnce();
  });

  it("does not load notification permissions when identity changes during the Workflow chunk await", async () => {
    const run = summary("notification-cold-identity");
    recordWorkflowFacts([run]);
    const notifying = notifyTaskEvent(workflowEvent(run));
    useProjectStore.setState({ authority: { ...useProjectStore.getState().authority!, identityRevision: "replacement" } });
    await notifying;
    expect(permissionMock).not.toHaveBeenCalled();
    expect(sendNotificationMock).not.toHaveBeenCalled();
  });

  it("reserves a status while its send is pending across newer same-status revisions", async () => {
    let release!: () => void;
    sendNotificationMock.mockReturnValue(new Promise<void>((resolve) => { release = resolve; }));
    const first = summary("send-in-flight");
    recordWorkflowFacts([first]);
    const sending = notifyTaskEvent(workflowEvent(first));
    await vi.waitFor(() => expect(sendNotificationMock).toHaveBeenCalledOnce());
    const latest = { ...first, revision: "2" };
    recordWorkflowFacts([latest]);
    await notifyTaskEvent(workflowEvent(latest));
    expect(sendNotificationMock).toHaveBeenCalledOnce();
    release();
    await sending;
  });

  it("allows retry after notification delivery fails", async () => {
    sendNotificationMock.mockRejectedValueOnce(new Error("notification unavailable")).mockResolvedValue(undefined);
    const run = summary("notification-retry");
    recordWorkflowFacts([run]);
    await notifyTaskEvent(workflowEvent(run));
    await notifyTaskEvent(workflowEvent(run));
    expect(sendNotificationMock).toHaveBeenCalledTimes(2);
  });

  it("fails closed on nonnumeric summary counts before asking for permission", async () => {
    const run = summary("unsafe-summary", { outcome: { kind: "health_check", errorCount: "private text", warningCount: 0 } as never });
    recordWorkflowFacts([run]);
    await notifyTaskEvent(workflowEvent(run));
    expect(permissionMock).not.toHaveBeenCalled();
    expect(sendNotificationMock).not.toHaveBeenCalled();
  });

  it("allows a fresh backend session to notify for a recovered task", async () => {
    const first = summary("recovered-task");
    recordWorkflowFacts([first]);
    await notifyTaskEvent(workflowEvent(first));
    const recovered = { ...first, sessionId: "session-2", revision: "1" };
    recordWorkflowFacts([recovered]);
    await notifyTaskEvent(workflowEvent(recovered));
    expect(sendNotificationMock).toHaveBeenCalledTimes(2);
  });
});
