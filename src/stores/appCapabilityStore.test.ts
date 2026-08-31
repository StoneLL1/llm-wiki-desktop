import { beforeEach, describe, expect, it, vi } from "vitest";

import type { AppCapabilityView } from "../types/appCapability";
import type { BackendTask } from "../types/task";

const api = vi.hoisted(() => ({
  list: vi.fn(),
  install: vi.fn(),
  resume: vi.fn(),
  cancel: vi.fn(),
}));
const appTasks = vi.hoisted(() => ({ list: vi.fn() }));

vi.mock("../services/appCapabilityApi", () => ({
  listAppCapabilities: api.list,
  installAppCapability: api.install,
  resumeAppCapabilityInstall: api.resume,
  cancelAppCapabilityInstall: api.cancel,
  pauseAppCapabilityInstall: vi.fn(),
}));
vi.mock("../services/appTaskClient", () => ({ listAppTasks: appTasks.list }));

import {
  appCapabilityPrimaryAction,
  matchesAppCapabilityStatus,
  useAppCapabilityStore,
} from "./appCapabilityStore";
import { useTaskStore } from "./taskStore";

function capability(overrides: Partial<AppCapabilityView> = {}): AppCapabilityView {
  return {
    capabilityId: "browser-runtime",
    nameKey: "importV2.capabilityName.browser-runtime",
    purposeKey: "importV2.capabilityPurpose.web",
    category: "web",
    routes: ["web.generic.browser"],
    formats: ["html"],
    platformContentTypes: ["web_page"],
    targetTriple: "x86_64-pc-windows-msvc",
    publisherKeyId: "llm-wiki-capability-v1",
    sourceDomain: "github.com",
    targetVersion: "1.4.0",
    acknowledgementVersion: "ack-v1",
    installAllowed: true,
    distribution: { state: "published" },
    installation: { state: "absent" },
    operation: {},
    update: { state: "none" },
    displayState: "install_available",
    compressedBytes: 12_000,
    installedBytes: 45_000,
    modelBytes: 0,
    licenseExpression: "Apache-2.0",
    thirdPartyNotices: [],
    runtimeNetwork: true,
    runtimeSubprocess: true,
    runtimeFilesystem: ["app-capability-dir"],
    currentProjectWaitingCount: 0,
    ...overrides,
  };
}

function globalTask(): BackendTask {
  return {
    id: "capability-task-a",
    taskType: "capability_install",
    projectId: null,
    operation: {
      kind: "app_capability_install",
      capabilityId: "browser-runtime",
      version: "1.4.0",
      targetTriple: "x86_64-pc-windows-msvc",
      archiveIdentity: "archive-a",
    },
    title: "Install browser runtime",
    status: "queued",
    progress: { current: 0, total: 12_000, label: "capability.downloading" },
    startedAt: "2026-08-30T00:00:00Z",
    updatedAt: "2026-08-30T00:00:00Z",
    completedAt: null,
    cancellable: true,
    logPath: null,
    result: null,
    error: null,
  };
}

beforeEach(() => {
  api.list.mockReset();
  api.install.mockReset();
  api.resume.mockReset();
  api.cancel.mockReset();
  appTasks.list.mockReset();
  appTasks.list.mockResolvedValue([]);
  useAppCapabilityStore.getState().resetForTests();
  useTaskStore.setState({ taskById: {}, tasks: [] });
  Object.defineProperty(window, "__TAURI_INTERNALS__", { value: {}, configurable: true });
});

