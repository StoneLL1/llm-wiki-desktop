import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { i18next } from "../../i18n";
import { useNavigationStore } from "../../stores/navigationStore";
import { defaultProject, useProjectStore } from "../../stores/projectStore";
import { useUpdateStore } from "../../stores/updateStore";
import {
  resetProjectFactsStoreForTests,
} from "../../stores/projectFactsStore";
import { AppShell } from "./AppShell";
import type { PendingAction } from "../../types/backend";

const invokeMock = vi.hoisted(() => vi.fn());
const originalMatchMedia = window.matchMedia;

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

function mockNarrowDesktop(matches: boolean) {
  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    writable: true,
    value: vi.fn((query: string) => ({
      matches: query === "(max-width: 1180px)" ? matches : false,
      media: query,
      onchange: null,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      addListener: vi.fn(),
      removeListener: vi.fn(),
      dispatchEvent: vi.fn(),
    })),
  });
}

beforeEach(async () => {
  mockNarrowDesktop(false);
  await i18next.changeLanguage("en");
  useNavigationStore.getState().setActiveView("dashboard");
  useNavigationStore.getState().setRightPanelOpen(true);
  useNavigationStore.getState().closeSettings();
  useNavigationStore.setState({ workspaceFocus: null, rightPanelOpenBeforeFocus: null });
  useProjectStore.getState().setCurrentProject({
    ...defaultProject,
    projectId: "shell-project",
    name: "Shell Project",
    rootPath: "D:/knowledge/shell-project",
  });
  useProjectStore.getState().setPendingAction(undefined);
  invokeMock.mockReset();
  useUpdateStore.getState().resetForTests();
  resetProjectFactsStoreForTests();
});

afterEach(() => {
  cleanup();
  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    writable: true,
    value: originalMatchMedia,
  });
});

function workspaceHeader() {
  const header = document.querySelector("main section > header");
  if (!header) throw new Error("workspace header not rendered");
  return within(header as HTMLElement);
}

describe("AppShell workspace header", () => {
  it("runs a real manual update check without an open project", async () => {
    useProjectStore.getState().setCurrentProject(defaultProject);
    useUpdateStore.setState({
      initialized: true,
      uiStatus: "idle",
    });
    invokeMock.mockImplementation((command: string) => {
      if (command === "check_app_update") {
        return Promise.resolve({
          phase: "idle",
          offer: null,
          downloadedBytes: 0,
          totalBytes: null,
          error: null,
        });
      }
      if (command === "get_global_update_preferences") {
        return Promise.resolve({
          checkUpdates: true,
          updateFrequency: "daily",
          autoDownloadUpdates: false,
          promptChangelogBeforeInstall: true,
          lastCheckedAt: "2026-08-21T00:00:00Z",
          dismissedOfferId: null,
          dismissedVersion: null,
        });
      }
      return Promise.resolve([]);
    });

    render(<AppShell />);
    fireEvent.click(screen.getByRole("button", { name: "Check for updates" }));

    expect(screen.getByRole("dialog", { name: "Updates" })).toBeVisible();
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("check_app_update"));
    expect(useUpdateStore.getState().uiStatus).toBe("up_to_date");
  });

  it("keeps only the view title and context control in shared chrome", async () => {
    render(<AppShell />);

    expect(workspaceHeader().getByRole("heading", { name: "Dashboard" })).toBeInTheDocument();
    expect(workspaceHeader().queryByText("Health · recent activity · quick actions")).not.toBeInTheDocument();
    expect(workspaceHeader().queryByRole("button", { name: "Import" })).not.toBeInTheDocument();
    expect(
      await screen.findByRole("button", { name: "Collapse context panel" }),
    ).toBeInTheDocument();
  });

  it("opens settings as a floating dialog without leaving the active workspace view", () => {
    useNavigationStore.getState().setActiveView("dashboard");
    useNavigationStore.getState().openSettings();
    render(<AppShell />);

    // Settings renders as a modal dialog layered over the workspace.
    expect(screen.getByRole("dialog", { name: "Settings" })).toBeInTheDocument();
    // The workspace behind is preserved (still Dashboard, not swapped out).
    expect(workspaceHeader().getByRole("heading", { name: "Dashboard" })).toBeInTheDocument();
    expect(useNavigationStore.getState().activeView).toBe("dashboard");
    expect(useNavigationStore.getState().settingsOpen).toBe(true);
  });
});

