import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("../i18n", () => ({
  activateLocale: (_language: string, _loader: unknown, _target: unknown, isCurrent: () => boolean) => Promise.resolve(isCurrent()),
  LANGUAGE_STORAGE_KEY: "llm-wiki-desktop.language",
}));

import { defaultProject, useProjectStore } from "./projectStore";
import { defaultSettings, useSettingsStore } from "./settingsStore";
import type { GlobalPreferencesPatch, Settings } from "../types/settings";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<T>((res, rej) => { resolve = res; reject = rej; });
  return { promise, resolve, reject };
}

function merge(preferences: Settings, patch: GlobalPreferencesPatch): Settings {
  return {
    ...preferences,
    ...patch,
    systemNotifications: { ...preferences.systemNotifications, ...patch.systemNotifications },
  };
}

beforeEach(() => {
  invokeMock.mockReset();
  useProjectStore.getState().clearCurrentProject();
  useSettingsStore.setState({
    settings: { ...defaultSettings, systemNotifications: { ...defaultSettings.systemNotifications } },
    loading: false, saving: false, globalLoading: false, globalSaving: false, globalLoaded: false,
    error: null, globalError: null,
  });
  Object.defineProperty(window, "__TAURI_INTERNALS__", { value: {}, configurable: true });
});

afterEach(() => { Reflect.deleteProperty(window, "__TAURI_INTERNALS__"); });

