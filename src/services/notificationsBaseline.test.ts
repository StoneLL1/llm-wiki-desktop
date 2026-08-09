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
import { notifyTaskEvent } from "./notifications";

describe("Workflows Batch 0 notification permission counters", () => {
  beforeEach(() => {
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

  it("records the pre-fix denied-permission path for a 200-event eligible burst", async () => {
    for (let index = 0; index < WORKFLOW_BASELINE_SIZES.workflowEvents; index += 1) {
      const task = {
        ...makeDrawerTask(index),
        id: `completed-${index}`,
        status: "succeeded" as const,
        completedAt: "2026-08-09T00:03:19Z",
      };
      await notifyTaskEvent({
        eventId: `completed-event-${index}`,
        eventType: "task_completed",
        projectId: task.projectId,
        taskId: task.id,
        timestamp: task.completedAt,
        payload: task,
      } satisfies BackendEvent);
    }

    expect(notificationMocks.isPermissionGranted).toHaveBeenCalledTimes(
      WORKFLOW_BASELINE_SIZES.workflowEvents,
    );
    expect(notificationMocks.requestPermission).toHaveBeenCalledTimes(
      WORKFLOW_BASELINE_SIZES.workflowEvents,
    );
    expect(notificationMocks.sendNotification).not.toHaveBeenCalled();
  });
});
