import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { useNavigationStore } from "../../stores/navigationStore";
import { defaultProject, useProjectStore } from "../../stores/projectStore";
import { useTaskStore } from "../../stores/taskStore";
import { useWorkflowStore } from "../../stores/workflowStore";
import type { LlmProviderConfig } from "../../types/llm";

const mocks = vi.hoisted(() => ({
  useAiCapabilities: vi.fn(),
  useTaskLauncher: vi.fn(),
  useImportWorkflow: vi.fn(),
  useProviderWorkflow: vi.fn(),
  useWorkflowsController: vi.fn(),
  addPaths: vi.fn(),
  saveProvider: vi.fn(),
}));

vi.mock("../../hooks/useAiCapabilities", () => ({
  useAiCapabilities: mocks.useAiCapabilities,
}));
vi.mock("../../hooks/useTaskLauncher", () => ({
  useTaskLauncher: mocks.useTaskLauncher,
}));
vi.mock("../../features/import/useImportWorkflow", () => ({
  useImportWorkflow: mocks.useImportWorkflow,
}));
vi.mock("../../features/settings/useProviderWorkflow", () => ({
  useProviderWorkflow: mocks.useProviderWorkflow,
}));
vi.mock("../../features/workflows/useWorkflowsController", () => ({
  useWorkflowsController: mocks.useWorkflowsController,
}));
vi.mock("../../features/project/ProjectAuthorityDialog", () => ({
  ProjectAuthorityDialog: ({ action, onSatisfied }: { action: string; onSatisfied: () => void }) => (
    <div data-testid="project-authority-dialog">
      {action}
      <button onClick={onSatisfied}>Authority satisfied</button>
    </div>
  ),
}));
vi.mock("./WorkspaceRouter", () => ({
  WorkspaceRouter: ({ activeView, importWorkflow }: {
    activeView: string;
    importWorkflow: { addPaths: (paths: string[]) => void };
  }) => {
    const workflowSurface = useWorkflowStore((state) => state.surface);
    return (
      <div className={activeView === "workflows" ? "workflows-view" : undefined} data-testid="workspace-router">
        router:{activeView}
        {activeView === "workflows" ? (
          <>
            <h2 data-workflow-surface-title tabIndex={-1}>
              surface:{workflowSurface}
            </h2>
            {workflowSurface === "overview" ? (
              <>
                <button
                  aria-label="Open workflow preparation"
                  data-workflow-return-key="row:update_wiki:prepare"
                  onClick={() => useWorkflowStore.setState({ surface: "preparation" })}
                >
                  Prepare workflow
                </button>
                <button aria-label="View Update Wiki" data-workflow-return-key="attention:run-a" onClick={() => useWorkflowStore.setState({ surface: "detail" })}>
                  View attention run
                </button>
                <button aria-label="View Update Wiki" data-workflow-return-key="row:update_wiki:run-a" onClick={() => useWorkflowStore.setState({ surface: "detail" })}>
                  View row run
                </button>
                <aside id="right-context-panel">
                  <button
                    aria-label="Open queued workflow run"
                    data-workflow-return-key="context-queue:run-a"
                    onClick={() => useWorkflowStore.setState({ surface: "detail" })}
                  >
                    Queued workflow
                  </button>
                </aside>
              </>
            ) : (
              <button onClick={() => useWorkflowStore.setState({ surface: "overview" })}>
                Back to workflow overview
              </button>
            )}
          </>
        ) : null}
        <button onClick={() => importWorkflow.addPaths(["C:/source.pdf"])}>
          Import through router
        </button>
      </div>
    );
  },
}));
vi.mock("../../features/settings/SettingsDialog", () => ({
  SettingsDialog: ({
    open,
    initialSection,
    onClose,
    onSaveProvider,
    onManageProjectAuthority,
  }: {
    open: boolean;
    initialSection?: string;
    onClose: () => void;
    onSaveProvider: (config: LlmProviderConfig) => void;
    onManageProjectAuthority?: () => void;
  }) => (
    <div data-testid="settings-dialog">
      {String(open)}:{initialSection}
      <button onClick={onClose}>Close settings</button>
      <button onClick={onManageProjectAuthority}>Manage project authority</button>
      <button
        onClick={() =>
          onSaveProvider({
            provider: "ollama",
            model: "llama",
            baseUrl: "http://localhost:11434",
            contextWindow: 8192,
            enabled: true,
          })
        }
      >
        Save through settings
      </button>
    </div>
  ),
}));

import { WorkspaceController } from "./WorkspaceController";

const project = {
  ...defaultProject,
  projectId: "project-a",
  name: "Project A",
  rootPath: "D:/知识库/project-a",
};

