import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

import { applyColorThemePresetPreference, useSettingsStore } from "./settingsStore";
import { invalidateProjectResources } from "./projectScope";

let darkModeListener: ((event: MediaQueryListEvent) => void) | null = null;

describe("settingsStore color theme preset", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    window.localStorage.clear();
    document.documentElement.removeAttribute("data-color-theme-preset");
    document.documentElement.removeAttribute("data-theme");
    document.documentElement.style.cssText = "";
    darkModeListener = null;
    vi.stubGlobal(
      "matchMedia",
      vi.fn(
        () =>
          ({
            matches: false,
            media: "(prefers-color-scheme: dark)",
            addEventListener: vi.fn((event: string, listener: (event: MediaQueryListEvent) => void) => {
              if (event === "change") darkModeListener = listener;
            }),
            removeEventListener: vi.fn(),
          }) as unknown as MediaQueryList,
      ),
    );
    useSettingsStore.getState().reset();
  });

  afterEach(() => {
    applyColorThemePresetPreference("codex", "light");
    Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
    vi.unstubAllGlobals();
  });

  it("applies the default color theme preset to the document root", async () => {
    await useSettingsStore.getState().loadSettings("project-1", "D:/wiki");

    expect(document.documentElement.dataset.colorThemePreset).toBe("codex");
  });

  it("reapplies auto color theme tokens when the OS dark preference changes", async () => {
    await useSettingsStore.getState().loadSettings("project-1", "D:/wiki");

    expect(document.documentElement.style.getPropertyValue("--background")).toBe("#fbfbfa");

    darkModeListener?.({ matches: true } as MediaQueryListEvent);

    expect(document.documentElement.style.getPropertyValue("--background")).toBe("#101312");
  });
});

