import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import "../../i18n";
import type { AppUpdateState, GlobalUpdatePreferences } from "../../types/update";

const api = vi.hoisted(() => ({
  getAppSummary: vi.fn(),
  getUpdateState: vi.fn(),
  getGlobalUpdatePreferences: vi.fn(),
  saveGlobalUpdatePreferences: vi.fn(),
  checkAppUpdate: vi.fn(),
  downloadAppUpdate: vi.fn(),
  cancelAppUpdateDownload: vi.fn(),
  installAppUpdate: vi.fn(),
  restartAppAfterUpdate: vi.fn(),
  dismissAppUpdate: vi.fn(),
}));

vi.mock("../../services/updateApi", () => api);

import { UpdateController } from "../../components/app/UpdateController";
import { useUpdateStore } from "../../stores/updateStore";

const idle: AppUpdateState = {
  phase: "idle",
  offer: null,
  downloadedBytes: 0,
  totalBytes: null,
  error: null,
};

const preferences: GlobalUpdatePreferences = {
  checkUpdates: true,
  updateFrequency: "daily",
  autoDownloadUpdates: false,
  promptChangelogBeforeInstall: true,
  lastCheckedAt: null,
  dismissedOfferId: null,
  dismissedVersion: null,
};

const available: AppUpdateState = {
  phase: "available",
  offer: {
    offerId: "offer-1",
    currentVersion: "0.1.0",
    version: "0.2.0",
    target: "windows",
    arch: "x86_64",
    notes: "Safe release",
    publishedAt: "2026-08-21T00:00:00Z",
    createdAtUnixSeconds: 1,
    expiresAtUnixSeconds: 2,
  },
  downloadedBytes: 0,
  totalBytes: null,
  error: null,
};

beforeEach(() => {
  vi.useRealTimers();
  vi.clearAllMocks();
  Object.defineProperty(window, "__TAURI_INTERNALS__", { configurable: true, value: {} });
  useUpdateStore.getState().resetForTests();
  api.getAppSummary.mockResolvedValue({ name: "LLM Wiki Desktop", version: "0.1.0" });
  api.getUpdateState.mockResolvedValue(idle);
  api.getGlobalUpdatePreferences.mockResolvedValue(preferences);
  api.checkAppUpdate.mockResolvedValue(idle);
});

describe("UpdateController", () => {
  it("checks globally when no project is open", async () => {
    vi.useFakeTimers();
    render(<UpdateController />);

    await act(async () => { await Promise.resolve(); });
    await act(async () => { await vi.advanceTimersByTimeAsync(750); });

    expect(api.checkAppUpdate).toHaveBeenCalledTimes(1);
  });

  it("shows an available offer", () => {
    useUpdateStore.setState({ backendState: available, uiStatus: "available", dialogOpen: true, initialized: true });
    render(<UpdateController />);
    expect(screen.getByText("0.2.0")).toBeVisible();
  });

  it("shows downloading progress", () => {
    useUpdateStore.setState({
      backendState: { ...available, phase: "downloading", downloadedBytes: 25, totalBytes: 100 },
      uiStatus: "downloading",
      dialogOpen: true,
      initialized: true,
    });
    render(<UpdateController />);
    expect(screen.getByRole("progressbar", { name: "Download progress" })).toHaveValue(25);
  });

  it("cancels a download into the paused state", async () => {
    api.cancelAppUpdateDownload.mockResolvedValue({ ...available, phase: "cancelled" });
    useUpdateStore.setState({
      backendState: { ...available, phase: "downloading" },
      uiStatus: "downloading",
      dialogOpen: true,
      initialized: true,
    });
    render(<UpdateController />);

    fireEvent.click(screen.getByRole("button", { name: "Cancel download" }));
    await act(async () => { await Promise.resolve(); });
    expect(useUpdateStore.getState().uiStatus).toBe("paused_or_cancelled");
  });

  it("offers installation when ready to install", () => {
    useUpdateStore.setState({
      backendState: { ...available, phase: "downloaded" },
      uiStatus: "ready_to_install",
      dialogOpen: true,
      initialized: true,
    });
    render(<UpdateController />);
    expect(screen.getByRole("button", { name: "Review installation" })).toBeEnabled();
  });

  it("renders the installing state", () => {
    useUpdateStore.setState({
      backendState: { ...available, phase: "installing" },
      uiStatus: "installing",
      dialogOpen: true,
      initialized: true,
    });
    render(<UpdateController />);
    expect(screen.getByText("Installing the verified update…")).toBeVisible();
  });
});
