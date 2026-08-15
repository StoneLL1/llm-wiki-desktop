import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const activateLocaleMock = vi.hoisted(() => vi.fn());
const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("../i18n", () => ({
  activateLocale: activateLocaleMock,
  LANGUAGE_STORAGE_KEY: "llm-wiki-desktop.language",
}));

import { defaultProject, useProjectStore } from "./projectStore";
import { defaultSettings, useSettingsStore } from "./settingsStore";

beforeEach(() => {
  activateLocaleMock.mockReset();
  activateLocaleMock.mockImplementation(
    (_language: "en" | "zh-CN", _loader: unknown, _target: unknown, isCurrent: () => boolean) =>
      Promise.resolve(isCurrent()),
  );
  invokeMock.mockReset();
  Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
  useProjectStore.getState().setCurrentProject({
    ...defaultProject,
    projectId: "project-a",
    name: "Project A",
    rootPath: "D:/wiki-a",
    health: { ...defaultProject.health },
  });
  useSettingsStore.setState({ settings: { ...defaultSettings }, loading: false, saving: false });
});

afterEach(() => {
  useProjectStore.getState().clearCurrentProject();
});

describe("settingsStore language request ownership", () => {
  it("lets an explicit return to the committed language supersede a slow request", async () => {
    let resolveChinese!: () => void;
    activateLocaleMock.mockImplementation(
      (language: "en" | "zh-CN", _loader: unknown, _target: unknown, isCurrent: () => boolean) => {
        if (language === "en") return Promise.resolve(isCurrent());
        return new Promise<boolean>((resolve) => {
          resolveChinese = () => resolve(isCurrent());
        });
      },
    );

    const staleChinese = useSettingsStore
      .getState()
      .persistPatch("project-a", "D:/wiki-a", { language: "zh-CN" });
    const latestEnglish = useSettingsStore
      .getState()
      .persistPatch("project-a", "D:/wiki-a", { language: "en" });

    await expect(latestEnglish).resolves.toMatchObject({ language: "en" });
    resolveChinese();
    await staleChinese;

    expect(activateLocaleMock).toHaveBeenNthCalledWith(
      1,
      "zh-CN",
      undefined,
      undefined,
      expect.any(Function),
    );
    expect(activateLocaleMock).toHaveBeenNthCalledWith(
      2,
      "en",
      undefined,
      undefined,
      expect.any(Function),
    );
    expect(useSettingsStore.getState()).toMatchObject({
      settings: { language: "en" },
      saving: false,
    });
  });

  it("clears loading when another locale activation supersedes the current load", async () => {
    activateLocaleMock.mockResolvedValue(false);

    await useSettingsStore.getState().loadSettings("project-a", "D:/wiki-a");

    expect(useSettingsStore.getState().loading).toBe(false);
  });

  it("carries a pending language intent into a later non-language patch", async () => {
    let resolveFirstActivation!: () => void;
    activateLocaleMock
      .mockImplementationOnce(
        (_language: "en" | "zh-CN", _loader: unknown, _target: unknown, isCurrent: () => boolean) =>
          new Promise<boolean>((resolve) => {
            resolveFirstActivation = () => resolve(isCurrent());
          }),
      )
      .mockImplementationOnce(
        (_language: "en" | "zh-CN", _loader: unknown, _target: unknown, isCurrent: () => boolean) =>
          Promise.resolve(isCurrent()),
      );

    const languageSave = useSettingsStore
      .getState()
      .persistPatch("project-a", "D:/wiki-a", { language: "zh-CN" });
    const densitySave = useSettingsStore
      .getState()
      .persistPatch("project-a", "D:/wiki-a", { density: "compact" });

    await expect(densitySave).resolves.toMatchObject({ language: "zh-CN", density: "compact" });
    resolveFirstActivation();
    await languageSave;

    expect(activateLocaleMock).toHaveBeenNthCalledWith(
      2,
      "zh-CN",
      undefined,
      undefined,
      expect.any(Function),
    );
    expect(useSettingsStore.getState()).toMatchObject({
      settings: { language: "zh-CN", density: "compact" },
      saving: false,
    });
  });

  it("does not let an older settings load overwrite a user save", async () => {
    Object.defineProperty(window, "__TAURI_INTERNALS__", { value: {}, configurable: true });
    let resolveLoad!: (settings: typeof defaultSettings) => void;
    invokeMock.mockImplementation((command: string, args?: { request?: { settings?: typeof defaultSettings } }) => {
      if (command === "get_settings") {
        return new Promise<typeof defaultSettings>((resolve) => {
          resolveLoad = resolve;
        });
      }
      if (command === "save_settings") return Promise.resolve(args?.request?.settings);
      return Promise.resolve(undefined);
    });

    const staleLoad = useSettingsStore.getState().loadSettings("project-a", "D:/wiki-a");
    const saved = await useSettingsStore
      .getState()
      .persistPatch("project-a", "D:/wiki-a", { language: "zh-CN", theme: "dark" });
    resolveLoad({ ...defaultSettings, language: "en", theme: "light" });
    await staleLoad;

    expect(saved).toMatchObject({ language: "zh-CN", theme: "dark" });
    expect(useSettingsStore.getState()).toMatchObject({
      settings: { language: "zh-CN", theme: "dark" },
      loading: false,
      saving: false,
    });
  });
});
