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
import { handleNotificationAction, notifyTaskEvent } from "./notifications";

beforeEach(() => {
  sendNotificationMock.mockReset();
  showMock.mockReset().mockResolvedValue(undefined);
  focusMock.mockReset().mockResolvedValue(undefined);
  useTaskStore.setState({ drawerOpen: false, selectedTaskId: null });
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
});
