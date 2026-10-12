import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { i18next } from "../../i18n";
import { defaultSettings, useSettingsStore } from "../../stores/settingsStore";
import { defaultProject, useProjectStore } from "../../stores/projectStore";
import { useUpdateStore } from "../../stores/updateStore";
import type { GlobalPreferencesPatch } from "../../types/settings";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), readPermission: vi.fn(), requestPermission: vi.fn(), sendTest: vi.fn() }));
vi.mock("@tauri-apps/api/core", async (importOriginal) => ({ ...await importOriginal<typeof import("@tauri-apps/api/core")>(), invoke: mocks.invoke }));
vi.mock("../../services/notifications", () => ({
  readNotificationPermission: mocks.readPermission,
  requestNotificationPermissionFromUser: mocks.requestPermission,
  invalidateNotificationPermissionEpoch: vi.fn(),
  sendTestNotification: mocks.sendTest,
}));

import { NoProjectSettingsDialog } from "./NoProjectSettingsDialog";
import { SettingsView } from "../settings/SettingsView";

let durable = { ...defaultSettings };

beforeEach(async () => {
  await i18next.changeLanguage("en");
  useProjectStore.getState().clearCurrentProject();
  useSettingsStore.setState({ settings: { ...defaultSettings }, globalLoaded: false, globalLoading: false, globalSaving: false, globalError: null, error: null });
  useUpdateStore.setState({ initialized: true });
  durable = { ...defaultSettings, systemNotifications: { ...defaultSettings.systemNotifications } };
  mocks.invoke.mockReset().mockImplementation((command: string, args?: { patch?: GlobalPreferencesPatch }) => {
    if (command === "get_global_preferences") return Promise.resolve(durable);
    if (command === "patch_global_preferences") {
      durable = { ...durable, ...args?.patch, systemNotifications: { ...durable.systemNotifications, ...args?.patch?.systemNotifications } };
      return Promise.resolve(durable);
    }
    throw new Error(`Unexpected project or settings command: ${command}`);
  });
  mocks.readPermission.mockReset().mockResolvedValue("granted");
  mocks.requestPermission.mockReset().mockResolvedValue(true);
  mocks.sendTest.mockReset().mockResolvedValue(true);
  Object.defineProperty(window, "__TAURI_INTERNALS__", { value: {}, configurable: true });
});

afterEach(() => { Reflect.deleteProperty(window, "__TAURI_INTERNALS__"); });

async function openSettings() {
  render(<NoProjectSettingsDialog open onClose={vi.fn()} />);
  await screen.findByRole("button", { name: "General" });
  await waitFor(() => expect(useSettingsStore.getState().globalLoaded).toBe(true));
  return screen.getByRole("dialog", { name: "Settings" });
}

