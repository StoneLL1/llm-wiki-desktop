import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { defaultSettings } from "../../stores/settingsStore";
import type { NotificationPermissionState } from "../../services/notifications";

const mocks = vi.hoisted(() => ({ read: vi.fn(), send: vi.fn() }));
vi.mock("../../services/notifications", () => ({ readNotificationPermission: mocks.read, sendTestNotification: mocks.send }));

import { BackgroundTaskSettings } from "./BackgroundTaskSettings";

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((complete) => { resolve = complete; });
  return { promise, resolve };
}

function renderSettings(requestPermission = vi.fn().mockResolvedValue(true)) {
  return render(<BackgroundTaskSettings closeBehavior="ask" systemNotifications={defaultSettings.systemNotifications} onChangeCloseBehavior={vi.fn()} onChangeSystemNotification={vi.fn().mockResolvedValue(undefined)} onRequestNotificationPermission={requestPermission} />);
}

beforeEach(() => { mocks.read.mockReset().mockResolvedValue("granted"); mocks.send.mockReset().mockResolvedValue(true); });

describe("notification permission result ownership", () => {
  it("does not let an earlier permission read overwrite successful explicit authorization", async () => {
    const oldRead = deferred<NotificationPermissionState>();
    mocks.read.mockReturnValue(oldRead.promise);
    renderSettings();
    fireEvent.click(screen.getByRole("button", { name: "Check or request permission" }));
    expect(await screen.findByText("Notification service available")).toBeInTheDocument();
    await act(async () => { oldRead.resolve("denied"); });
    expect(screen.getByText("Notification service available")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Send test notification" })).toBeEnabled();
  });

  it("retains the explicit authorization result when focus returns during the prompt", async () => {
    const permission = deferred<boolean>();
    mocks.read.mockResolvedValue("denied");
    renderSettings(vi.fn().mockReturnValue(permission.promise));
    await screen.findByText("Notification service not authorized");
    fireEvent.click(screen.getByRole("button", { name: "Check or request permission" }));
    fireEvent.focus(window);
    expect(mocks.read).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("button", { name: "Check or request permission" })).toBeDisabled();
    await act(async () => { permission.resolve(true); });
    expect(screen.getByText("Notification service available")).toBeInTheDocument();
    fireEvent.focus(window);
    await waitFor(() => expect(mocks.read).toHaveBeenCalledTimes(2));
    expect(await screen.findByText("Notification service not authorized")).toBeInTheDocument();
  });

  it("does not apply results from a panel that was unmounted", async () => {
    const oldRead = deferred<NotificationPermissionState>();
    mocks.read.mockReturnValueOnce(oldRead.promise).mockResolvedValue("granted");
    const previous = renderSettings();
    previous.unmount();
    renderSettings();
    await screen.findByText("Notification service available");
    await act(async () => { oldRead.resolve("denied"); });
    expect(screen.getByText("Notification service available")).toBeInTheDocument();
  });
});
