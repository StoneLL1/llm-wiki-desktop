import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { defaultProject } from "../../stores/projectStore";
import { useImportStore } from "../../stores/importStore";
import { useTaskStore } from "../../stores/taskStore";
import type { TaskLauncher } from "../../hooks/useTaskLauncher";
import type { ImportItem, ImportSession } from "../../types/importV2";
import type { ImportFrontendReadiness } from "../../types/importV2Presentation";
import type { BackendTask } from "../../types/task";

const api = vi.hoisted(() => ({
  getReadiness: vi.fn(),
  createSession: vi.fn(),
  getSession: vi.fn(),
  addPaths: vi.fn(),
  addUrl: vi.fn(),
  setSelection: vi.fn(),
  startItems: vi.fn(),
  confirmSession: vi.fn(),
  getAgentPolicy: vi.fn(),
  setAgentPolicy: vi.fn(),
  startAgentAssistance: vi.fn(),
  previewByokScope: vi.fn(),
  approveByokAssistance: vi.fn(),
  acceptAgentCandidate: vi.fn(),
  selectAgentCandidate: vi.fn(),
  discardAgentCandidate: vi.fn(),
}));

vi.mock("../../services/importV2Api", () => ({ importV2Api: api }));

import { useImportWorkflow } from "./useImportWorkflow";

const projectA = {
  ...defaultProject,
  projectId: "project-a",
  name: "Project A",
  rootPath: "D:/知识库/project-a",
};
const projectB = {
  ...projectA,
  projectId: "project-b",
  name: "Project B",
  rootPath: "D:/知识库/project-b",
};

const readiness: ImportFrontendReadiness = {
  backendVersion: "2.0.0",
  active: true,
  migrationStatus: "applied",
  unfinishedSessionId: null,
  legacyHistoryAvailable: false,
};

function task(id: string, projectId = projectA.projectId, status: BackendTask["status"] = "queued"): BackendTask {
  return {
    id,
    taskType: "import",
    projectId,
    title: `Import ${id}`,
    status,
    progress: null,
    startedAt: "2026-07-13T00:00:00Z",
    updatedAt: "2026-07-13T00:00:00Z",
    completedAt: null,
    cancellable: true,
    logPath: null,
    result: null,
    error: null,
  };
}

function item(itemId: string, status: ImportItem["status"] = "queued"): ImportItem {
  return {
    itemId,
    input: {
      kind: itemId.startsWith("url") ? "url" : "file",
      displayName: itemId,
      locator: itemId.startsWith("url") ? `https://example.com/${itemId}` : `C:\\sources\\${itemId}`,
      normalizedLocator: null,
    },
    status,
    selected: status === "preview_ready",
    taskId: null,
    progress: null,
    attempts: [],
    preview: null,
    issue: null,
  };
}

function session(projectId: string, items: ImportItem[] = []): ImportSession {
  return {
    schemaVersion: 2,
    sessionId: `session-${projectId}`,
    projectId,
    status: "draft",
    resourceMode: "balanced",
    createdAt: "2026-07-13T00:00:00Z",
    updatedAt: "2026-07-13T00:00:00Z",
    items,
  };
}

function launcher(): TaskLauncher {
  return {
    startCompile: vi.fn(),
    startDeepLint: vi.fn(),
    startExport: vi.fn(),
    cancel: vi.fn().mockResolvedValue(undefined),
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  Object.defineProperty(window, "__TAURI_INTERNALS__", { value: {}, configurable: true });
  useImportStore.getState().reset();
  useTaskStore.setState({ tasks: [], logs: {}, drawerOpen: false, selectedTaskId: null, runningCount: 0 });
  api.getReadiness.mockResolvedValue(readiness);
  api.createSession.mockResolvedValue(session(projectA.projectId));
  api.getSession.mockResolvedValue(session(projectA.projectId));
  api.addPaths.mockResolvedValue(task("add-paths"));
  api.addUrl.mockResolvedValue(session(projectA.projectId, [item("url-1")]));
  api.setSelection.mockResolvedValue(session(projectA.projectId, [item("file.md", "preview_ready")]));
  api.startItems.mockResolvedValue([]);
  api.confirmSession.mockResolvedValue(task("confirm"));
  api.getAgentPolicy.mockResolvedValue({
    autoLocalOnHardFailure: false,
    autoLocalOnQualityWarning: false,
    autoByok: false,
    maxAttemptsPerItem: 1,
  });
  api.setAgentPolicy.mockImplementation(async ({ policy }: { policy: unknown }) => policy);
  api.startAgentAssistance.mockResolvedValue(task("agent-task", projectA.projectId, "queued"));
  api.previewByokScope.mockResolvedValue({
    approvalId: "approval-1",
    itemId: "failed.md",
    provider: "open_ai",
    model: "gpt-5",
    destination: "api.openai.com/v1/responses",
    publicMetadata: [],
    files: [],
    estimatedInputTokens: 100,
    estimatedCostMicros: null,
    requiresDuplicateChargeAcknowledgement: false,
    scopeSha256: "a".repeat(64),
    expiresAt: "2099-01-01T00:00:00Z",
  });
  api.approveByokAssistance.mockResolvedValue(task("byok-task", projectA.projectId, "queued"));
  api.acceptAgentCandidate.mockResolvedValue({});
  api.selectAgentCandidate.mockImplementation(async ({ itemId }: { itemId: string }) => ({
    projectId: projectA.projectId,
    sessionId: `session-${projectA.projectId}`,
    itemId,
    candidateId: "candidate-1",
    item: item(itemId, "preview_ready"),
  }));
  api.discardAgentCandidate.mockImplementation(async ({ itemId }: { itemId: string }) => ({
    projectId: projectA.projectId,
    sessionId: `session-${projectA.projectId}`,
    itemId,
    candidateId: "candidate-1",
    item: item(itemId, "failed"),
  }));
});

