import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { defaultProject, useProjectStore } from "../../stores/projectStore";
import { defaultSettings, useSettingsStore } from "../../stores/settingsStore";
import type { ImportWorkflow } from "../import/importWorkflow";

const mocks = vi.hoisted(() => ({
  saveSecret: vi.fn(),
  refresh: vi.fn(),
  invalidatePermission: vi.fn(),
  requestPermission: vi.fn().mockResolvedValue(true),
  readPermission: vi.fn().mockResolvedValue("granted"),
  sendTest: vi.fn().mockResolvedValue(true),
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));

vi.mock("../../services/notifications", () => ({
  readNotificationPermission: mocks.readPermission,
  sendTestNotification: mocks.sendTest,
  invalidateNotificationPermissionEpoch: mocks.invalidatePermission,
  requestNotificationPermissionFromUser: mocks.requestPermission,
}));

vi.mock("./AiSettings", () => ({
  AiSettings: (props: { onSaveSecret: (provider: "anthropic", secret: string) => Promise<unknown> }) => (
    <button type="button" onClick={() => void props.onSaveSecret("anthropic", "never-rendered")}>Save workflow secret</button>
  ),
}));
vi.mock("./AppearanceSettings", () => ({ AppearanceSettings: () => null }));
vi.mock("./LanguageSettings", () => ({ LanguageSettings: () => null }));
vi.mock("./SecuritySettings", () => ({ SecuritySettings: () => null }));
vi.mock("./UpdateSettings", () => ({ UpdateSettings: () => null }));
vi.mock("./ImportCompatibilitySettings", () => ({
  ImportCompatibilitySettings: ({ workflow }: { workflow: ImportWorkflow | null }) => (
    <div>{workflow ? `Compatibility workflow ${workflow.projectKey}` : "Compatibility unavailable"}</div>
  ),
}));

import { SettingsView } from "./SettingsView";

beforeEach(() => {
  mocks.refresh.mockReset().mockResolvedValue(undefined);
  mocks.saveSecret.mockReset().mockImplementation(async () => { await mocks.refresh(); });
  mocks.invalidatePermission.mockReset();
  mocks.requestPermission.mockReset().mockResolvedValue(true);
  mocks.readPermission.mockReset().mockResolvedValue("granted");
  mocks.sendTest.mockReset().mockResolvedValue(true);
  mocks.invoke.mockReset();
  useProjectStore.getState().clearCurrentProject();
  useSettingsStore.setState({ settings: { ...defaultSettings }, globalLoading: false, globalSaving: false, globalLoaded: false, globalError: null });
  Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
});

describe("SettingsView workflow ownership", () => {
  it("opens directly to the requested AI section", async () => {
    render(<SettingsView initialSection="ai" project={{ ...defaultProject, projectId: "p1", name: "P1", rootPath: "/p1" }} providers={[]} agents={[]}
      onRefreshCapabilities={mocks.refresh} onSaveProvider={vi.fn()} onSaveSecret={mocks.saveSecret}
      onDeleteSecret={vi.fn()} onTestProvider={vi.fn()} />);

    expect(await screen.findByRole("button", { name: "Save workflow secret" })).toBeInTheDocument();
  });

  it("lets the provider workflow own the single capability refresh after secret save", async () => {
    render(<SettingsView project={{ ...defaultProject, projectId: "p1", name: "P1", rootPath: "/p1" }} providers={[]} agents={[]}
      onRefreshCapabilities={mocks.refresh} onSaveProvider={vi.fn()} onSaveSecret={mocks.saveSecret}
      onDeleteSecret={vi.fn()} onTestProvider={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: /AI/i }));
    fireEvent.click(await screen.findByRole("button", { name: "Save workflow secret" }));
    await waitFor(() => expect(mocks.saveSecret).toHaveBeenCalledTimes(1));
    expect(mocks.refresh).toHaveBeenCalledTimes(1);
  });

  it("exposes legacy import review only through the Settings compatibility section", () => {
    const importWorkflow = { projectKey: "p1\u0000/p1" } as ImportWorkflow;
    render(<SettingsView project={{ ...defaultProject, projectId: "p1", name: "P1", rootPath: "/p1" }} providers={[]} agents={[]}
      onRefreshCapabilities={mocks.refresh} onSaveProvider={vi.fn()} onSaveSecret={mocks.saveSecret}
      onDeleteSecret={vi.fn()} onTestProvider={vi.fn()} importWorkflow={importWorkflow} />);

    expect(screen.queryByText(/Compatibility workflow/)).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /Compatibility/i }));
    expect(screen.getByText("Compatibility workflow p1\u0000/p1")).toBeInTheDocument();
  });

  it("requests notification permission only through an explicit settings action", async () => {
    render(<SettingsView project={{ ...defaultProject, projectId: "p1", name: "P1", rootPath: "/p1" }} providers={[]} agents={[]}
      onRefreshCapabilities={mocks.refresh} onSaveProvider={vi.fn()} onSaveSecret={mocks.saveSecret}
      onDeleteSecret={vi.fn()} onTestProvider={vi.fn()} />);

    expect(mocks.requestPermission).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: /Background/i }));
    await waitFor(() => expect(mocks.readPermission).toHaveBeenCalledTimes(1));
    expect(mocks.requestPermission).not.toHaveBeenCalled();
    fireEvent.click(await screen.findByRole("button", { name: /Check or request permission/i }));
    await waitFor(() => expect(mocks.requestPermission).toHaveBeenCalledTimes(1));
    expect(mocks.invalidatePermission).toHaveBeenCalledTimes(1);
  });

  it("preserves accepted preferences and blocks extra changes while system permission is pending", async () => {
    let resolvePermission!: (granted: boolean) => void;
    mocks.requestPermission.mockReturnValue(new Promise<boolean>((resolve) => { resolvePermission = resolve; }));
    render(<SettingsView project={{ ...defaultProject, projectId: "p1", name: "P1", rootPath: "/p1" }} providers={[]} agents={[]}
      onRefreshCapabilities={mocks.refresh} onSaveProvider={vi.fn()} onSaveSecret={mocks.saveSecret}
      onDeleteSecret={vi.fn()} onTestProvider={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: /Background/i }));
    const completed = await screen.findByRole("checkbox", { name: /Task completed/i });
    const failed = screen.getByRole("checkbox", { name: /Task failed/i });
    fireEvent.click(completed);
    await waitFor(() => expect(completed).toBeEnabled());
    expect(completed).not.toBeChecked();
    fireEvent.click(completed);
    await waitFor(() => expect(mocks.requestPermission).toHaveBeenCalledTimes(1));
    expect(completed).toBeChecked();
    expect(failed).toBeDisabled();
    fireEvent.click(failed);
    expect(useSettingsStore.getState().settings.systemNotifications.onTaskFailed).toBe(true);
    await act(async () => { resolvePermission(true); });
    await waitFor(() => expect(failed).toBeEnabled());
    fireEvent.click(failed);
    await waitFor(() => expect(failed).toBeEnabled());
    expect(useSettingsStore.getState().settings.systemNotifications).toMatchObject({ onTaskCompleted: true, onTaskFailed: false });
  });

  it("persists accepted application notification preferences across a project change", async () => {
    let resolveFirstSave!: (settings: typeof defaultSettings) => void;
    let durable = { ...defaultSettings };
    let saves = 0;
    Object.defineProperty(window, "__TAURI_INTERNALS__", { value: {}, configurable: true });
    mocks.invoke.mockImplementation((command, args) => {
      if (command === "patch_global_preferences") {
        saves += 1;
        durable = { ...durable, systemNotifications: { ...durable.systemNotifications, ...args.patch.systemNotifications } };
        if (saves === 1) return new Promise((resolve) => { resolveFirstSave = resolve; });
        return Promise.resolve(durable);
      }
      if (command === "get_settings" || command === "get_global_preferences") return Promise.resolve(durable);
      if (command === "get_chat_convenience_authorization") return Promise.resolve({ enabled: false, confirmedAt: "", projectId: args.request.projectId, rootPathFingerprint: "" });
      throw new Error(`Unexpected settings command: ${command}`);
    });
    const projectA = { ...defaultProject, projectId: "p1", name: "P1", rootPath: "/p1" };
    const projectB = { ...defaultProject, projectId: "p2", name: "P2", rootPath: "/p2" };
    const props = {
      providers: [], agents: [], onRefreshCapabilities: mocks.refresh,
      onSaveProvider: vi.fn(), onSaveSecret: mocks.saveSecret,
      onDeleteSecret: vi.fn(), onTestProvider: vi.fn(),
    };
    useProjectStore.getState().setCurrentProject(projectA);
    const { rerender } = render(<SettingsView project={projectA} {...props} />);
    await waitFor(() => expect(useSettingsStore.getState().globalLoaded).toBe(true));
    fireEvent.click(screen.getByRole("button", { name: /Background/i }));
    fireEvent.click(await screen.findByRole("checkbox", { name: /Task completed/i }));
    await waitFor(() => expect(saves).toBe(1));
    expect(screen.getByRole("checkbox", { name: /Task failed/i })).toBeDisabled();

    act(() => { useProjectStore.getState().setCurrentProject(projectB); });
    rerender(<SettingsView project={projectB} {...props} />);
    await act(async () => { resolveFirstSave(durable); });
    await waitFor(() => expect(screen.getByRole("checkbox", { name: /Task failed/i })).toBeEnabled());
    expect(screen.getByRole("checkbox", { name: /Task completed/i })).not.toBeChecked();
    fireEvent.click(screen.getByRole("checkbox", { name: /Task failed/i }));
    await waitFor(() => expect(saves).toBe(2));
    await waitFor(() => expect(useSettingsStore.getState().globalSaving).toBe(false));

    expect(durable.systemNotifications).toMatchObject({ onTaskCompleted: false, onTaskFailed: false });
    expect(mocks.invoke.mock.calls.filter(([command]) => command === "patch_global_preferences")).toEqual([
      ["patch_global_preferences", { patch: { systemNotifications: { onTaskCompleted: false } } }],
      ["patch_global_preferences", { patch: { systemNotifications: { onTaskFailed: false } } }],
    ]);
    expect(mocks.invoke.mock.calls.some(([command]) => command === "save_project_settings" || command === "save_settings")).toBe(false);
  });
});