describe("application preference persistence", () => {
  it("loads and applies the complete appearance without a project", async () => {
    invokeMock.mockResolvedValue({ ...defaultSettings, theme: "dark", colorThemePreset: "paper", density: "compact" });
    await useSettingsStore.getState().loadGlobalSettings();
    expect(invokeMock).toHaveBeenCalledWith("get_global_preferences");
    expect(document.documentElement.dataset).toMatchObject({ theme: "dark", colorThemePreset: "paper", density: "compact" });
    expect(document.documentElement.style.colorScheme).toBe("dark");
    expect(useSettingsStore.getState()).toMatchObject({ globalLoaded: true, globalLoading: false });
  });

  it("routes legacy global patches away from project write authorization", async () => {
    invokeMock.mockImplementation((_command, { patch }) => Promise.resolve(merge(defaultSettings, patch)));
    await useSettingsStore.getState().persistPatch("read-only-project", "/只读/知识库", { theme: "dark", closeBehavior: "ask" });
    expect(invokeMock).toHaveBeenCalledExactlyOnceWith("patch_global_preferences", { patch: { theme: "dark", closeBehavior: "ask" } });
  });

  it("serializes persistence while merging nested optimistic notification patches", async () => {
    const first = deferred<Settings>();
    let durable = { ...defaultSettings };
    invokeMock.mockImplementationOnce(() => first.promise).mockImplementation((_command, { patch }) => {
      durable = merge(durable, patch);
      return Promise.resolve(durable);
    });
    const completed = useSettingsStore.getState().persistGlobalPatch({ systemNotifications: { onTaskCompleted: false } });
    const failed = useSettingsStore.getState().persistGlobalPatch({ systemNotifications: { onTaskFailed: false } });
    await vi.waitFor(() => expect(invokeMock).toHaveBeenCalledTimes(1));
    expect(useSettingsStore.getState().settings.systemNotifications).toMatchObject({ onTaskCompleted: false, onTaskFailed: false });
    durable = merge(durable, { systemNotifications: { onTaskCompleted: false } });
    first.resolve(durable);
    await Promise.all([completed, failed]);
    expect(invokeMock).toHaveBeenCalledTimes(2);
    expect(useSettingsStore.getState()).toMatchObject({ globalSaving: false, settings: { systemNotifications: { onTaskCompleted: false, onTaskFailed: false, onConfirmationNeeded: true } } });
  });

  it("rejects failed saves, rolls back only their patch, and continues later saves", async () => {
    const first = deferred<Settings>();
    invokeMock.mockImplementationOnce(() => first.promise).mockImplementation((_command, { patch }) => Promise.resolve(merge(defaultSettings, patch)));
    const themeSave = useSettingsStore.getState().persistGlobalPatch({ theme: "dark" });
    const rejected = expect(themeSave).rejects.toThrow("disk unavailable");
    const densitySave = useSettingsStore.getState().persistGlobalPatch({ density: "compact" });
    first.reject(new Error("disk unavailable"));
    await rejected;
    await densitySave;
    expect(useSettingsStore.getState()).toMatchObject({ globalSaving: false, globalError: "disk unavailable", settings: { theme: "auto", density: "compact" } });
    expect(invokeMock.mock.calls[1]).toEqual(["patch_global_preferences", { patch: { density: "compact" } }]);
    expect(document.documentElement.dataset.theme).toBe("auto");
  });

  it("keeps global save facts when a project scope resets during the request", async () => {
    const request = deferred<Settings>();
    invokeMock.mockReturnValue(request.promise);
    const saving = useSettingsStore.getState().persistGlobalPatch({ colorThemePreset: "mint", theme: "dark" });
    useProjectStore.getState().setCurrentProject({ ...defaultProject, projectId: "next", rootPath: "/知识库" });
    request.resolve({ ...defaultSettings, colorThemePreset: "mint", theme: "dark" });
    await saving;
    expect(useSettingsStore.getState()).toMatchObject({ globalSaving: false, globalLoaded: true, settings: { colorThemePreset: "mint", theme: "dark", aiDefaultRoute: null } });
    expect(document.documentElement.dataset.colorThemePreset).toBe("mint");
  });

  it("does not overwrite a newer global save with a slow global read", async () => {
    const loading = deferred<Settings>();
    invokeMock.mockImplementation((command, args) => command === "get_global_preferences" ? loading.promise : Promise.resolve(merge(defaultSettings, args.patch)));
    const read = useSettingsStore.getState().loadGlobalSettings();
    await vi.waitFor(() => expect(invokeMock).toHaveBeenCalledWith("get_global_preferences"));
    await useSettingsStore.getState().persistGlobalPatch({ language: "zh-CN", theme: "dark" });
    loading.resolve(defaultSettings);
    await read;
    expect(useSettingsStore.getState()).toMatchObject({ globalLoading: false, settings: { language: "zh-CN", theme: "dark" } });
  });

  it("waits for pending writes before refreshing durable global preferences", async () => {
    const first = deferred<Settings>();
    invokeMock.mockImplementation((command) => command === "patch_global_preferences"
      ? first.promise
      : Promise.resolve({ ...defaultSettings, theme: "dark" }));
    const save = useSettingsStore.getState().persistGlobalPatch({ theme: "dark" });
    const load = useSettingsStore.getState().loadGlobalSettings();
    await vi.waitFor(() => expect(invokeMock).toHaveBeenCalledTimes(1));
    expect(invokeMock.mock.calls[0][0]).toBe("patch_global_preferences");
    first.resolve({ ...defaultSettings, theme: "dark" });
    await Promise.all([save, load]);
    expect(invokeMock.mock.calls.map(([command]) => command)).toEqual(["patch_global_preferences", "get_global_preferences"]);
    expect(useSettingsStore.getState()).toMatchObject({ globalLoading: false, settings: { theme: "dark" } });
  });

  it("serializes project patches and never carries a failed value into the next save", async () => {
    useProjectStore.getState().setCurrentProject({ ...defaultProject, projectId: "p", rootPath: "/p" });
    const first = deferred<Settings>();
    invokeMock.mockImplementationOnce(() => first.promise).mockImplementation((_command, { request }) => Promise.resolve(request.settings));
    const contextSave = useSettingsStore.getState().persistPatch("p", "/p", { contextWindow: 64_000 });
    const rejected = expect(contextSave).rejects.toThrow("project read only");
    const tokenSave = useSettingsStore.getState().persistPatch("p", "/p", { maxTokens: 8000 });
    await vi.waitFor(() => expect(invokeMock).toHaveBeenCalledTimes(1));
    first.reject(new Error("project read only"));
    await rejected;
    await tokenSave;
    expect(invokeMock.mock.calls[1][1].request.settings).toMatchObject({ contextWindow: 32_000, maxTokens: 8000 });
    expect(useSettingsStore.getState()).toMatchObject({ saving: false, error: "project read only", settings: { contextWindow: 32_000, maxTokens: 8000 } });
  });

  it("commits project settings without applying stale global response fields", async () => {
    useProjectStore.getState().setCurrentProject({ ...defaultProject, projectId: "p", rootPath: "/p" });
    const projectSave = deferred<Settings>();
    invokeMock.mockImplementation((command, args) => command === "save_project_settings" ? projectSave.promise : Promise.resolve(merge(defaultSettings, args.patch)));
    const project = useSettingsStore.getState().persistPatch("p", "/p", { contextWindow: 64_000 });
    await useSettingsStore.getState().persistGlobalPatch({ theme: "dark" });
    projectSave.resolve({ ...defaultSettings, contextWindow: 64_000 });
    await project;
    expect(useSettingsStore.getState().settings).toMatchObject({ contextWindow: 64_000, theme: "dark" });
  });
});