describe("useImportWorkflow", () => {
  it("bootstraps readiness and creates a balanced draft when no unfinished session exists", async () => {
    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));

    await waitFor(() => expect(result.current.bootstrapState).toBe("ready"));
    expect(api.getReadiness).toHaveBeenCalledWith({ projectId: projectA.projectId, projectRootPath: projectA.rootPath });
    expect(api.createSession).toHaveBeenCalledWith({
      projectId: projectA.projectId,
      projectRootPath: projectA.rootPath,
      resourceMode: "balanced",
    });
    expect(result.current.session?.sessionId).toBe("session-project-a");
  });

  it("recovers the unfinished session without creating a competing draft", async () => {
    api.getReadiness.mockResolvedValue({ ...readiness, unfinishedSessionId: "session-recover" });
    api.getSession.mockResolvedValue({ ...session(projectA.projectId, [item("recover.md")]), sessionId: "session-recover" });

    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));

    await waitFor(() => expect(result.current.session?.sessionId).toBe("session-recover"));
    expect(api.createSession).not.toHaveBeenCalled();
    expect(api.getSession).toHaveBeenCalledWith({
      projectId: projectA.projectId,
      projectRootPath: projectA.rootPath,
      sessionId: "session-recover",
    });
  });

  it("blocks the import surface until migration activation is ready", async () => {
    api.getReadiness.mockResolvedValue({ ...readiness, active: false, migrationStatus: "awaiting_confirmation" });

    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));

    await waitFor(() => expect(result.current.bootstrapState).toBe("blocked"));
    expect(result.current.session).toBeNull();
    expect(api.createSession).not.toHaveBeenCalled();
  });

  it("ignores late project A readiness and session responses after switching to project B", async () => {
    let resolveA!: (value: ImportFrontendReadiness) => void;
    const readinessA = new Promise<ImportFrontendReadiness>((resolve) => { resolveA = resolve; });
    api.getReadiness.mockImplementation(({ projectId }: { projectId: string }) =>
      projectId === projectA.projectId ? readinessA : Promise.resolve(readiness));
    api.createSession.mockImplementation(({ projectId }: { projectId: string }) => Promise.resolve(session(projectId)));

    const { result, rerender } = renderHook(({ project }) => useImportWorkflow(project, "import", launcher()), {
      initialProps: { project: projectA },
    });
    rerender({ project: projectB });
    await waitFor(() => expect(result.current.session?.projectId).toBe(projectB.projectId));

    await act(async () => resolveA(readiness));
    expect(result.current.session?.projectId).toBe(projectB.projectId);
  });

  it("adds a URL, starts only newly queued items, and upserts returned task facts", async () => {
    const existing = item("existing.md", "preview_ready");
    api.createSession.mockResolvedValue(session(projectA.projectId, [existing]));
    api.addUrl.mockResolvedValue(session(projectA.projectId, [existing, item("url-1")]));
    const started = task("url-task");
    api.startItems.mockResolvedValue([started]);
    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));
    await waitFor(() => expect(result.current.session?.items).toHaveLength(1));

    await act(async () => result.current.addUrl("https://example.com/article"));
    expect(api.addUrl).toHaveBeenCalledWith({
      projectId: projectA.projectId,
      projectRootPath: projectA.rootPath,
      sessionId: "session-project-a",
      url: "https://example.com/article",
    });
    expect(api.startItems).toHaveBeenCalledWith({
      projectId: projectA.projectId,
      projectRootPath: projectA.rootPath,
      sessionId: "session-project-a",
      itemIds: ["url-1"],
    });
    expect(useTaskStore.getState().tasks).toContainEqual(started);
  });

  it("routes selection, retry, cancellation, and confirmation through V2 contracts", async () => {
    const retryable = item("retry.md", "failed");
    retryable.taskId = "retry-task";
    api.createSession.mockResolvedValue(session(projectA.projectId, [retryable, item("ready.md", "preview_ready")]));
    const selectionSession = session(projectA.projectId, [item("retry.md", "failed"), item("ready.md", "preview_ready")]);
    selectionSession.items[0].taskId = "retry-task";
    api.setSelection.mockResolvedValue(selectionSession);
    const taskLauncher = launcher();
    const { result } = renderHook(() => useImportWorkflow(projectA, "import", taskLauncher));
    await waitFor(() => expect(result.current.session?.items).toHaveLength(2));

    await act(async () => result.current.setItemSelected("ready.md", true));
    await act(async () => result.current.retryItem("retry.md"));
    await act(async () => result.current.cancelItem("retry.md"));
    await act(async () => result.current.confirm([{ itemId: "ready.md", conflictAction: null, expectedWikiHash: null }]));

    expect(api.setSelection).toHaveBeenCalled();
    expect(api.startItems).toHaveBeenCalledWith(expect.objectContaining({ itemIds: ["retry.md"] }));
    expect(taskLauncher.cancel).toHaveBeenCalledWith("retry-task");
    expect(api.confirmSession).toHaveBeenCalledWith(expect.objectContaining({
      projectId: projectA.projectId,
      projectRootPath: projectA.rootPath,
      sessionId: "session-project-a",
    }));
  });

  it("routes Agent, BYOK, and candidate actions through typed project-scoped contracts", async () => {
    const failed = item("failed.md", "failed");
    api.createSession.mockResolvedValue(session(projectA.projectId, [failed]));
    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));
    await waitFor(() => expect(result.current.session?.items).toHaveLength(1));

    await act(async () => expect(await result.current.getAgentPolicy()).toEqual({
      autoLocalOnHardFailure: false,
      autoLocalOnQualityWarning: false,
      autoByok: false,
      maxAttemptsPerItem: 1,
    }));
    await act(async () => result.current.setAgentPolicy({
      autoLocalOnHardFailure: true,
      autoLocalOnQualityWarning: false,
      autoByok: false,
      maxAttemptsPerItem: 1,
    }, "codex"));
    await act(async () => result.current.invokeLocalAgent("failed.md", "manual", "codex"));
    await act(async () => result.current.previewByokScope("failed.md", "manual", "open_ai"));
    await act(async () => result.current.approveByokAssistance({
      itemId: "failed.md",
      trigger: "manual",
      provider: "open_ai",
      model: "gpt-5",
      approvalId: "approval-1",
      scopeSha256: "a".repeat(64),
      acknowledgePossibleDuplicateCharge: false,
    }));
    await act(async () => result.current.selectAgentCandidate({ itemId: "failed.md", candidateId: "candidate-1", mergedMarkdown: null, expectedCurrentWikiSha256: null }));
    await act(async () => result.current.discardAgentCandidate("failed.md", "candidate-1"));

    expect(api.getAgentPolicy).toHaveBeenCalledWith({ projectId: projectA.projectId, projectRootPath: projectA.rootPath });
    expect(api.setAgentPolicy).toHaveBeenCalledWith(expect.objectContaining({ projectId: projectA.projectId, localAgentKind: "codex" }));
    expect(api.startAgentAssistance).toHaveBeenCalledWith(expect.objectContaining({ sessionId: "session-project-a", itemId: "failed.md", trigger: "manual", agentKind: "codex" }));
    expect(api.previewByokScope).toHaveBeenCalledWith(expect.objectContaining({ provider: "open_ai" }));
    expect(api.approveByokAssistance).toHaveBeenCalledWith(expect.objectContaining({ approvalId: "approval-1", scopeSha256: "a".repeat(64) }));
    expect(api.selectAgentCandidate).toHaveBeenCalledWith(expect.objectContaining({ candidateId: "candidate-1" }));
    expect(api.discardAgentCandidate).toHaveBeenCalledWith(expect.objectContaining({ candidateId: "candidate-1" }));
    expect(useTaskStore.getState().tasks.map((entry) => entry.id)).toEqual(expect.arrayContaining(["agent-task", "byok-task"]));
  });
});
