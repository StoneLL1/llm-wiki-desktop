import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
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

  it("does not repeat a generic settings title inside the settings section", () => {
    useNavigationStore.getState().setActiveView("settings");
    render(<AppShell />);

    expect(
      screen.queryByText("Global preferences, project-scoped provider settings, and secure key status."),
    ).not.toBeInTheDocument();
  });
});

describe("AppShell confirmation dialog checkpoint wiring", () => {
  const baseAction: PendingAction = {
    id: "action-cp",
    actionType: "delete_source",
    title: "Delete source",
    message: "Remove the original source file.",
    riskLevel: "destructive",
    affectedPaths: ["raw/sources/report.pdf"],
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