describe("appCapabilityStore", () => {
  it("loads the app-global catalog without requiring an open project", async () => {
    api.list.mockResolvedValueOnce([capability()]);

    await useAppCapabilityStore.getState().initialize();

    expect(api.list).toHaveBeenCalledOnce();
    expect(useAppCapabilityStore.getState().capabilities[0].capabilityId).toBe("browser-runtime");
  });

  it("keeps a forced project-context refresh authoritative over a late snapshot", async () => {
    let resolveFirst!: (value: AppCapabilityView[]) => void;
    let resolveSecond!: (value: AppCapabilityView[]) => void;
    api.list
      .mockImplementationOnce(() => new Promise<AppCapabilityView[]>((resolve) => { resolveFirst = resolve; }))
      .mockImplementationOnce(() => new Promise<AppCapabilityView[]>((resolve) => { resolveSecond = resolve; }));

    const first = useAppCapabilityStore.getState().refresh();
    const second = useAppCapabilityStore.getState().refresh(true);
    resolveSecond([capability({ currentProjectWaitingCount: 2 })]);
    await second;
    resolveFirst([capability({ currentProjectWaitingCount: 7 })]);
    await first;

    expect(useAppCapabilityStore.getState().capabilities[0].currentProjectWaitingCount).toBe(2);
  });

  it("deduplicates install confirmations and registers the returned global task", async () => {
    const task = globalTask();
    api.install.mockResolvedValue(task);
    api.list.mockResolvedValue([capability({ operation: { state: "queued", taskId: task.id }, activeTaskId: task.id })]);
    useAppCapabilityStore.setState({ capabilities: [capability()], initialized: true });

    const first = useAppCapabilityStore.getState().confirmInstall("browser-runtime");
    const second = useAppCapabilityStore.getState().confirmInstall("browser-runtime");
    await Promise.all([first, second]);

    expect(api.install).toHaveBeenCalledOnce();
    expect(api.install).toHaveBeenCalledWith({
      capabilityId: "browser-runtime",
      expectedVersion: "1.4.0",
      acknowledgementVersion: "ack-v1",
    });
    expect(useTaskStore.getState().taskById[task.id]).toMatchObject({ projectId: null });
  });

  it("derives only truthful actions and orthogonal filters", () => {
    const unpublished = capability({
      installAllowed: false,
      distribution: { state: "not_published_for_target" },
      displayState: "not_published_for_target",
    });
    const paused = capability({ operation: { state: "paused", taskId: "task-a" } });
    const updating = capability({
      installation: { state: "healthy", healthyVersion: "1.3.0" },
      update: { state: "available", availableVersion: "1.4.0" },
      displayState: "update_available",
    });
    const failed = capability({ operation: { state: "failed", errorCode: "CAPABILITY_INSTALL_DISK_FULL" } });
    const catalogEmpty = capability({
      installAllowed: false,
      distribution: { state: "source_catalog_empty", errorCode: "CAPABILITY_CATALOG_UNAVAILABLE" },
      displayState: "catalog_unavailable",
    });

    expect(appCapabilityPrimaryAction(unpublished)).toBe("details");
    expect(matchesAppCapabilityStatus(unpublished, "unpublished")).toBe(true);
    expect(appCapabilityPrimaryAction(paused)).toBe("continue");
    expect(matchesAppCapabilityStatus(paused, "active")).toBe(true);
    expect(appCapabilityPrimaryAction(updating)).toBe("update");
    expect(matchesAppCapabilityStatus(updating, "updating")).toBe(true);
    expect(appCapabilityPrimaryAction(failed)).toBe("retry");
    expect(matchesAppCapabilityStatus(failed, "attention")).toBe(true);
    expect(appCapabilityPrimaryAction(catalogEmpty)).toBe("details");
    expect(matchesAppCapabilityStatus(catalogEmpty, "attention")).toBe(true);
  });

  it("hydrates a catalog-owned active task before continuing it", async () => {
    const task = { ...globalTask(), status: "interrupted" as const };
    appTasks.list.mockResolvedValue([task]);
    api.resume.mockResolvedValue(task);
    api.list.mockResolvedValue([capability({ operation: { state: "paused", taskId: task.id }, activeTaskId: task.id })]);
    useAppCapabilityStore.setState({
      initialized: true,
      capabilities: [capability({ operation: { state: "paused", taskId: task.id }, activeTaskId: task.id })],
    });

    await useAppCapabilityStore.getState().continueInstall("browser-runtime");

    expect(appTasks.list).toHaveBeenCalledOnce();
    expect(api.resume).toHaveBeenCalledWith({
      taskId: task.id,
      taskRevision: task.updatedAt,
      scope: "app_global",
    });
  });

  it("keeps a late action failure bound to the capability that caused it", async () => {
    let rejectFirst!: (error: unknown) => void;
    api.install
      .mockImplementationOnce(() => new Promise<BackendTask>((_resolve, reject) => { rejectFirst = reject; }))
      .mockResolvedValueOnce(globalTask());
    api.list.mockResolvedValue([]);
    useAppCapabilityStore.setState({
      initialized: true,
      capabilities: [capability(), capability({ capabilityId: "document-standard" })],
    });

    const first = useAppCapabilityStore.getState().confirmInstall("browser-runtime");
    await useAppCapabilityStore.getState().confirmInstall("document-standard");
    rejectFirst({ code: "APP_CAPABILITY_NETWORK", message: "offline", recoverable: true });
    await expect(first).rejects.toMatchObject({ code: "APP_CAPABILITY_NETWORK" });

    expect(useAppCapabilityStore.getState()).toMatchObject({
      actionErrorCapabilityId: "browser-runtime",
      actionErrorOperation: "install",
    });
  });
});
