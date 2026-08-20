import { beforeEach, describe, expect, it, vi } from "vitest";

import type { AppUpdateState, GlobalUpdatePreferences } from "../types/update";

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

vi.mock("../services/updateApi", () => api);

import { updateCheckDue, useUpdateStore } from "./updateStore";
import { defaultProject, useProjectStore } from "./projectStore";

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
  lastCheckedAt: "2026-08-21T00:00:00Z",
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
  vi.clearAllMocks();
  Object.defineProperty(window, "__TAURI_INTERNALS__", { configurable: true, value: {} });
  useUpdateStore.getState().resetForTests();
  api.getAppSummary.mockResolvedValue({ name: "LLM Wiki Desktop", version: "0.1.0" });
  api.getUpdateState.mockResolvedValue(idle);
  api.getGlobalUpdatePreferences.mockResolvedValue(preferences);
});

describe("updateStore", () => {
  it("does not let a late startup snapshot overwrite a completed manual check", async () => {
    let resolveStartupPreferences!: (value: GlobalUpdatePreferences) => void;
    api.getGlobalUpdatePreferences
      .mockReturnValueOnce(new Promise<GlobalUpdatePreferences>((done) => {
        resolveStartupPreferences = done;
      }))
      .mockResolvedValue(preferences);
    api.checkAppUpdate.mockResolvedValue(available);

    const initialization = useUpdateStore.getState().initialize();
    await vi.waitFor(() => expect(api.getUpdateState).toHaveBeenCalledTimes(1));
    await useUpdateStore.getState().checkNow();
    resolveStartupPreferences(preferences);
    await initialization;

    expect(useUpdateStore.getState().backendState.offer?.offerId).toBe("offer-1");
    expect(useUpdateStore.getState().uiStatus).toBe("available");
  });

  it("deduplicates simultaneous checks and reports an up-to-date result", async () => {
    let resolve!: (state: AppUpdateState) => void;
    api.checkAppUpdate.mockReturnValue(new Promise<AppUpdateState>((done) => { resolve = done; }));

    const first = useUpdateStore.getState().checkNow();
    const second = useUpdateStore.getState().checkNow();
    expect(api.checkAppUpdate).toHaveBeenCalledTimes(1);

    resolve(idle);
    await Promise.all([first, second]);
    expect(useUpdateStore.getState().uiStatus).toBe("up_to_date");
  });

  it("keeps an available offer across project-independent state changes", async () => {
    useProjectStore.setState({
      currentProject: { ...defaultProject, projectId: "project-a", rootPath: "D:/a" },
    });
    api.checkAppUpdate.mockResolvedValue(available);
    await useUpdateStore.getState().checkNow();

    expect(useUpdateStore.getState().backendState.offer?.offerId).toBe("offer-1");
    useProjectStore.setState({
      currentProject: { ...defaultProject, projectId: "project-b", rootPath: "D:/b" },
    });
    expect(useUpdateStore.getState().backendState.offer?.version).toBe("0.2.0");
  });

  it("streams progress and permits a real retry after a failed download", async () => {
    useUpdateStore.setState({ backendState: available, uiStatus: "available" });
    api.downloadAppUpdate
      .mockImplementationOnce(async (_offerId, onProgress) => {
        onProgress({ phase: "downloading", downloadedBytes: 50, totalBytes: 100 });
        throw { code: "UPDATE_DOWNLOAD_FAILED", recoverable: true };
      })
      .mockImplementationOnce(async (_offerId, onProgress) => {
        onProgress({ phase: "downloading", downloadedBytes: 100, totalBytes: 100 });
        return { ...available, phase: "downloaded", downloadedBytes: 100, totalBytes: 100 };
      });

    await expect(useUpdateStore.getState().download()).rejects.toMatchObject({ code: "UPDATE_DOWNLOAD_FAILED" });
    expect(useUpdateStore.getState().uiStatus).toBe("error");
    api.checkAppUpdate.mockResolvedValue(available);
    await useUpdateStore.getState().retry();
    expect(api.checkAppUpdate).toHaveBeenCalledTimes(1);
    expect(useUpdateStore.getState().uiStatus).toBe("ready_to_install");

  });

  it("re-checks an expired cancelled offer before downloading again", async () => {
    useUpdateStore.setState({ backendState: available, uiStatus: "available" });
    api.cancelAppUpdateDownload.mockResolvedValue({ ...available, phase: "cancelled" });
    await useUpdateStore.getState().cancelDownload();
    expect(api.cancelAppUpdateDownload).toHaveBeenCalledWith("offer-1");
    expect(useUpdateStore.getState().uiStatus).toBe("paused_or_cancelled");

    api.checkAppUpdate.mockResolvedValue({
      ...available,
      offer: { ...available.offer!, offerId: "offer-2" },
    });
    api.downloadAppUpdate.mockResolvedValue({
      ...available,
      phase: "downloaded",
      offer: { ...available.offer!, offerId: "offer-2" },
    });
    await useUpdateStore.getState().download();
    expect(api.checkAppUpdate).toHaveBeenCalledTimes(1);
    expect(api.downloadAppUpdate).toHaveBeenCalledWith("offer-2", expect.any(Function));
  });

  it("never auto-downloads unless the global preference explicitly enables it", async () => {
    api.checkAppUpdate.mockResolvedValue(available);
    await useUpdateStore.getState().checkNow();
    expect(api.downloadAppUpdate).not.toHaveBeenCalled();

    api.getGlobalUpdatePreferences.mockResolvedValue({ ...preferences, autoDownloadUpdates: true });
    api.downloadAppUpdate.mockResolvedValue({ ...available, phase: "downloaded" });
    await useUpdateStore.getState().checkNow();
    expect(api.downloadAppUpdate).toHaveBeenCalledTimes(1);
    expect(useUpdateStore.getState().dialogOpen).toBe(true);
  });

  it("deduplicates repeated installation consent", async () => {
    let resolve!: (state: AppUpdateState) => void;
    api.installAppUpdate.mockReturnValue(new Promise<AppUpdateState>((done) => { resolve = done; }));
    useUpdateStore.setState({
      backendState: { ...available, phase: "downloaded" },
      uiStatus: "ready_to_install",
      installReviewIntent: "install",
      installGuard: {
        blockers: [],
        safeRunningTaskCount: 0,
        request: {
          unsavedEditor: false,
          importCommitActive: false,
          pendingUserConfirmation: false,
        },
      },
    });

    const first = useUpdateStore.getState().confirmInstallOrRestart();
    const second = useUpdateStore.getState().confirmInstallOrRestart();
    await vi.waitFor(() => expect(api.installAppUpdate).toHaveBeenCalledTimes(1));
    expect(useUpdateStore.getState().uiStatus).toBe("installing");

    resolve({ ...available, phase: "installed" });
    await Promise.all([first, second]);
    expect(useUpdateStore.getState().uiStatus).toBe("restart_required");
  });
});

describe("updateCheckDue", () => {
  it("respects disabled, daily, weekly, and never frequencies", () => {
    const now = Date.parse("2026-08-21T12:00:00Z");
    expect(updateCheckDue({ ...preferences, checkUpdates: false }, now)).toBe(false);
    expect(updateCheckDue({ ...preferences, updateFrequency: "never" }, now)).toBe(false);
    expect(updateCheckDue({ ...preferences, lastCheckedAt: "2026-08-20T11:59:00Z" }, now)).toBe(true);
    expect(updateCheckDue({ ...preferences, updateFrequency: "weekly", lastCheckedAt: "2026-08-15T12:00:00Z" }, now)).toBe(false);
  });
});