describe("AppShell workspace focus", () => {
  it("hides the right panel reopen control while the export preview is focused", () => {
    useNavigationStore.getState().setActiveView("exports");
    useNavigationStore.getState().focusWorkspace("exportPreview");

    render(<AppShell />);

    expect(document.querySelector(".app-shell")).toHaveClass("is-workspace-focused");
    expect(workspaceHeader().queryByRole("button", { name: "Open context panel" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Collapse context panel" })).not.toBeInTheDocument();
  });

  it("clears workspace focus with Escape and restores the previous right panel state", () => {
    useNavigationStore.getState().setActiveView("exports");
    useNavigationStore.getState().setRightPanelOpen(true);
    useNavigationStore.getState().focusWorkspace("exportPreview");

    render(<AppShell />);
    fireEvent.keyDown(document, { key: "Escape" });

    expect(useNavigationStore.getState().workspaceFocus).toBeNull();
    expect(useNavigationStore.getState().rightPanelOpen).toBe(true);
  });
});

describe("AppShell narrow context panel", () => {
  it("scopes the narrow overlay motion hook to Workflows", async () => {
    mockNarrowDesktop(true);
    const dashboard = render(<AppShell />);
    fireEvent.click(await screen.findByRole("button", { name: "Open context panel" }));
    expect(document.querySelector(".right-panel-overlay__surface")).not.toHaveClass("is-workflows");
    dashboard.unmount();

    useNavigationStore.getState().setActiveView("workflows");
    useNavigationStore.getState().setRightPanelOpen(false);
    render(<AppShell />);
    fireEvent.click(await screen.findByRole("button", { name: "Open context panel" }));
    expect(document.querySelector(".right-panel-overlay__surface")).toHaveClass("is-workflows");
  });

  it("uses a labelled modal overlay, traps focus, closes by Escape or outside click, and restores the trigger", async () => {
    mockNarrowDesktop(true);
    render(<AppShell />);

    const trigger = await screen.findByRole("button", { name: "Open context panel" });
    trigger.focus();
    fireEvent.click(trigger);

    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveAttribute("aria-modal", "true");
    expect(dialog).toHaveAccessibleName();
    expect(within(dialog).getByRole("button", { name: "Close context panel" })).toBeVisible();
    expect(document.querySelector(".app-shell")).toHaveAttribute("inert");
    expect(dialog).toContainElement(document.activeElement as HTMLElement);
    expect(document.querySelector(".right-panel__backdrop")).not.toBeInTheDocument();

    const backgroundButton = document.querySelector<HTMLElement>(".app-topbar button");
    backgroundButton?.focus();
    expect(dialog).toContainElement(document.activeElement as HTMLElement);

    fireEvent.keyDown(document, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(screen.getByRole("button", { name: "Open context panel" })).toHaveFocus();

    fireEvent.click(trigger);
    const reopened = await screen.findByRole("dialog");
    fireEvent.click(reopened);
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(screen.getByRole("button", { name: "Open context panel" })).toHaveFocus();
  });

  it("keeps the panel as a complementary docked surface above the narrow breakpoint", () => {
    render(<AppShell />);

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.getByRole("complementary")).toHaveAttribute("id", "right-context-panel");
    expect(document.querySelector(".app-shell")).not.toHaveAttribute("inert");
  });
});

describe("AppShell confirmation dialog checkpoint wiring", () => {
  const baseAction: PendingAction = {
    id: "action-cp",
    actionType: "delete_file",
    title: "Delete file",
    message: "Remove a project file.",
    riskLevel: "destructive",
    affectedPaths: ["wiki/report.md"],
    preview: null,
    expiresAt: null,
  };

  it("shows checkpoint available when PendingAction.checkpointHash is set", () => {
    useProjectStore.getState().setPendingAction({ ...baseAction, checkpointHash: "abc123" });
    render(<AppShell />);

    expect(screen.getByText("Checkpoint: available")).toBeInTheDocument();
  });

  it("shows checkpoint not created yet when checkpointHash is null", () => {
    useProjectStore.getState().setPendingAction({ ...baseAction, checkpointHash: null });
    render(<AppShell />);

    expect(screen.getByText("Checkpoint: not created yet")).toBeInTheDocument();
  });

  it("treats a missing checkpointHash field as not created yet", () => {
    useProjectStore.getState().setPendingAction(baseAction);
    render(<AppShell />);

    expect(screen.getByText("Checkpoint: not created yet")).toBeInTheDocument();
  });
});

describe("AppShell first-screen agent detection", () => {
  const tauriWindow = window as unknown as { __TAURI_INTERNALS__?: unknown };

  afterEach(() => {
    delete tauriWindow.__TAURI_INTERNALS__;
    useProjectStore.getState().setCurrentProject(defaultProject);
  });

  it("refreshes agentRoute when the dashboard mounts with an active project", async () => {
    tauriWindow.__TAURI_INTERNALS__ = {};
    useProjectStore.getState().setCurrentProject({
      ...defaultProject,
      projectId: "proj-1",
      rootPath: "/tmp/proj-1",
      agentRoute: "unconfigured",
    });
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "detect_agents") {
        return Promise.resolve([
          {
            kind: "claude",
            command: "claude",
            state: "installed",
            version: "1.0",
            executablePath: "/usr/bin/claude",
            isDefault: true,
            installGuidance: "",
            error: null,
          },
        ]);
      }
      if (cmd === "list_llm_providers") return Promise.resolve([]);
      if (cmd === "git_status") {
        return Promise.resolve({ isRepository: false, branch: null, head: null, hasChanges: false });
      }
      return Promise.resolve([]);
    });

    render(<AppShell />);

    await waitFor(() => {
      expect(useProjectStore.getState().currentProject.agentRoute).toBe("agent");
    });
  });

  it("single-flights StrictMode shell status consumers and the AI controller", async () => {
    tauriWindow.__TAURI_INTERNALS__ = {};
    useProjectStore.getState().setCurrentProject({
      ...defaultProject,
      projectId: "proj-single-flight",
      rootPath: "/tmp/proj-single-flight",
    });
    invokeMock.mockImplementation((command: string) => {
      if (command === "git_status") {
        return Promise.resolve({
          isRepository: false,
          branch: null,
          head: null,
          hasChanges: false,
        });
      }
      if (command === "detect_agents" || command === "list_llm_providers") {
        return Promise.resolve([]);
      }
      return Promise.resolve([]);
    });

    render(<StrictMode><AppShell /></StrictMode>);

    await waitFor(() => {
      expect(invokeMock.mock.calls.filter(([command]) => command === "git_status")).toHaveLength(1);
      expect(invokeMock.mock.calls.filter(([command]) => command === "detect_agents")).toHaveLength(1);
      expect(invokeMock.mock.calls.filter(([command]) => command === "list_llm_providers")).toHaveLength(1);
    });
  });

  it("single-flights one revalidation per fact when the app window regains focus", async () => {
    tauriWindow.__TAURI_INTERNALS__ = {};
    useProjectStore.getState().setCurrentProject({
      ...defaultProject,
      projectId: "proj-focus",
      rootPath: "/tmp/proj-focus",
    });
    invokeMock.mockImplementation((command: string) => {
      if (command === "git_status") {
        return Promise.resolve({
          isRepository: false,
          branch: null,
          head: null,
          hasChanges: false,
        });
      }
      return Promise.resolve([]);
    });
    render(<AppShell />);
    await waitFor(() => {
      expect(invokeMock.mock.calls.filter(([command]) => command === "git_status")).toHaveLength(1);
      expect(invokeMock.mock.calls.filter(([command]) => command === "detect_agents")).toHaveLength(1);
      expect(invokeMock.mock.calls.filter(([command]) => command === "list_llm_providers")).toHaveLength(1);
    });

    fireEvent.focus(window);

    await waitFor(() => {
      expect(invokeMock.mock.calls.filter(([command]) => command === "git_status")).toHaveLength(2);
      expect(invokeMock.mock.calls.filter(([command]) => command === "detect_agents")).toHaveLength(2);
      expect(invokeMock.mock.calls.filter(([command]) => command === "list_llm_providers")).toHaveLength(2);
    });
  });

  it("reprobes mounted observers after the project authority identity changes", async () => {
    tauriWindow.__TAURI_INTERNALS__ = {};
    const project = {
      ...defaultProject,
      projectId: "proj-authority",
      rootPath: "/tmp/proj-authority",
    };
    useProjectStore.getState().setCurrentProject(project);
    useNavigationStore.getState().setRightPanelOpen(false);
    useProjectStore.setState({
      authority: {
        projectId: project.projectId,
        canonicalIdentityKey: "identity-a",
        identityRevision: "revision-1",
      } as never,
    });
    invokeMock.mockImplementation((command: string) => {
      if (command === "git_status") {
        return Promise.resolve({
          isRepository: false,
          branch: null,
          head: null,
          hasChanges: false,
        });
      }
      return Promise.resolve([]);
    });
    render(<AppShell />);
    await waitFor(() => {
      expect(invokeMock.mock.calls.filter(([command]) => command === "git_status")).toHaveLength(1);
      expect(invokeMock.mock.calls.filter(([command]) => command === "detect_agents")).toHaveLength(1);
      expect(invokeMock.mock.calls.filter(([command]) => command === "list_llm_providers")).toHaveLength(1);
    });

    act(() => useProjectStore.setState({
      authority: {
        projectId: project.projectId,
        canonicalIdentityKey: "identity-a",
        identityRevision: "revision-2",
      } as never,
    }));

    await waitFor(() => {
      expect(invokeMock.mock.calls.filter(([command]) => command === "git_status")).toHaveLength(2);
      expect(invokeMock.mock.calls.filter(([command]) => command === "detect_agents")).toHaveLength(2);
      expect(invokeMock.mock.calls.filter(([command]) => command === "list_llm_providers")).toHaveLength(2);
    });
  });
});
