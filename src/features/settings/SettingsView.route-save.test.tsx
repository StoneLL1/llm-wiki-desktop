import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { i18next } from "../../i18n";
import { defaultProject, useProjectStore } from "../../stores/projectStore";
import { defaultSettings, useSettingsStore } from "../../stores/settingsStore";
import type { Settings } from "../../types/settings";
import { SettingsView } from "./SettingsView";

const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", async (importOriginal) => ({
  ...await importOriginal<typeof import("@tauri-apps/api/core")>(),
  invoke: mocks.invoke,
}));

beforeEach(async () => {
  await i18next.changeLanguage("en");
  useProjectStore.getState().clearCurrentProject();
  useSettingsStore.setState({
    settings: { ...defaultSettings },
    globalLoaded: false,
    globalLoading: false,
    globalSaving: false,
    globalError: null,
    error: null,
  });
  mocks.invoke.mockReset();
  Object.defineProperty(window, "__TAURI_INTERNALS__", { value: {}, configurable: true });
});

afterEach(() => { Reflect.deleteProperty(window, "__TAURI_INTERNALS__"); });

describe("SettingsView default route persistence", () => {
  it("re-enables the route after persistence without starting or awaiting capability probes", async () => {
    const project = { ...defaultProject, projectId: "p1", name: "P1", rootPath: "/p1" };
    let resolveSave!: (settings: Settings) => void;
    const saved = new Promise<Settings>((resolve) => { resolveSave = resolve; });
    const refreshCapabilities = vi.fn(() => new Promise<void>(() => {}));
    mocks.invoke.mockImplementation((command: string) => {
      if (command === "get_settings" || command === "get_global_preferences") return Promise.resolve({ ...defaultSettings });
      if (command === "get_chat_convenience_authorization") return Promise.resolve({
        enabled: false, confirmedAt: "", projectId: project.projectId, rootPathFingerprint: "",
      });
      if (command === "save_project_settings") return saved;
      throw new Error(`Unexpected settings command: ${command}`);
    });
    useProjectStore.getState().setCurrentProject(project);
    render(<SettingsView initialSection="ai" project={project} providers={[]} agents={[{
      kind: "codex", command: "codex", state: "installed", version: "1.0",
      executablePath: "/bin/codex", isDefault: false, installGuidance: "", error: null,
    }]} onRefreshCapabilities={refreshCapabilities} onSaveProvider={vi.fn()} onSaveSecret={vi.fn()}
    onDeleteSecret={vi.fn()} onTestProvider={vi.fn()} />);
    await waitFor(() => expect(useSettingsStore.getState().loadedProjectKey).toBe("p1:/p1"));

    const route = screen.getByRole("combobox", { name: /Default execution route/i });
    fireEvent.change(route, { target: { value: "agent:codex" } });
    await waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith("save_project_settings", {
      request: expect.objectContaining({
        projectId: "p1", projectRootPath: "/p1",
        settings: expect.objectContaining({ aiDefaultRoute: { kind: "agent", agent: "codex" }, agentDefault: "codex" }),
      }),
    }));
    expect(route).toBeDisabled();
    expect(refreshCapabilities).not.toHaveBeenCalled();

    await act(async () => { resolveSave({ ...defaultSettings, aiDefaultRoute: { kind: "agent", agent: "codex" }, agentDefault: "codex" }); });

    await waitFor(() => expect(route).toBeEnabled());
    expect(route).toHaveValue("agent:codex");
    expect(refreshCapabilities).not.toHaveBeenCalled();
  });
});
