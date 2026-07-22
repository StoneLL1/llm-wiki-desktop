import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { useNavigationStore } from "../../stores/navigationStore";
import { defaultProject, useProjectStore } from "../../stores/projectStore";
import { useTaskStore } from "../../stores/taskStore";
import type { LlmProviderConfig } from "../../types/llm";
import type { RunAgentOptions } from "../../features/agent/RunAgentDialog";

const mocks = vi.hoisted(() => ({
  useAiCapabilities: vi.fn(),
  useTaskLauncher: vi.fn(),
  useImportWorkflow: vi.fn(),
  useProviderWorkflow: vi.fn(),
  useAgentWorkflow: vi.fn(),
  addPaths: vi.fn(),
  runAgent: vi.fn(),
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
vi.mock("../../features/agent/useAgentWorkflow", () => ({
  useAgentWorkflow: mocks.useAgentWorkflow,
}));
vi.mock("./WorkspaceRouter", () => ({
  WorkspaceRouter: ({ activeView, importWorkflow }: {
    activeView: string;
    importWorkflow: { addPaths: (paths: string[]) => void };
  }) => (
    <div data-testid="workspace-router">
      router:{activeView}
      <button onClick={() => importWorkflow.addPaths(["C:/source.pdf"])}>
        Import through router
      </button>
    </div>
  ),
}));
vi.mock("../../features/agent/RunAgentDialog", () => ({
  RunAgentDialog: ({
    open,
    presetSkill,
    onRun,
  }: {
    open: boolean;
    presetSkill?: string;
    onRun: (options: RunAgentOptions) => void;
  }) => (
    <div data-testid="run-agent-dialog">
      {String(open)}:{presetSkill}
      <button
        onClick={() =>
          onRun({
            skill: "wiki-ingest",
            route: "auto",
            agent: null,
            provider: null,
            checkpoint: true,
            background: true,
          })
        }
      >
        Run through dialog
      </button>
    </div>
  ),
}));
vi.mock("../../features/settings/SettingsDialog", () => ({
  SettingsDialog: ({
    open,
    onSaveProvider,
  }: {
    open: boolean;
    onSaveProvider: (config: LlmProviderConfig) => void;
  }) => (
    <div data-testid="settings-dialog">
      {String(open)}
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
  startCompile: vi.fn(),
  startDeepLint: vi.fn(),
  startExport: vi.fn(),
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
const agentWorkflow = {
  agents: [],
  defaultAgentKind: null,
  dialogOpen: true,
  dialogPreset: "wiki-lint",
  openRunDialog: vi.fn(),
  closeRunDialog: vi.fn(),
  setDefaultAgent: vi.fn(),
  runAgent: mocks.runAgent,
};

beforeEach(() => {
  vi.clearAllMocks();
  mocks.useAiCapabilities.mockReturnValue(capabilities);
  mocks.useTaskLauncher.mockReturnValue(taskLauncher);
  mocks.useImportWorkflow.mockReturnValue(importWorkflow);
  mocks.useProviderWorkflow.mockReturnValue(providerWorkflow);
  mocks.useAgentWorkflow.mockReturnValue(agentWorkflow);
  useProjectStore.setState({ currentProject: project });
  useNavigationStore.setState({
    activeView: "import",
    settingsOpen: true,
    rightPanelOpen: false,
    workspaceFocus: null,
  });
  useTaskStore.setState({ tasks: [] });
});

describe("WorkspaceController", () => {
  it("composes project workflows once and wires Import, Agent, and Provider callbacks", () => {
    render(<WorkspaceController />);

    expect(mocks.useAiCapabilities).toHaveBeenCalledWith(project, true);
    expect(mocks.useTaskLauncher).toHaveBeenCalledWith(project);
    expect(mocks.useImportWorkflow).toHaveBeenCalledWith(
      project,
      "import",
      taskLauncher,
    );
    expect(mocks.useProviderWorkflow).toHaveBeenCalledWith(project, capabilities);
    expect(mocks.useAgentWorkflow).toHaveBeenCalledWith(
      project,
      capabilities,
      taskLauncher,
    );

    fireEvent.click(screen.getByRole("button", { name: "Import through router" }));
    expect(mocks.addPaths).toHaveBeenCalledWith(["C:/source.pdf"]);
    fireEvent.click(screen.getByRole("button", { name: "Run through dialog" }));
    expect(mocks.runAgent).toHaveBeenCalledWith(
      expect.objectContaining({ skill: "wiki-ingest" }),
    );
    fireEvent.click(screen.getByRole("button", { name: "Save through settings" }));
    expect(mocks.saveProvider).toHaveBeenCalledWith(
      expect.objectContaining({ provider: "ollama" }),
    );
  });

  it("switches routed views without replacing the current project", () => {
    render(<WorkspaceController />);
    expect(screen.getByTestId("workspace-router")).toHaveTextContent("router:import");

    act(() => useNavigationStore.getState().setActiveView("agent"));

    expect(screen.getByTestId("workspace-router")).toHaveTextContent("router:agent");
    expect(useProjectStore.getState().currentProject).toBe(project);
    expect(mocks.useAiCapabilities).toHaveBeenLastCalledWith(project, true);
  });
});
