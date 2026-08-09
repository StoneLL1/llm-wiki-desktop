import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { defaultProject } from "../../stores/projectStore";
import { useSettingsStore } from "../../stores/settingsStore";
import type { ImportWorkflow } from "../import/importWorkflow";

const mocks = vi.hoisted(() => ({
  saveSecret: vi.fn(),
  refresh: vi.fn(),
  invalidatePermission: vi.fn(),
  requestPermission: vi.fn().mockResolvedValue(true),
}));

vi.mock("../../services/notifications", () => ({
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

    fireEvent.click(screen.getByRole("button", { name: /Background/i }));
    fireEvent.click(await screen.findByRole("button", { name: /Enable or retry/i }));
    await waitFor(() => expect(mocks.requestPermission).toHaveBeenCalledTimes(1));
    expect(mocks.invalidatePermission).toHaveBeenCalledTimes(1);
  });

  it("preserves two notification toggles while the permission prompt is pending", async () => {
    let resolvePermission!: (granted: boolean) => void;
    mocks.requestPermission.mockReturnValue(
      new Promise<boolean>((resolve) => { resolvePermission = resolve; }),
    );
    render(<SettingsView project={{ ...defaultProject, projectId: "p1", name: "P1", rootPath: "/p1" }} providers={[]} agents={[]}
      onRefreshCapabilities={mocks.refresh} onSaveProvider={vi.fn()} onSaveSecret={mocks.saveSecret}
      onDeleteSecret={vi.fn()} onTestProvider={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: /Background/i }));

    const completed = await screen.findByRole("checkbox", { name: /Task completed/i });
    const failed = screen.getByRole("checkbox", { name: /Task failed/i });
    fireEvent.click(completed);
    fireEvent.click(failed);
    await waitFor(() => expect(useSettingsStore.getState().settings.systemNotifications).toMatchObject({
      onTaskCompleted: false,
      onTaskFailed: false,
    }));
    fireEvent.click(completed);
    fireEvent.click(failed);

    await waitFor(() => expect(useSettingsStore.getState().settings.systemNotifications).toMatchObject({
      onTaskCompleted: true,
      onTaskFailed: true,
    }));
    expect(mocks.requestPermission).toHaveBeenCalledTimes(2);
    resolvePermission(true);
  });

  it("drops queued notification saves when the owning project changes", async () => {
    const originalPersistPatch = useSettingsStore.getState().persistPatch;
    let resolveFirstSave!: (settings: ReturnType<typeof useSettingsStore.getState>["settings"]) => void;
    const persistPatch = vi.fn().mockReturnValueOnce(
      new Promise((resolve) => { resolveFirstSave = resolve; }),
    );
    useSettingsStore.setState({ persistPatch });
    const projectA = { ...defaultProject, projectId: "p1", name: "P1", rootPath: "/p1" };
    const projectB = { ...defaultProject, projectId: "p2", name: "P2", rootPath: "/p2" };
    const props = {
      providers: [], agents: [], onRefreshCapabilities: mocks.refresh,
      onSaveProvider: vi.fn(), onSaveSecret: mocks.saveSecret,
      onDeleteSecret: vi.fn(), onTestProvider: vi.fn(),
    };
    const { rerender } = render(<SettingsView project={projectA} {...props} />);
    fireEvent.click(screen.getByRole("button", { name: /Background/i }));
    fireEvent.click(await screen.findByRole("checkbox", { name: /Task completed/i }));
    fireEvent.click(screen.getByRole("checkbox", { name: /Task failed/i }));
    await waitFor(() => expect(persistPatch).toHaveBeenCalledTimes(1));

    rerender(<SettingsView project={projectB} {...props} />);
    resolveFirstSave(useSettingsStore.getState().settings);
    await act(async () => { await Promise.resolve(); await Promise.resolve(); });

    expect(persistPatch).toHaveBeenCalledTimes(1);
    useSettingsStore.setState({ persistPatch: originalPersistPatch });
  });
});
