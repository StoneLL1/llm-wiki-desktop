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
  projectKey: "project-a\0D:/wiki/project-a",
  session: null,
  readiness: null,
  bootstrapState: "loading",
  visibleItems: [],
  counts: { all: 0, active: 0, ready: 0, needsAction: 0, failed: 0, completed: 0 },
  progress: { completed: 0, total: 0, active: 0 },
  selectedItemId: null,
  filter: "all",
  addPaths: vi.fn(),
  addUrl: vi.fn(),
  setItemSelected: vi.fn(),
  startItems: vi.fn(),
  retryItem: vi.fn(),
  cancelItem: vi.fn(),
  skipItem: vi.fn(),
  authorizeLocalAsr: vi.fn(),
  refreshSession: vi.fn(),
  selectItem: vi.fn(),
  setFilter: vi.fn(),
  loadPreview: vi.fn(),
  loadSession: vi.fn(),
  isConfirming: false,
  requestClipboard: vi.fn(),
  confirm: vi.fn(),
  getAgentPolicy: vi.fn(),
  setAgentPolicy: vi.fn(),
  invokeLocalAgent: vi.fn(),
  previewByokScope: vi.fn(),
  approveByokAssistance: vi.fn(),
  acceptAgentCandidate: vi.fn(),
  selectAgentCandidate: vi.fn(),
  discardAgentCandidate: vi.fn(),
  beginLogin: vi.fn(),
  completeLogin: vi.fn(),
  revokeLogin: vi.fn(),
  authorizePrivateTarget: vi.fn(),
  getCapabilityRequirement: vi.fn(),
  installCapability: vi.fn(),
  scanMigration: vi.fn(),
  planMigration: vi.fn(),
  applyMigration: vi.fn(),
  getMigrationStatus: vi.fn(),
  resumeMigration: vi.fn(),
  listHistory: vi.fn(),
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
