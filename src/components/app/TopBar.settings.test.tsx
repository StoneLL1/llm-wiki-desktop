import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { i18next } from "../../i18n";
import { defaultProject, useProjectStore } from "../../stores/projectStore";
import { defaultSettings, useSettingsStore } from "../../stores/settingsStore";
import { useToastStore } from "../../stores/toastStore";
import { useNavigationStore } from "../../stores/navigationStore";

const invokeMock = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", async (importOriginal) => ({ ...await importOriginal<typeof import("@tauri-apps/api/core")>(), invoke: invokeMock }));
vi.mock("./TaskActivityButton", () => ({ TaskActivityButton: () => null }));

import { TopBar } from "./TopBar";

beforeEach(async () => {
  await i18next.changeLanguage("en");
  useProjectStore.getState().clearCurrentProject();
  useProjectStore.getState().setRecentProjects([]);
  useSettingsStore.setState({ settings: { ...defaultSettings }, globalLoaded: false, globalSaving: false, globalError: null });
  useToastStore.setState({ toasts: [] });
  useNavigationStore.getState().closeSettings();
  invokeMock.mockReset().mockImplementation((_command, { patch }) => Promise.resolve({ ...defaultSettings, ...patch }));
  Object.defineProperty(window, "__TAURI_INTERNALS__", { value: {}, configurable: true });
});

afterEach(() => { Reflect.deleteProperty(window, "__TAURI_INTERNALS__"); });

describe("TopBar application language preference", () => {
  it.each([false, true])("persists the chosen language globally (project open: %s)", async (projectOpen) => {
    if (projectOpen) useProjectStore.getState().setCurrentProject({ ...defaultProject, projectId: "readonly", rootPath: "/只读/知识库" });
    render(<TopBar />);
    fireEvent.click(screen.getByRole("button", { name: "中文" }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("patch_global_preferences", { patch: { language: "zh-CN" } }));
    await waitFor(() => expect(useSettingsStore.getState().globalSaving).toBe(false));
    await waitFor(() => expect(screen.getByRole("button", { name: "中文" })).toHaveAttribute("aria-pressed", "true"));
    expect(invokeMock).toHaveBeenCalledTimes(1);
  });

  it("reports persistence failure and restores the previous language", async () => {
    invokeMock.mockRejectedValue(new Error("preferences read only"));
    render(<TopBar />);
    fireEvent.click(screen.getByRole("button", { name: "中文" }));
    await waitFor(() => expect(useToastStore.getState().toasts).toHaveLength(1));
    expect(useToastStore.getState().toasts[0].tone).toBe("error");
    await waitFor(() => expect(screen.getByRole("button", { name: "EN" })).toHaveAttribute("aria-pressed", "true"));
    expect(useSettingsStore.getState().settings.language).toBe("en");
  });

  it("retains an in-flight language save when the current project changes", async () => {
    let finish!: (value: typeof defaultSettings) => void;
    invokeMock.mockReturnValue(new Promise((resolve) => { finish = resolve; }));
    render(<TopBar />);
    fireEvent.click(screen.getByRole("button", { name: "中文" }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledTimes(1));
    act(() => { useProjectStore.getState().setCurrentProject({ ...defaultProject, projectId: "next", rootPath: "/next" }); });
    await act(async () => { finish({ ...defaultSettings, language: "zh-CN" }); });
    expect(useSettingsStore.getState()).toMatchObject({ globalSaving: false, settings: { language: "zh-CN" } });
    await waitFor(() => expect(screen.getByRole("button", { name: "中文" })).toHaveAttribute("aria-pressed", "true"));
  });
});