const capabilities = {
  agents: [],
  providers: [],
  refreshing: false,
  refresh: vi.fn(),
};
const taskLauncher = {
  cancel: vi.fn(),
};
const importWorkflow = {
  isConfirming: false,
  addPaths: mocks.addPaths,
  requestClipboard: vi.fn(),
  confirm: vi.fn(),
};
const providerWorkflow = {
  providers: [],
  saveProvider: mocks.saveProvider,
  saveSecret: vi.fn(),
  deleteSecret: vi.fn(),
  testProvider: vi.fn(),
};
const workflowsController = {
  refresh: vi.fn(), prepare: vi.fn(), startPrepared: vi.fn(), cancel: vi.fn(),
  undoCancel: vi.fn(), reorder: vi.fn(), retry: vi.fn(), adjustAndPrepare: vi.fn(),
  openRun: vi.fn(), openResult: vi.fn(), confirm: vi.fn(), discard: vi.fn(),
  continueQueue: vi.fn(), loadHistoryMore: vi.fn(), handlePrerequisite: vi.fn(),
  backToOverview: vi.fn(),
};

beforeEach(() => {
  vi.clearAllMocks();
  mocks.useAiCapabilities.mockReturnValue(capabilities);
  mocks.useTaskLauncher.mockReturnValue(taskLauncher);
  mocks.useImportWorkflow.mockReturnValue(importWorkflow);
  mocks.useProviderWorkflow.mockReturnValue(providerWorkflow);
  mocks.useWorkflowsController.mockReturnValue(workflowsController);
  useProjectStore.setState({ currentProject: project });
  useNavigationStore.setState({
    activeView: "import",
    settingsOpen: true,
    rightPanelOpen: false,
    workspaceFocus: null,
    settingsSection: "general",
    workflowSettingsReturnIntent: null,
    workflowLaunchIntent: null,
  });
  useTaskStore.setState({ tasks: [] });
  useWorkflowStore.getState().reset();
});

