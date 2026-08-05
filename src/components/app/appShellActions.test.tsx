import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { i18next } from "../../i18n";
import { useNavigationStore } from "../../stores/navigationStore";
import { defaultProject, useProjectStore } from "../../stores/projectStore";
import { AppShell } from "./AppShell";
import type { PendingAction } from "../../types/backend";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

beforeEach(async () => {
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
});

afterEach(() => cleanup());

function workspaceHeader() {
  const header = document.querySelector("main section > header");
  if (!header) throw new Error("workspace header not rendered");
  return within(header as HTMLElement);
}

describe("AppShell workspace header", () => {
  it("keeps only the view title and context control in shared chrome", () => {
    render(<AppShell />);

    expect(workspaceHeader().getByRole("heading", { name: "Dashboard" })).toBeInTheDocument();
    expect(workspaceHeader().queryByText("Health · recent activity · quick actions")).not.toBeInTheDocument();
    expect(workspaceHeader().queryByRole("button", { name: "Import" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Collapse context panel" })).toBeInTheDocument();
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
});
