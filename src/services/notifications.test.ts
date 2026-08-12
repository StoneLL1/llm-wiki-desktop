import { beforeEach, describe, expect, it, vi } from "vitest";

const sendNotificationMock = vi.hoisted(() => vi.fn());
const showMock = vi.hoisted(() => vi.fn());
const focusMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(true),
  requestPermission: vi.fn().mockResolvedValue("granted"),
  sendNotification: sendNotificationMock,
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ show: showMock, setFocus: focusMock }),
}));

import { useTaskStore } from "../stores/taskStore";
import { useSettingsStore } from "../stores/settingsStore";
import { defaultProject, useProjectStore } from "../stores/projectStore";
import type { WorkflowRun } from "../types/workflow";
import type { BackendEvent } from "../types/task";
import { handleNotificationAction, notifyTaskEvent } from "./notifications";

beforeEach(() => {
  sendNotificationMock.mockReset();
  showMock.mockReset().mockResolvedValue(undefined);
  focusMock.mockReset().mockResolvedValue(undefined);
  useTaskStore.setState({ drawerOpen: false, selectedTaskId: null });
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
    currentProject: { ...defaultProject, projectId: "project-1", name: "Project One" },
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

    await notifyTaskEvent({ ...event, payload: { ...run, displayStatus: "waiting_for_confirmation" as const } });
    await notifyTaskEvent({ ...event, payload: { ...run, displayStatus: "waiting_for_confirmation" as const } });
    await notifyTaskEvent({ ...event, payload: { ...run, displayStatus: "completed" as const } });
    await notifyTaskEvent({ ...event, payload: { ...run, displayStatus: "cancelled" as const } });

    expect(sendNotificationMock).toHaveBeenCalledTimes(2);
    expect(sendNotificationMock.mock.calls.map(([options]) => options.extra.workflowStatus)).toEqual([
      "waiting_for_confirmation",
      "completed",
    ]);
  });
});