describe("settingsStore chat convenience authorization", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    useSettingsStore.getState().reset();
    Object.defineProperty(window, "__TAURI_INTERNALS__", { value: {}, configurable: true });
  });

  afterEach(() => {
    Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
  });

  it("loads authorization through the backend command", async () => {
    invokeMock.mockResolvedValueOnce({
      enabled: true,
      confirmedAt: "2026-07-05T00:00:00Z",
      projectId: "project-1",
      rootPathFingerprint: "0123456789abcdef",
    });

    const authorization = await useSettingsStore
      .getState()
      .loadChatConvenienceAuthorization("project-1", "D:/wiki");

    expect(invokeMock).toHaveBeenCalledWith("get_chat_convenience_authorization", {
      request: { projectId: "project-1", projectRootPath: "D:/wiki" },
    });
    expect(authorization.enabled).toBe(true);
    expect(useSettingsStore.getState().chatConvenienceAuthorization).toEqual(authorization);
  });

  it("single-flights authorization ensures inside the freshness window", async () => {
    invokeMock.mockResolvedValue({
      enabled: false,
      confirmedAt: "",
      projectId: "project-1",
      rootPathFingerprint: "",
    });

    await Promise.all(Array.from({ length: 20 }, () =>
      useSettingsStore.getState().ensureChatConvenienceAuthorization("project-1", "D:/wiki")));

    expect(invokeMock).toHaveBeenCalledTimes(1);
  });

  it("falls back to disabled authorization when loading fails", async () => {
    useSettingsStore.setState({
      chatConvenienceAuthorization: {
        enabled: true,
        confirmedAt: "2026-07-05T00:00:00Z",
        projectId: "project-a",
        rootPathFingerprint: "aaaaaaaaaaaaaaaa",
      },
    });
    invokeMock.mockRejectedValueOnce(new Error("backend unavailable"));

    const authorization = await useSettingsStore
      .getState()
      .loadChatConvenienceAuthorization("project-b", "D:/wiki-b");

    expect(authorization).toMatchObject({
      enabled: false,
      confirmedAt: "",
      projectId: "project-b",
      rootPathFingerprint: "",
    });
    expect(useSettingsStore.getState().chatConvenienceAuthorization).toEqual(authorization);
  });

  it("sets authorization through the backend command", async () => {
    invokeMock.mockResolvedValueOnce({
      enabled: true,
      confirmedAt: "2026-07-05T00:00:00Z",
      projectId: "project-1",
      rootPathFingerprint: "0123456789abcdef",
    });

    await useSettingsStore
      .getState()
      .setChatConvenienceAuthorization("project-1", "D:/wiki", true);

    expect(invokeMock).toHaveBeenCalledWith("set_chat_convenience_authorization", {
      request: { projectId: "project-1", projectRootPath: "D:/wiki", enabled: true },
    });
    expect(useSettingsStore.getState().chatConvenienceAuthorization?.enabled).toBe(true);
  });

  it("finishes an authorization mutation when focus invalidation arrives mid-save", async () => {
    let resolveSave!: (authorization: {
      enabled: boolean;
      confirmedAt: string;
      projectId: string;
      rootPathFingerprint: string;
    }) => void;
    useSettingsStore.setState({
      chatConvenienceAuthorization: {
        enabled: false,
        confirmedAt: "",
        projectId: "project-1",
        rootPathFingerprint: "",
      },
    });
    invokeMock.mockReturnValueOnce(new Promise((resolve) => { resolveSave = resolve; }));

    const saving = useSettingsStore
      .getState()
      .setChatConvenienceAuthorization("project-1", "D:/wiki", true);
    invalidateProjectResources(
      { projectId: "project-1", rootPath: "D:/wiki" },
      ["settings-chat-authorization"],
      true,
    );
    resolveSave({
      enabled: true,
      confirmedAt: "2026-07-05T00:00:00Z",
      projectId: "project-1",
      rootPathFingerprint: "0123456789abcdef",
    });
    await saving;

    expect(useSettingsStore.getState()).toMatchObject({
      chatConvenienceSaving: false,
    });
    expect(useSettingsStore.getState().chatConvenienceAuthorization?.enabled).toBe(true);
  });

  it("uses backend-style disabled fallback when Tauri is unavailable", async () => {
    Reflect.deleteProperty(window, "__TAURI_INTERNALS__");

    const authorization = await useSettingsStore
      .getState()
      .setChatConvenienceAuthorization("project-1", "D:/wiki", false);

    expect(authorization).toEqual({
      enabled: false,
      confirmedAt: "",
      projectId: "project-1",
      rootPathFingerprint: "",
    });
  });

  it("falls back to disabled authorization when setting fails", async () => {
    useSettingsStore.setState({
      chatConvenienceAuthorization: {
        enabled: true,
        confirmedAt: "2026-07-05T00:00:00Z",
        projectId: "project-a",
        rootPathFingerprint: "aaaaaaaaaaaaaaaa",
      },
    });
    invokeMock.mockRejectedValueOnce(new Error("backend unavailable"));

    const authorization = await useSettingsStore
      .getState()
      .setChatConvenienceAuthorization("project-b", "D:/wiki-b", true);

    expect(authorization).toMatchObject({
      enabled: false,
      confirmedAt: "",
      projectId: "project-b",
      rootPathFingerprint: "",
    });
    expect(useSettingsStore.getState().chatConvenienceAuthorization).toEqual(authorization);
  });

  it("revokes all authorizations through the backend command", async () => {
    useSettingsStore.setState({
      chatConvenienceAuthorization: {
        enabled: true,
        confirmedAt: "2026-07-05T00:00:00Z",
        projectId: "project-1",
        rootPathFingerprint: "0123456789abcdef",
      },
    });
    invokeMock.mockResolvedValueOnce(undefined);

    await useSettingsStore.getState().revokeAllChatConvenienceAuthorizations();

    expect(invokeMock).toHaveBeenCalledWith("revoke_all_chat_convenience_authorizations");
    expect(useSettingsStore.getState().chatConvenienceAuthorization).toBeNull();
  });

  it("clears a pending authorization save when revoke-all supersedes it", async () => {
    let resolveSave!: (value: {
      enabled: boolean;
      confirmedAt: string;
      projectId: string;
      rootPathFingerprint: string;
    }) => void;
    invokeMock.mockImplementation((command: string) => {
      if (command === "set_chat_convenience_authorization") {
        return new Promise((resolve) => { resolveSave = resolve; });
      }
      return Promise.resolve(undefined);
    });

    const saving = useSettingsStore.getState()
      .setChatConvenienceAuthorization("project-1", "D:/wiki", true);
    await useSettingsStore.getState().revokeAllChatConvenienceAuthorizations();
    expect(useSettingsStore.getState()).toMatchObject({
      chatConvenienceAuthorization: null,
      chatConvenienceSaving: false,
    });

    resolveSave({
      enabled: true,
      confirmedAt: "2026-07-05T00:00:00Z",
      projectId: "project-1",
      rootPathFingerprint: "fingerprint",
    });
    await saving;
    expect(useSettingsStore.getState()).toMatchObject({
      chatConvenienceAuthorization: null,
      chatConvenienceSaving: false,
    });
  });
});