describe("application settings before a knowledge base opens", () => {
  it("offers every application page and clearly explains unavailable project settings", async () => {
    const dialog = await openSettings();
    expect(within(dialog).getByRole("radiogroup", { name: "Language" })).toBeInTheDocument();
    for (const page of ["Appearance", "Background tasks", "Updates", "General"]) {
      fireEvent.click(within(dialog).getByRole("button", { name: page }));
      expect(within(dialog).getByRole("heading", { name: page })).toBeInTheDocument();
      expect(within(dialog).getByText("Application · Applies to all knowledge bases")).toBeInTheDocument();
    }
    fireEvent.click(within(dialog).getByRole("button", { name: "AI" }));
    expect(within(dialog).getByRole("heading", { name: "Open a knowledge base to configure" })).toBeInTheDocument();
    expect(within(dialog).queryByLabelText(/API key|Model/i)).not.toBeInTheDocument();
    expect(mocks.invoke.mock.calls.map(([command]) => command)).toEqual(["get_global_preferences"]);
  });

  it("saves appearance and close behavior through global patches only", async () => {
    const dialog = await openSettings();
    fireEvent.click(within(dialog).getByRole("button", { name: "Appearance" }));
    fireEvent.click(within(dialog).getByRole("radio", { name: "Dark" }));
    fireEvent.click(within(dialog).getByRole("radio", { name: "Parchment" }));
    fireEvent.click(within(dialog).getByRole("button", { name: "Background tasks" }));
    fireEvent.click(within(dialog).getByRole("radio", { name: /Ask every time/ }));
    await waitFor(() => expect(durable).toMatchObject({ theme: "dark", colorThemePreset: "paper", closeBehavior: "ask" }));
    await waitFor(() => expect(useSettingsStore.getState().globalSaving).toBe(false));
    expect(mocks.invoke.mock.calls.filter(([command]) => command === "patch_global_preferences")).toEqual([
      ["patch_global_preferences", { patch: { theme: "dark" } }],
      ["patch_global_preferences", { patch: { colorThemePreset: "paper" } }],
      ["patch_global_preferences", { patch: { closeBehavior: "ask" } }],
    ]);
    expect(mocks.invoke.mock.calls.every(([command]) => command === "get_global_preferences" || command === "patch_global_preferences")).toBe(true);
  });

  it("finds settings by Chinese keywords and focuses the selected field", async () => {
    const dialog = await openSettings();
    fireEvent.change(within(dialog).getByRole("searchbox", { name: "Search settings" }), { target: { value: "配色" } });
    const results = within(dialog).getByRole("region", { name: "Settings search" });
    expect(within(results).getByRole("heading", { name: "1 settings" })).toBeInTheDocument();
    fireEvent.click(within(results).getByRole("button", { name: /Color theme/i }));
    expect(within(dialog).getByRole("radiogroup", { name: "Color theme" })).toBeInTheDocument();
    expect(document.activeElement).toHaveTextContent("Color theme");
    expect(within(dialog).getByRole("searchbox", { name: "Search settings" })).toHaveValue("");
  });

  it("filters modified preferences and resets only the chosen value", async () => {
    durable = { ...durable, theme: "dark", colorThemePreset: "paper" };
    const dialog = await openSettings();
    fireEvent.click(within(dialog).getByRole("button", { name: "Modified only" }));
    const results = within(dialog).getByRole("region", { name: "Settings search" });
    expect(within(results).getByRole("heading", { name: "2 settings" })).toBeInTheDocument();
    fireEvent.click(within(results).getByRole("button", { name: "Reset Appearance to default" }));
    await waitFor(() => expect(durable).toMatchObject({ theme: "auto", colorThemePreset: "paper" }));
    expect(mocks.invoke).toHaveBeenCalledWith("patch_global_preferences", { patch: { theme: "auto" } });
    expect(within(results).queryByRole("button", { name: "Reset Appearance to default" })).not.toBeInTheDocument();
    expect(within(results).getByRole("button", { name: "Reset Color theme to default" })).toBeInTheDocument();
  });

  it("keeps notification permissions separate from saved toggles and can send a test", async () => {
    const dialog = await openSettings();
    fireEvent.click(within(dialog).getByRole("button", { name: "Background tasks" }));
    expect(await within(dialog).findByText("Notification service available")).toBeInTheDocument();
    expect(mocks.requestPermission).not.toHaveBeenCalled();
    fireEvent.click(within(dialog).getByRole("button", { name: "Send test notification" }));
    expect(await within(dialog).findByText(/Test notification request submitted\./)).toBeInTheDocument();
    expect(mocks.sendTest).toHaveBeenCalledTimes(1);
  });

  it("keeps application preferences writable when project settings cannot be read", async () => {
    const project = { ...defaultProject, projectId: "untrusted", name: "Read only", rootPath: "/只读/知识库" };
    const globalInvoke = mocks.invoke.getMockImplementation()!;
    mocks.invoke.mockImplementation((command, args) => {
      if (command === "get_settings" || command === "get_chat_convenience_authorization") return Promise.reject(new Error("PROJECT_ACCESS_DENIED"));
      return globalInvoke(command, args);
    });
    act(() => { useProjectStore.getState().setCurrentProject(project); });
    render(<SettingsView project={project} providers={[]} agents={[]} onRefreshCapabilities={vi.fn()} onSaveProvider={vi.fn()} onSaveSecret={vi.fn()} onDeleteSecret={vi.fn()} onTestProvider={vi.fn()} />);
    await waitFor(() => expect(useSettingsStore.getState().globalLoaded).toBe(true));
    fireEvent.click(screen.getByRole("button", { name: "Appearance" }));
    fireEvent.click(screen.getByRole("radio", { name: "Dark" }));
    await waitFor(() => expect(durable.theme).toBe("dark"));
    expect(mocks.invoke).toHaveBeenCalledWith("patch_global_preferences", { patch: { theme: "dark" } });
    expect(mocks.invoke.mock.calls.some(([command]) => command === "save_settings" || command === "save_project_settings")).toBe(false);
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });
});
