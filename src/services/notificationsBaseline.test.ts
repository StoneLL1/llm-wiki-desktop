import { beforeEach, describe, expect, it, vi } from "vitest";

const notificationMocks = vi.hoisted(() => ({
  isPermissionGranted: vi.fn(),
  requestPermission: vi.fn(),
  sendNotification: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: notificationMocks.isPermissionGranted,
  requestPermission: notificationMocks.requestPermission,
  sendNotification: notificationMocks.sendNotification,
  onAction: vi.fn().mockResolvedValue({ unregister: vi.fn() }),
}));

import { makeDrawerTask, WORKFLOW_BASELINE_SIZES } from "../features/workflows/workflowBaselineFixtures";
import { useSettingsStore } from "../stores/settingsStore";
import type { BackendEvent } from "../types/task";
import {
  invalidateNotificationPermissionEpoch,
  notifyTaskEvent,
  requestNotificationPermissionFromUser,
} from "./notifications";

describe("Workflows Batch 0 notification permission counters", () => {
  beforeEach(() => {
    invalidateNotificationPermissionEpoch();
    notificationMocks.isPermissionGranted.mockReset().mockResolvedValue(false);
    notificationMocks.requestPermission.mockReset().mockResolvedValue("denied");
    notificationMocks.sendNotification.mockReset();
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
  });

  it("coalesces a denied eligible burst to one permission check per epoch without background requests", async () => {
    const notifications = [];
    for (let index = 0; index < WORKFLOW_BASELINE_SIZES.workflowEvents; index += 1) {
      const task = {
        ...makeDrawerTask(index),
        id: `completed-${index}`,
        status: "succeeded" as const,
        completedAt: "2026-08-09T00:03:19Z",
      };
      notifications.push(notifyTaskEvent({
        eventId: `completed-event-${index}`,
        eventType: "task_completed",
        projectId: task.projectId,
        taskId: task.id,
        timestamp: task.completedAt,
        payload: task,
      } satisfies BackendEvent));
    }
    await Promise.all(notifications);

    expect(notificationMocks.isPermissionGranted).toHaveBeenCalledTimes(1);
    expect(notificationMocks.requestPermission).not.toHaveBeenCalled();
    expect(notificationMocks.sendNotification).not.toHaveBeenCalled();

    invalidateNotificationPermissionEpoch();
    await notifyTaskEvent({
      eventId: "next-epoch",
      eventType: "task_completed",
      projectId: "project-a",
      taskId: "next-task",
      timestamp: "2026-08-09T00:04:00Z",
      payload: { ...makeDrawerTask(999), id: "next-task", status: "succeeded" },
    } satisfies BackendEvent);
    expect(notificationMocks.isPermissionGranted).toHaveBeenCalledTimes(2);
    expect(notificationMocks.requestPermission).not.toHaveBeenCalled();
  });

  it("does not touch permission for 1,000 ineligible events", async () => {
    await Promise.all(Array.from({ length: WORKFLOW_BASELINE_SIZES.drawerEvents }, (_, index) =>
      notifyTaskEvent({
        eventId: `progress-${index}`,
        eventType: "task_updated",
        projectId: "project-a",
        taskId: `task-${index}`,
        timestamp: "2026-08-09T00:00:00Z",
        payload: { ...makeDrawerTask(index), status: "running" },
      } satisfies BackendEvent),
    ));

    expect(notificationMocks.isPermissionGranted).not.toHaveBeenCalled();
    expect(notificationMocks.requestPermission).not.toHaveBeenCalled();
    expect(notificationMocks.sendNotification).not.toHaveBeenCalled();
  });

  it("rejects an eligible malformed workflow payload before touching permission", async () => {
    await notifyTaskEvent({
      eventId: "malformed-workflow",
      eventType: "workflow_updated",
      projectId: "project-a",
      taskId: "malformed-task",
      timestamp: "2026-08-09T00:00:00Z",
      payload: {
        taskId: "malformed-task",
        projectId: "project-a",
        kind: "unknown_workflow",
        displayStatus: "failed",
      },
    } satisfies BackendEvent);

    expect(notificationMocks.isPermissionGranted).not.toHaveBeenCalled();
    expect(notificationMocks.requestPermission).not.toHaveBeenCalled();
    expect(notificationMocks.sendNotification).not.toHaveBeenCalled();
  });

  it("fails closed when a workflow result is missing or contains a non-numeric count", async () => {
    const base = {
      taskId: "unsafe-workflow",
      projectId: "project-a",
      kind: "update_wiki",
      displayStatus: "completed",
    };
    await expect(notifyTaskEvent({
      eventId: "missing-result",
      eventType: "workflow_updated",
      projectId: "project-a",
      taskId: base.taskId,
      timestamp: "2026-08-09T00:00:00Z",
      payload: base,
    } satisfies BackendEvent)).resolves.toBeUndefined();
    await notifyTaskEvent({
      eventId: "unsafe-count",
      eventType: "workflow_updated",
      projectId: "project-a",
      taskId: base.taskId,
      timestamp: "2026-08-09T00:00:01Z",
      payload: {
        ...base,
        result: { kind: "update_wiki", created: 1, updated: 1, deleted: 0, conflicted: "C:/private/model output" },
      },
    } satisfies BackendEvent);

    expect(notificationMocks.isPermissionGranted).not.toHaveBeenCalled();
    expect(notificationMocks.sendNotification).not.toHaveBeenCalled();
  });

  it("never copies raw task errors or paths into a system notification", async () => {
    invalidateNotificationPermissionEpoch();
    notificationMocks.isPermissionGranted.mockReset().mockResolvedValue(true);
    const task = {
      ...makeDrawerTask(7),
      id: "sensitive-failure",
      status: "failed" as const,
      error: {
        code: "MODEL_FAILURE",
        message: "C:/Users/private/wiki.md contained secret model output",
        details: null,
      },
    };
    await notifyTaskEvent({
      eventId: "sensitive-failure",
      eventType: "task_failed",
      projectId: task.projectId,
      taskId: task.id,
      timestamp: "2026-08-09T00:00:00Z",
      payload: task,
    } satisfies BackendEvent);

    expect(notificationMocks.isPermissionGranted).toHaveBeenCalledTimes(1);
    const options = notificationMocks.sendNotification.mock.calls[0]?.[0];
    expect(options?.body).not.toContain("C:/Users/private/wiki.md");
    expect(options?.body).not.toContain("secret model output");
  });

  it("coalesces permission requests only when initiated by an explicit user action", async () => {
    const results = await Promise.all(Array.from(
      { length: WORKFLOW_BASELINE_SIZES.workflowEvents },
      () => requestNotificationPermissionFromUser(),
    ));

    expect(results.every((granted) => granted === false)).toBe(true);
    expect(notificationMocks.isPermissionGranted).toHaveBeenCalledTimes(1);
    expect(notificationMocks.requestPermission).toHaveBeenCalledTimes(1);
    expect(notificationMocks.sendNotification).not.toHaveBeenCalled();
  });

  it("keeps one OS permission request in flight across an epoch invalidation", async () => {
    let resolveRequest!: (result: "granted") => void;
    notificationMocks.requestPermission.mockReset().mockReturnValue(
      new Promise<"granted">((resolve) => { resolveRequest = resolve; }),
    );
    notificationMocks.isPermissionGranted
      .mockReset()
      .mockResolvedValueOnce(false)
      .mockResolvedValueOnce(true);

    const first = requestNotificationPermissionFromUser();
    await vi.waitFor(() => expect(notificationMocks.requestPermission).toHaveBeenCalledTimes(1));
    invalidateNotificationPermissionEpoch();
    const second = requestNotificationPermissionFromUser();
    await Promise.resolve();
    expect(notificationMocks.requestPermission).toHaveBeenCalledTimes(1);
    resolveRequest("granted");

    await expect(Promise.all([first, second])).resolves.toEqual([false, true]);
    expect(notificationMocks.requestPermission).toHaveBeenCalledTimes(1);
    expect(notificationMocks.isPermissionGranted).toHaveBeenCalledTimes(2);
  });

  it("suppresses a granted permission result invalidated while its check is in flight", async () => {
    let resolvePermission!: (granted: boolean) => void;
    notificationMocks.isPermissionGranted.mockReset().mockReturnValueOnce(
      new Promise<boolean>((resolve) => { resolvePermission = resolve; }),
    ).mockResolvedValue(true);
    const event = {
      eventId: "stale-permission",
      eventType: "task_completed",
      projectId: "project-a",
      taskId: "task-a",
      timestamp: "2026-08-09T00:04:00Z",
      payload: { ...makeDrawerTask(1), id: "task-a", status: "succeeded" },
    } satisfies BackendEvent;

    const staleNotification = notifyTaskEvent(event);
    invalidateNotificationPermissionEpoch();
    resolvePermission(true);
    await staleNotification;

    expect(notificationMocks.sendNotification).not.toHaveBeenCalled();
    await notifyTaskEvent({ ...event, eventId: "current-permission" });
    expect(notificationMocks.isPermissionGranted).toHaveBeenCalledTimes(2);
    expect(notificationMocks.sendNotification).toHaveBeenCalledTimes(1);
  });
});