describe("WorkspaceController", () => {
  it("composes project workflows once and wires Import and Provider callbacks", () => {
    render(<WorkspaceController />);

    expect(mocks.useAiCapabilities).toHaveBeenCalledWith(project, true);
    expect(mocks.useTaskLauncher).toHaveBeenCalledWith(project);
    expect(mocks.useImportWorkflow).toHaveBeenCalledWith(
      project,
      "import",
      taskLauncher,
    );
    expect(mocks.useProviderWorkflow).toHaveBeenCalledWith(project, capabilities);
    fireEvent.click(screen.getByRole("button", { name: "Import through router" }));
    expect(mocks.addPaths).toHaveBeenCalledWith(["C:/source.pdf"]);
    fireEvent.click(screen.getByRole("button", { name: "Save through settings" }));
    expect(mocks.saveProvider).toHaveBeenCalledWith(
      expect.objectContaining({ provider: "ollama" }),
    );
  });

  it("switches routed views without replacing the current project", () => {
    render(<WorkspaceController />);
    expect(screen.getByTestId("workspace-router")).toHaveTextContent("router:import");

    act(() => useNavigationStore.getState().setActiveView("workflows"));

    expect(screen.getByTestId("workspace-router")).toHaveTextContent("router:workflows");
    expect(useProjectStore.getState().currentProject).toBe(project);
    expect(mocks.useAiCapabilities).toHaveBeenLastCalledWith(project, true);
  });

  it("consumes a matching launch intent through backend preparation only", async () => {
    useNavigationStore.setState({
      activeView: "workflows",
      workflowLaunchIntent: {
        projectId: project.projectId,
        projectRootPath: project.rootPath,
        kind: "generate_content",
        origin: "wiki",
        scopePreset: {
          kind: "generate_content",
          artifactType: "knowledge_card",
          pagePaths: ["wiki/中文.md"],
          outputPath: null,
        },
        routeSelection: { kind: "byok", provider: "open_ai" },
      },
    });

    render(<WorkspaceController />);

    expect(workflowsController.prepare).toHaveBeenCalledWith(
      "generate_content",
      expect.objectContaining({ pagePaths: ["wiki/中文.md"] }),
      { kind: "byok", provider: "open_ai" },
    );
    expect(useNavigationStore.getState().workflowLaunchIntent).toBeNull();
  });

  it("keeps an Exports launch intent on the structured Workflows preparation route", async () => {
    useNavigationStore.setState({
      activeView: "workflows",
      workflowLaunchIntent: {
        projectId: project.projectId,
        projectRootPath: project.rootPath,
        kind: "generate_content",
        origin: "exports",
        scopePreset: {
          kind: "generate_content",
          artifactType: "beautiful_read",
          pagePaths: ["wiki/export-source.md"],
          outputPath: "exports/html/export-source.html",
        },
      },
    });

    render(<WorkspaceController />);

    expect(workflowsController.prepare).toHaveBeenCalledWith(
      "generate_content",
      expect.objectContaining({
        pagePaths: ["wiki/export-source.md"],
        outputPath: "exports/html/export-source.html",
      }),
      null,
    );
    expect(useNavigationStore.getState().workflowLaunchIntent).toBeNull();
  });

  it("clears a launch intent whose project identity no longer matches", () => {
    useNavigationStore.setState({
      activeView: "workflows",
      workflowLaunchIntent: {
        projectId: "other-project",
        projectRootPath: "D:/other",
        kind: "health_check",
        origin: "lint",
        scopePreset: { kind: "health_check", mode: "complete" },
      },
    });

    render(<WorkspaceController />);

    expect(workflowsController.prepare).not.toHaveBeenCalled();
    expect(useNavigationStore.getState().workflowLaunchIntent).toBeNull();
  });

  it("clears a deferred launch draft after a same-root identity replacement", () => {
    useProjectStore.setState({
      currentProject: project,
      authority: {
        projectId: project.projectId,
        canonicalRootPath: project.rootPath,
        canonicalIdentityKey: "replacement-identity",
        identityRevision: "replacement-revision",
      } as never,
    });
    useNavigationStore.setState({
      activeView: "workflows",
      workflowLaunchIntent: {
        projectId: project.projectId,
        projectRootPath: project.rootPath,
        kind: "update_wiki",
        origin: "workflows",
        scopePreset: { kind: "update_wiki", mode: "changed_sources", sourceVersions: [] },
        routeSelection: null,
        expectedCanonicalIdentityKey: "original-identity",
        expectedIdentityRevision: "original-revision",
      },
    });

    render(<WorkspaceController />);

    expect(workflowsController.prepare).not.toHaveBeenCalled();
    expect(useNavigationStore.getState().workflowLaunchIntent).toBeNull();
  });

  it("shows the Run history header action and opens the history surface", () => {
    useNavigationStore.setState({ activeView: "workflows" });

    render(<WorkspaceController />);
    fireEvent.click(screen.getByRole("button", { name: "Workflow history" }));

    expect(useWorkflowStore.getState().surface).toBe("history");
    expect(screen.queryByRole("button", { name: "Workflow history" })).not.toBeInTheDocument();
  });

  it("focuses each new workflow surface title and restores the history trigger on return", async () => {
    useNavigationStore.setState({ activeView: "workflows" });

    render(<WorkspaceController />);
    const historyTrigger = screen.getByRole("button", { name: "Workflow history" });
    historyTrigger.focus();
    fireEvent.click(historyTrigger);

    await waitFor(() => expect(screen.getByRole("heading", { name: "surface:history" })).toHaveFocus());

    act(() => useWorkflowStore.getState().setSurface("overview"));
    await waitFor(() => expect(screen.getByRole("button", { name: "Workflow history" })).toHaveFocus());
  });

  it("restores a workflow row trigger after returning from preparation", async () => {
    useNavigationStore.setState({ activeView: "workflows" });
    render(<WorkspaceController />);

    const trigger = screen.getByRole("button", { name: "Open workflow preparation" });
    trigger.focus();
    fireEvent.click(trigger);
    await waitFor(() => expect(screen.getByRole("heading", { name: "surface:preparation" })).toHaveFocus());

    fireEvent.click(screen.getByRole("button", { name: "Back to workflow overview" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "Open workflow preparation" })).toHaveFocus());
  });

  it("restores a right-panel queue trigger after returning from task detail", async () => {
    useNavigationStore.setState({ activeView: "workflows" });
    render(<WorkspaceController />);

    const trigger = screen.getByRole("button", { name: "Open queued workflow run" });
    trigger.focus();
    fireEvent.click(trigger);
    await waitFor(() => expect(screen.getByRole("heading", { name: "surface:detail" })).toHaveFocus());

    fireEvent.click(screen.getByRole("button", { name: "Back to workflow overview" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "Open queued workflow run" })).toHaveFocus());
  });

  it("restores the exact workflow trigger when accessible labels are duplicated", async () => {
    useNavigationStore.setState({ activeView: "workflows" });
    render(<WorkspaceController />);

    const [, rowTrigger] = screen.getAllByRole("button", { name: "View Update Wiki" });
    rowTrigger!.focus();
    fireEvent.click(rowTrigger!);
    await waitFor(() => expect(screen.getByRole("heading", { name: "surface:detail" })).toHaveFocus());

    fireEvent.click(screen.getByRole("button", { name: "Back to workflow overview" }));
    await waitFor(() => expect(screen.getAllByRole("button", { name: "View Update Wiki" })[1]).toHaveFocus());
  });

  it("does not restore a workflow trigger into a replacement project", async () => {
    useNavigationStore.setState({ activeView: "workflows" });
    useWorkflowStore.getState().activateProject("project-a\0D:/a");
    render(<WorkspaceController />);

    const trigger = screen.getByRole("button", { name: "Open workflow preparation" });
    trigger.focus();
    fireEvent.click(trigger);
    await waitFor(() => expect(screen.getByRole("heading", { name: "surface:preparation" })).toHaveFocus());

    act(() => useWorkflowStore.getState().activateProject("project-b\0D:/b"));
    await waitFor(() => expect(screen.getByRole("heading", { name: "surface:overview" })).toHaveFocus());
    expect(screen.getByRole("button", { name: "Open workflow preparation" })).not.toHaveFocus();
  });

  it("does not restore a workflow trigger after same-root identity replacement", async () => {
    useNavigationStore.setState({ activeView: "workflows" });
    useWorkflowStore.getState().activateProject("project-a\0D:/a");
    useWorkflowStore.setState({
      identityGuard: { canonicalIdentityKey: "identity-old", identityRevision: "revision-old" },
    });
    render(<WorkspaceController />);

    const trigger = screen.getByRole("button", { name: "Open workflow preparation" });
    trigger.focus();
    fireEvent.click(trigger);
    await waitFor(() => expect(screen.getByRole("heading", { name: "surface:preparation" })).toHaveFocus());

    act(() => useWorkflowStore.setState({
      surface: "overview",
      identityGuard: { canonicalIdentityKey: "identity-new", identityRevision: "revision-new" },
    }));
    await waitFor(() => expect(screen.getByRole("heading", { name: "surface:overview" })).toHaveFocus());
    expect(screen.getByRole("button", { name: "Open workflow preparation" })).not.toHaveFocus();
  });

  it("does not restore the history trigger after same-root identity replacement", async () => {
    useNavigationStore.setState({ activeView: "workflows" });
    useWorkflowStore.getState().activateProject("project-a\0D:/a");
    useWorkflowStore.setState({
      identityGuard: { canonicalIdentityKey: "identity-old", identityRevision: "revision-old" },
    });
    render(<WorkspaceController />);

    const historyTrigger = screen.getByRole("button", { name: "Workflow history" });
    fireEvent.click(historyTrigger);
    await waitFor(() => expect(screen.getByRole("heading", { name: "surface:history" })).toHaveFocus());

    act(() => useWorkflowStore.setState({
      surface: "overview",
      identityGuard: { canonicalIdentityKey: "identity-new", identityRevision: "revision-new" },
    }));
    await waitFor(() => expect(screen.getByRole("heading", { name: "surface:overview" })).toHaveFocus());
    expect(historyTrigger).not.toHaveFocus();
  });

  it("returns an open-project workflow prerequisite to the project workbench", () => {
    render(<WorkspaceController />);
    const options = mocks.useWorkflowsController.mock.calls.at(-1)?.[2] as {
      onProjectPrerequisite: (action: string, context: {
        project: typeof project;
        preparation: null;
        prepareAgain: () => void;
      }) => void;
    };

    act(() => options.onProjectPrerequisite("open_or_create_project", {
      project,
      preparation: null,
      prepareAgain: vi.fn(),
    }));

    expect(useProjectStore.getState().currentProject).toEqual(defaultProject);
    expect(screen.queryByTestId("project-authority-dialog")).not.toBeInTheDocument();
  });

  it("opens Settings at the requested AI section", () => {
    useNavigationStore.setState({ settingsOpen: true, settingsSection: "ai" });

    render(<WorkspaceController />);

    expect(screen.getByTestId("settings-dialog")).toHaveTextContent("true:ai");
  });

  it("hosts project authority prerequisites and only prepares again after satisfaction", () => {
    useNavigationStore.setState({ activeView: "workflows", settingsOpen: false });
    const preparation = {
      preparationId: "prep-authority",
      preparationRevision: "revision-authority",
      projectAccess: {
        canonicalIdentityKey: "identity-authority",
        identityRevision: "identity-revision-authority",
      },
    } as never;
    useWorkflowStore.setState({
      projectKey: `${project.projectId}\0${project.rootPath}`,
      preparation,
      surface: "preparation",
    });
    render(<WorkspaceController />);
    const options = mocks.useWorkflowsController.mock.calls.at(-1)?.[2] as {
      onProjectPrerequisite: (action: string, context: {
        project: typeof project;
        preparation: typeof preparation;
        prepareAgain: () => void;
      }) => void;
    };
    const prepareAgain = vi.fn();

    act(() => options.onProjectPrerequisite("trust_project", {
      project,
      preparation,
      prepareAgain,
    }));

    expect(screen.getByTestId("project-authority-dialog")).toHaveTextContent("trust_project");
    expect(prepareAgain).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Authority satisfied" }));
    expect(prepareAgain).toHaveBeenCalledTimes(1);
    expect(workflowsController.startPrepared).not.toHaveBeenCalled();
  });

  it("exposes trust revocation through the general project settings host", () => {
    render(<WorkspaceController />);
    fireEvent.click(screen.getByRole("button", { name: "Manage project authority" }));

    expect(screen.getByTestId("project-authority-dialog")).toHaveTextContent("manage");
    expect(useNavigationStore.getState().settingsOpen).toBe(false);
  });

  it("drops a Settings authority request when the active project changes", () => {
    render(<WorkspaceController />);
    fireEvent.click(screen.getByRole("button", { name: "Manage project authority" }));
    expect(screen.getByTestId("project-authority-dialog")).toBeInTheDocument();

    act(() => useProjectStore.getState().setCurrentProject({
      ...project,
      projectId: "project-b",
      rootPath: "D:/wiki/project-b",
    }));

    expect(screen.queryByTestId("project-authority-dialog")).not.toBeInTheDocument();
  });

  it("returns from workflow AI Settings by preparing the preserved scope without starting", () => {
    const scope = { kind: "health_check", mode: "complete" } as const;
    useWorkflowStore.setState({
      projectKey: `${project.projectId}\0${project.rootPath}`,
      surface: "preparation",
      preparation: {
        preparationId: "prep-a",
        preparationRevision: "revision-3",
        projectAccess: {
          canonicalIdentityKey: "identity-a",
          identityRevision: "identity-revision-a",
        },
      } as never,
    });
    useNavigationStore.setState({
      activeView: "workflows",
      settingsOpen: true,
      settingsSection: "ai",
      workflowSettingsReturnIntent: {
        projectId: project.projectId,
        projectRootPath: project.rootPath,
        kind: "health_check",
        scope,
        routeSelection: { kind: "byok", provider: "ollama" },
        source: "prerequisite",
        expectedSurface: "preparation",
        expectedCanonicalIdentityKey: "identity-a",
        expectedIdentityRevision: "identity-revision-a",
        expectedPreparationId: "prep-a",
        expectedPreparationRevision: "revision-3",
        expectedTaskId: null,
      },
    });

    render(<WorkspaceController />);
    fireEvent.click(screen.getByRole("button", { name: "Close settings" }));

    expect(workflowsController.prepare).toHaveBeenCalledWith(
      "health_check",
      scope,
      { kind: "byok", provider: "ollama" },
    );
    expect(workflowsController.startPrepared).not.toHaveBeenCalled();
    expect(useNavigationStore.getState().workflowSettingsReturnIntent).toBeNull();
  });

  it("does not prepare after an ordinary or stale Settings close", () => {
    const { rerender } = render(<WorkspaceController />);
    fireEvent.click(screen.getByRole("button", { name: "Close settings" }));
    expect(workflowsController.prepare).not.toHaveBeenCalled();

    useWorkflowStore.setState({
      projectKey: `${project.projectId}\0${project.rootPath}`,
      surface: "history",
      selectedTaskId: null,
    });
    useNavigationStore.setState({
      activeView: "workflows",
      settingsOpen: true,
      workflowSettingsReturnIntent: {
        projectId: project.projectId,
        projectRootPath: project.rootPath,
        kind: "update_wiki",
        scope: { kind: "update_wiki", mode: "changed_sources", sourceVersions: [] },
        routeSelection: null,
        source: "adjust",
        expectedSurface: "detail",
        expectedCanonicalIdentityKey: "identity-a",
        expectedIdentityRevision: "identity-revision-a",
        expectedPreparationId: null,
        expectedPreparationRevision: null,
        expectedTaskId: "run-a",
      },
    });
    rerender(<WorkspaceController />);
    fireEvent.click(screen.getByRole("button", { name: "Close settings" }));

    expect(workflowsController.prepare).not.toHaveBeenCalled();
    expect(useNavigationStore.getState().workflowSettingsReturnIntent).toBeNull();
  });
});
