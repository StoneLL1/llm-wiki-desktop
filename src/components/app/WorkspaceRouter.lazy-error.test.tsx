import { render, screen } from "@testing-library/react";
import { afterAll, describe, expect, it, vi } from "vitest";

import type { AgentWorkflow } from "../../features/agent/useAgentWorkflow";
import type { ImportWorkflow } from "../../features/import/useImportWorkflow";
import type { AiCapabilitiesWorkflow } from "../../hooks/useAiCapabilities";
import type { TaskLauncher } from "../../hooks/useTaskLauncher";

vi.mock("../../features/agent/AgentView", () => {
  throw new Error("agent chunk missing");
});

import { WorkspaceRouter } from "./WorkspaceRouter";

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

afterAll(() => {
  consoleError.mockRestore();
});

describe("WorkspaceRouter lazy failure", () => {
  it("contains a rejected lazy chunk and exposes local recovery", async () => {
    render(
      <WorkspaceRouter
        activeView="agent"
        capabilities={capabilities}
        taskLauncher={taskLauncher}
        importWorkflow={importWorkflow}
        agentWorkflow={agentWorkflow}
        tasks={[]}
        onOpenTask={vi.fn()}
        onNavigate={vi.fn()}
      />,
    );

    expect(await screen.findByRole("alert")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /reload|retry/i }),
    ).toBeInTheDocument();
  });
});
