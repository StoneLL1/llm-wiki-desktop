import { render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { AiCapabilitiesWorkflow } from "../../hooks/useAiCapabilities";
import type { TaskLauncher } from "../../hooks/useTaskLauncher";
import type { AgentWorkflow } from "../../features/agent/useAgentWorkflow";
import type { ImportWorkflow } from "../../features/import/useImportWorkflow";

const consoleError = vi.spyOn(console, "error").mockImplementation(() => {});

const capabilities: AiCapabilitiesWorkflow = {
  agents: [],
  providers: [],
  refreshing: false,
  refresh: vi.fn(),
};
const taskLauncher: TaskLauncher = {
  startCompile: vi.fn(),
  startDeepLint: vi.fn(),
  startExport: vi.fn(),
  cancel: vi.fn(),
};
const importWorkflow: ImportWorkflow = {
  importedSources: [],
  isConfirming: false,
  requestPreview: vi.fn(),
  requestClipboard: vi.fn(),
  requestUrl: vi.fn(),
  requestDeleteSource: vi.fn(),
  requestReplaceSource: vi.fn(),
  confirm: vi.fn(),
};
const agentWorkflow: AgentWorkflow = {
  agents: [],
  defaultAgentKind: null,
  dialogOpen: false,
  dialogPreset: undefined,
  openRunDialog: vi.fn(),
  closeRunDialog: vi.fn(),
  setDefaultAgent: vi.fn(),
  runAgent: vi.fn(),
};

function installViewMocks() {
  vi.doMock("../../features/dashboard/DashboardView", () => ({
    DashboardView: () => <div data-testid="dashboard-view" />,
  }));
  vi.doMock("../../features/wiki/WikiView", () => ({
    WikiView: () => <div data-testid="wiki-view" />,
  }));
  vi.doMock("../../features/chat/ChatView", () => ({
    ChatView: () => <div data-testid="chat-view" />,
  }));
  vi.doMock("../../features/graph/GraphView", () => ({
    GraphView: () => <div data-testid="graph-view" />,
  }));
  vi.doMock("../../features/lint/LintView", () => ({
    LintView: () => <div data-testid="lint-view" />,
  }));
  vi.doMock("../../features/exports/ExportsView", () => ({
    ExportsView: () => <div data-testid="exports-view" />,
  }));
  vi.doMock("../../features/import/ImportView", () => ({
    ImportView: () => <div data-testid="import-view" />,
  }));
  vi.doMock("../../features/agent/AgentView", () => ({
    AgentView: () => <div data-testid="agent-view" />,
  }));
}

const sharedProps = {
  capabilities,
  taskLauncher,
  importWorkflow,
  agentWorkflow,
  tasks: [],
  onOpenTask: vi.fn(),
  onNavigate: vi.fn(),
};

afterEach(() => {
  consoleError.mockClear();
  vi.resetModules();
  vi.clearAllMocks();
});

describe("WorkspaceRouter", () => {
  it("maps every workspace view while keeping Dashboard static", async () => {
    installViewMocks();
    const { WorkspaceRouter } = await import("./WorkspaceRouter");
    const { rerender } = render(
      <WorkspaceRouter activeView="dashboard" {...sharedProps} />,
    );

    expect(screen.getByTestId("dashboard-view")).toBeInTheDocument();
    for (const view of [
      "wiki",
      "chat",
      "graph",
      "lint",
      "exports",
      "import",
      "agent",
    ] as const) {
      rerender(<WorkspaceRouter activeView={view} {...sharedProps} />);
      expect(await screen.findByTestId(`${view}-view`)).toBeInTheDocument();
    }
  });

  it("contains a rejected lazy chunk and exposes local recovery", async () => {
    installViewMocks();
    vi.doMock("../../features/agent/AgentView", () => {
      throw new Error("agent chunk missing");
    });
    const { WorkspaceRouter } = await import("./WorkspaceRouter");

    render(<WorkspaceRouter activeView="agent" {...sharedProps} />);

    expect(await screen.findByRole("alert")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /reload|retry/i }),
    ).toBeInTheDocument();
  });
});
