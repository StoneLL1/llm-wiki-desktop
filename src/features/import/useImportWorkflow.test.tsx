import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { defaultProject } from "../../stores/projectStore";
import { useImportStore } from "../../stores/importStore";
import { useTaskStore } from "../../stores/taskStore";
import type { TaskLauncher } from "../../hooks/useTaskLauncher";
import type { ImportItem, ImportSession } from "../../types/importV2";
import type { ImportFrontendReadiness } from "../../types/importV2Presentation";
import type { BackendTask } from "../../types/task";
import type { ImportHistoryPage } from "../../types/importV2Presentation";
import type { LegacyInventory, MigrationConfirmation, MigrationPlan } from "../../types/importV2Migration";
import type { AppView } from "../../stores/navigationStore";
import { notifyTaskEventListeners } from "../../hooks/useTaskEvents";

const api = vi.hoisted(() => ({
  getReadiness: vi.fn(),
  getPreviewContent: vi.fn(),
  createSession: vi.fn(),
  getSession: vi.fn(),
  addPaths: vi.fn(),
  getScanResult: vi.fn(),
  addUrl: vi.fn(),
  setSelection: vi.fn(),
  startItems: vi.fn(),
  cancelBatch: vi.fn(),
  skipItem: vi.fn(),
  confirmSession: vi.fn(),
  getAgentPolicy: vi.fn(),
  setAgentPolicy: vi.fn(),
  startAgentAssistance: vi.fn(),
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
}));
const tauriInvoke = vi.hoisted(() => vi.fn());

vi.mock("../../services/importV2Api", () => ({ importV2Api: api }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: tauriInvoke }));

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
    cancel: vi.fn().mockResolvedValue(true),
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  tauriInvoke.mockResolvedValue([]);
  Object.defineProperty(window, "__TAURI_INTERNALS__", { value: {}, configurable: true });
  useImportStore.getState().reset();
  useTaskStore.setState({ tasks: [], logs: {}, drawerOpen: false, selectedTaskId: null, runningCount: 0 });
  api.getReadiness.mockResolvedValue(readiness);
  api.createSession.mockResolvedValue(session(projectA.projectId));
  api.getSession.mockResolvedValue(session(projectA.projectId));
  api.getScanResult.mockResolvedValue({ files: [], skipped: [], truncated: false });
  api.getPreviewContent.mockResolvedValue({ sessionId: `session-${projectA.projectId}`, itemId: "file.md", candidateId: null, title: "file.md", markdown: "# Preview", truncated: false, totalBytes: 9, sha256: "hash" });
  api.addPaths.mockResolvedValue(task("add-paths"));
  api.addUrl.mockResolvedValue(session(projectA.projectId, [item("url-1")]));
  api.setSelection.mockResolvedValue(session(projectA.projectId, [item("file.md", "preview_ready")]));
  api.startItems.mockResolvedValue([]);
  api.cancelBatch.mockResolvedValue([]);
  api.skipItem.mockResolvedValue(session(projectA.projectId));
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
  api.beginLogin.mockResolvedValue({ sessionId: "connector-1", platform: "wechat", profileRef: "opaque-profile", state: "waiting_login" });
  api.completeLogin.mockResolvedValue({ sessionId: "connector-1", platform: "wechat", profileRef: "opaque-profile", state: "authenticated" });
  api.revokeLogin.mockResolvedValue(undefined);
  api.authorizePrivateTarget.mockResolvedValue("grant-opaque");
  api.getCapabilityRequirement.mockResolvedValue({
    requirement: { capabilityId: "browser-runtime", minimumVersion: "1.0.0", protocolVersion: "2", targetTriple: "x86_64-pc-windows-msvc", acceptedLicenseExpressions: ["Apache-2.0"] },
    route: "web.generic.browser",
    available: false,
    installable: false,
    compressedBytes: null,
    installedBytes: null,
    modelBytes: null,
    license: "Apache-2.0",
    fallback: "Use the signed release pack.",
  });
  api.installCapability.mockResolvedValue(task("capability-task", projectA.projectId, "queued"));
  const inventory: LegacyInventory = { schemaVersion: 1, projectIdentity: "identity-a", fingerprint: "inventory-fingerprint", records: [], warnings: [], scannedFiles: [] };
  const migrationPlan: MigrationPlan = { planVersion: 2, v2IndexFingerprint: "index-fingerprint", inventoryFingerprint: inventory.fingerprint, candidates: [], summary: { total: 0, automaticLinks: 0, proposedRecords: 0, conflicts: 0, legacyUnmanaged: 0, warnings: 0 } };
  const migrationConfirmation: MigrationConfirmation = { planFingerprint: "plan-fingerprint", token: "opaque-token", acknowledgeNoGitRollback: true };
  const history: ImportHistoryPage = { entries: [], legacyReadOnly: [], nextCursor: null, warnings: [] };
  api.scanMigration.mockResolvedValue(inventory);
  api.planMigration.mockResolvedValue(migrationPlan);
  api.applyMigration.mockResolvedValue(task("migration-task", projectA.projectId, "queued"));
  api.getMigrationStatus.mockResolvedValue({ status: "dry_run_ready", planFingerprint: migrationConfirmation.planFingerprint, report: null });
  api.resumeMigration.mockResolvedValue(task("migration-resume-task", projectA.projectId, "queued"));
  api.listHistory.mockResolvedValue(history);
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

  it("keeps the session and batch context when the user visits another workspace view", async () => {
    const queued = item("queued.md");
    const started = task("queued-task");
    api.createSession.mockResolvedValue(session(projectA.projectId, [queued]));
    api.startItems.mockResolvedValue([started]);
    const { result, rerender } = renderHook(({ activeView }: { activeView: AppView }) => useImportWorkflow(projectA, activeView, launcher()), {
      initialProps: { activeView: "import" as AppView },
    });

    await waitFor(() => expect(result.current.bootstrapState).toBe("ready"));
    await act(async () => result.current.startItems(["queued.md"]));
    expect(result.current.batch).toMatchObject({ total: 1, active: 1, tasks: [{ id: "queued-task" }] });

    rerender({ activeView: "wiki" });
    expect(result.current.session?.sessionId).toBe(`session-${projectA.projectId}`);
    expect(result.current.batch).toMatchObject({ total: 1, active: 1 });
    rerender({ activeView: "import" });
    expect(api.createSession).toHaveBeenCalledTimes(1);
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

  it("keeps the import surface usable while migration activation is pending", async () => {
    api.getReadiness.mockResolvedValue({ ...readiness, active: false, migrationStatus: "awaiting_confirmation" });

    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));

    await waitFor(() => expect(result.current.bootstrapState).toBe("ready"));
    expect(result.current.session?.projectId).toBe(projectA.projectId);
    expect(api.createSession).toHaveBeenCalledWith({
      projectId: projectA.projectId,
      projectRootPath: projectA.rootPath,
      resourceMode: "balanced",
    });
  });

  it("keeps V2 usable when optional migration readiness cannot be read", async () => {
    api.getReadiness.mockRejectedValue(new Error("MIGRATION_REPORT_INVALID: old metadata"));

    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));

    await waitFor(() => expect(result.current.bootstrapState).toBe("ready"));
    expect(result.current.session?.projectId).toBe(projectA.projectId);
    expect(result.current.readiness).toBeNull();
    expect(result.current.readinessWarning).toContain("MIGRATION_REPORT_INVALID");
    expect(api.createSession).toHaveBeenCalledWith({
      projectId: projectA.projectId,
      projectRootPath: projectA.rootPath,
      resourceMode: "balanced",
    });
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

  it("keeps the source entry busy through discovery and surfaces task progress", async () => {
    const scanTask = task("scan-task");
    api.addPaths.mockResolvedValue(scanTask);
    const taskLauncher = launcher();
    const { result } = renderHook(() => useImportWorkflow(projectA, "import", taskLauncher));
    await waitFor(() => expect(result.current.bootstrapState).toBe("ready"));

    await act(async () => result.current.addPaths(["D:\\知识库\\资料"]));
    expect(result.current.isAddingPaths).toBe(true);
    expect(result.current.discoveryTask?.id).toBe("scan-task");

    const progressTask = { ...scanTask, status: "running" as const, progress: { current: 12, total: null, label: "Discovering files" } };
    useTaskStore.getState().upsertTask(progressTask);
    await act(async () => notifyTaskEventListeners({
      eventId: "scan-progress",
      eventType: "task_updated",
      projectId: projectA.projectId,
      taskId: scanTask.id,
      timestamp: "2026-07-14T00:00:00Z",
      payload: progressTask,
    }));
    expect(result.current.discoveryTask?.progress?.current).toBe(12);

    const completedTask = {
      ...progressTask,
      status: "succeeded" as const,
      result: { summary: "Added 12 files; skipped 2.", affectedPaths: ["scan.json"] },
    };
    api.getScanResult.mockResolvedValueOnce({
      files: [],
      skipped: [{ sourcePath: "D:/Wiki/internal.md", relativePath: "internal.md", reason: "project_internal", detail: "internal" }],
      truncated: false,
    });
    useTaskStore.getState().upsertTask(completedTask);
    await act(async () => notifyTaskEventListeners({
      eventId: "scan-completed",
      eventType: "task_completed",
      projectId: projectA.projectId,
      taskId: scanTask.id,
      timestamp: "2026-07-14T00:00:01Z",
      payload: completedTask,
    }));
    await waitFor(() => expect(result.current.isAddingPaths).toBe(false));
    expect(result.current.discoveryTask?.result?.summary).toContain("Added 12 files");
    await waitFor(() => expect(result.current.discoveryScan?.skipped[0]?.reason).toBe("project_internal"));
  });

  it("adds a selected Markdown file when the terminal task snapshot arrives without an event", async () => {
    const scanQueued = task("markdown-scan");
    const scanSucceeded = {
      ...scanQueued,
      status: "succeeded" as const,
      result: { summary: "Added 1 file; skipped 0.", affectedPaths: ["scan.json"] },
    };
    let listTasksCalls = 0;
    tauriInvoke.mockImplementation(async (command: string) => {
      if (command !== "list_tasks") return [];
      listTasksCalls += 1;
      return [listTasksCalls === 1 ? scanQueued : scanSucceeded];
    });
    api.addPaths.mockResolvedValue(scanQueued);
    api.getSession.mockResolvedValue(session(projectA.projectId, [item("notes.md")]));

    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));
    await waitFor(() => expect(result.current.bootstrapState).toBe("ready"));

    await act(async () => result.current.addPaths(["D:\\sources\\notes.md"]));

    await waitFor(() => expect(result.current.session?.items.map((entry) => entry.input.displayName)).toEqual(["notes.md"]), { timeout: 2_000 });
    expect(listTasksCalls).toBeGreaterThanOrEqual(2);
    expect(api.startItems).toHaveBeenCalledWith({
      projectId: projectA.projectId,
      projectRootPath: projectA.rootPath,
      sessionId: "session-project-a",
      itemIds: ["notes.md"],
    });
  });

  it("does not merge an individual retry into a previously active batch", async () => {
    const first = item("first.md");
    const second = item("second.md");
    const firstTask = task("first-task");
    const secondTask = task("second-task");
    api.createSession.mockResolvedValue(session(projectA.projectId, [first, second]));
    api.startItems.mockResolvedValueOnce([firstTask]).mockResolvedValueOnce([secondTask]);
    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));
    await waitFor(() => expect(result.current.session?.items).toHaveLength(2));

    await act(async () => result.current.startItems(["first.md"]));
    await act(async () => result.current.startItems(["second.md"]));
    expect(result.current.batches?.map((batch) => batch.tasks[0]?.id)).toEqual(["first-task", "second-task"]);
  });

  it("keeps parallel backend batches independent and cancels only the requested batch", async () => {
    const first = item("first.md");
    const second = item("second.md");
    api.createSession.mockResolvedValue(session(projectA.projectId, [first, second]));
    api.startItems
      .mockResolvedValueOnce([{ ...task("first-task"), batchId: "batch-a" }])
      .mockResolvedValueOnce([{ ...task("second-task"), batchId: "batch-b" }]);
    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));
    await waitFor(() => expect(result.current.session?.items).toHaveLength(2));

    await act(async () => result.current.startItems(["first.md"]));
    await act(async () => result.current.startItems(["second.md"]));

    expect(result.current.batches?.map((batch) => batch.id)).toEqual(["batch-a", "batch-b"]);
    await act(async () => result.current.cancelBatch?.("batch-a"));
    expect(api.cancelBatch).toHaveBeenCalledWith({
      projectId: projectA.projectId,
      projectRootPath: projectA.rootPath,
      sessionId: `session-${projectA.projectId}`,
      batchId: "batch-a",
    });
  });

  it("rebuilds an unfinished batch from the recovered session and task identity", async () => {
    const recovered = item("recover.md", "extracting");
    recovered.taskId = "recover-task";
    api.getReadiness.mockResolvedValue({ ...readiness, unfinishedSessionId: "session-recover" });
    api.getSession.mockResolvedValue({ ...session(projectA.projectId, [recovered]), sessionId: "session-recover" });
    useTaskStore.getState().upsertTask({ ...task("recover-task"), batchId: "batch-recovered" });

    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));

    await waitFor(() => expect(result.current.batches?.[0]).toMatchObject({ id: "batch-recovered", total: 1, active: 1 }));
  });

  it("reattaches a persisted discovery task after the import workspace is reopened", async () => {
    api.getReadiness.mockResolvedValue({ ...readiness, unfinishedSessionId: "session-recover" });
    api.getSession.mockResolvedValue({ ...session(projectA.projectId), sessionId: "session-recover", discoveryTaskId: "scan-task" });
    const scanTask = task("scan-task", projectA.projectId, "failed");
    useTaskStore.getState().upsertTask({ ...scanTask, error: { code: "TASK_RECOVERY", message: "The scan was interrupted.", details: null, recoverable: true, userActionRequired: true } });

    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));

    await waitFor(() => expect(result.current.discoveryTask).toMatchObject({ id: "scan-task", status: "failed" }));
    expect(result.current.discoveryTaskUnavailable).toBe(false);
  });

  it("passes a selected recovery route through retry and persists skip as a session update", async () => {
    const failed = item("failed.pdf", "failed");
    api.createSession.mockResolvedValue(session(projectA.projectId, [failed]));
    api.startItems.mockResolvedValue([]);
    api.skipItem.mockResolvedValue(session(projectA.projectId, [item("failed.pdf", "skipped")]));
    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));
    await waitFor(() => expect(result.current.session?.items).toHaveLength(1));

    await act(async () => result.current.retryItem("failed.pdf", "enable_ocr"));
    expect(api.startItems).toHaveBeenCalledWith(expect.objectContaining({
      itemIds: ["failed.pdf"],
      recoveryAction: "enable_ocr",
    }));

    await act(async () => result.current.skipItem?.("failed.pdf"));
    expect(api.skipItem).toHaveBeenCalledWith({
      projectId: projectA.projectId,
      projectRootPath: projectA.rootPath,
      sessionId: `session-${projectA.projectId}`,
      itemId: "failed.pdf",
    });
  });

  it("settles a discovery when its terminal task event arrived before IPC returned", async () => {
    const scanTask = task("fast-scan-task");
    api.addPaths.mockResolvedValue(scanTask);
    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));
    await waitFor(() => expect(result.current.bootstrapState).toBe("ready"));

    useTaskStore.getState().upsertTask({ ...scanTask, status: "succeeded", result: { summary: "Added 1 file", affectedPaths: [] } });
    await act(async () => result.current.addPaths(["D:\\sources\\fast"]));

    await waitFor(() => expect(result.current.isAddingPaths).toBe(false));
    expect(result.current.discoveryTask?.status).toBe("succeeded");
    expect(api.getSession).toHaveBeenCalledWith(expect.objectContaining({ sessionId: "session-project-a" }));
  });

  it("optimistically updates selection and ignores a duplicate item action while pending", async () => {
    const ready = item("ready.md", "preview_ready");
    api.createSession.mockResolvedValue(session(projectA.projectId, [ready]));
    let resolveSelection!: (value: ImportSession) => void;
    api.setSelection.mockReturnValue(new Promise<ImportSession>((resolve) => { resolveSelection = resolve; }));
    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));
    await waitFor(() => expect(result.current.session?.items).toHaveLength(1));

    act(() => { void result.current.setItemSelected("ready.md", false); });
    await waitFor(() => expect(result.current.session?.items[0].selected).toBe(false));
    expect(result.current.pendingItemIds?.has("ready.md")).toBe(true);

    act(() => { void result.current.setItemSelected("ready.md", true); });
    expect(api.setSelection).toHaveBeenCalledTimes(1);

    resolveSelection(session(projectA.projectId, [item("ready.md", "preview_ready")]));
    await waitFor(() => expect(result.current.pendingItemIds?.has("ready.md")).toBe(false));
  });

  it("keeps a started item locked until its task reaches a terminal state", async () => {
    const queued = item("queued.md");
    api.createSession.mockResolvedValue(session(projectA.projectId, [queued]));
    let resolveStart!: (value: BackendTask[]) => void;
    api.startItems.mockReturnValue(new Promise<BackendTask[]>((resolve) => { resolveStart = resolve; }));
    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));
    await waitFor(() => expect(result.current.session?.items).toHaveLength(1));

    act(() => { void result.current.startItems(["queued.md"]); });
    await waitFor(() => expect(result.current.pendingItemIds?.has("queued.md")).toBe(true));
    act(() => { void result.current.startItems(["queued.md"]); });
    expect(api.startItems).toHaveBeenCalledTimes(1);

    const started = task("queued-task");
    await act(async () => resolveStart([started]));
    expect(result.current.pendingItemIds?.has("queued.md")).toBe(true);
    expect(result.current.batch).toMatchObject({ total: 1, active: 1, processed: 0 });
    useTaskStore.getState().upsertTask({ ...started, status: "succeeded" });
    await act(async () => notifyTaskEventListeners({
      eventId: "queued-task-completed",
      eventType: "task_completed",
      projectId: projectA.projectId,
      taskId: started.id,
      timestamp: "2026-07-14T00:00:00Z",
      payload: { ...started, status: "succeeded" },
    }));
    await waitFor(() => expect(result.current.pendingItemIds?.has("queued.md")).toBe(false));
    await waitFor(() => expect(result.current.batch).toMatchObject({ total: 1, active: 0, processed: 1, completed: 1 }));
  });

  it("releases the item lock when parsing pauses for user confirmation", async () => {
    const queued = item("review.md");
    api.createSession.mockResolvedValue(session(projectA.projectId, [queued]));
    let resolveStart!: (value: BackendTask[]) => void;
    api.startItems.mockReturnValue(new Promise<BackendTask[]>((resolve) => { resolveStart = resolve; }));
    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));
    await waitFor(() => expect(result.current.session?.items).toHaveLength(1));

    act(() => { void result.current.startItems(["review.md"]); });
    const started = task("review-task");
    await act(async () => resolveStart([started]));
    expect(result.current.pendingItemIds?.has("review.md")).toBe(true);

    const waitingTask = { ...started, status: "waiting_for_confirmation" as const };
    useTaskStore.getState().upsertTask(waitingTask);
    await act(async () => notifyTaskEventListeners({
      eventId: "review-task-waiting",
      eventType: "task_updated",
      projectId: projectA.projectId,
      taskId: waitingTask.id,
      timestamp: "2026-07-14T00:00:00Z",
      payload: waitingTask,
    }));

    await waitFor(() => expect(result.current.pendingItemIds?.has("review.md")).toBe(false));
    expect(result.current.batch).toMatchObject({ total: 1, active: 0, processed: 1, waitingForConfirmation: 1 });
  });

  it("releases the item lock for the backend confirmation_requested event", async () => {
    const queued = item("markdown.md");
    api.createSession.mockResolvedValue(session(projectA.projectId, [queued]));
    const started = task("markdown-task");
    api.startItems.mockResolvedValue([started]);
    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));
    await waitFor(() => expect(result.current.session?.items).toHaveLength(1));

    await act(async () => result.current.startItems(["markdown.md"]));
    expect(result.current.pendingItemIds?.has("markdown.md")).toBe(true);

    const waitingTask = { ...started, status: "waiting_for_confirmation" as const };
    useTaskStore.getState().upsertTask(waitingTask);
    await act(async () => notifyTaskEventListeners({
      eventId: "markdown-task-confirmation",
      eventType: "confirmation_requested",
      projectId: projectA.projectId,
      taskId: waitingTask.id,
      timestamp: "2026-07-14T00:00:00Z",
      payload: waitingTask,
    }));

    await waitFor(() => expect(result.current.pendingItemIds?.has("markdown.md")).toBe(false));
  });

  it("keeps a child task settled when its waiting snapshot arrived before IPC returned", async () => {
    const queued = item("fast.md");
    api.createSession.mockResolvedValue(session(projectA.projectId, [queued]));
    const started = task("fast-item-task");
    api.startItems.mockResolvedValue([started]);
    const waitingTask = { ...started, status: "waiting_for_confirmation" as const };
    useTaskStore.getState().upsertTask(waitingTask);
    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));
    await waitFor(() => expect(result.current.session?.items).toHaveLength(1));

    await act(async () => result.current.startItems(["fast.md"]));

    await waitFor(() => expect(result.current.pendingItemIds?.has("fast.md")).toBe(false));
    expect(useTaskStore.getState().tasks.find((entry) => entry.id === started.id)?.status).toBe("waiting_for_confirmation");
  });

  it("ignores a duplicate confirm action while the commit task is being created", async () => {
    api.createSession.mockResolvedValue(session(projectA.projectId, [item("ready.md", "preview_ready")]));
    let resolveConfirm!: (value: BackendTask) => void;
    api.confirmSession.mockReturnValue(new Promise<BackendTask>((resolve) => { resolveConfirm = resolve; }));
    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));
    await waitFor(() => expect(result.current.session?.items).toHaveLength(1));

    const decisions = [{ itemId: "ready.md", conflictAction: null, expectedWikiHash: null }];
    act(() => { void result.current.confirm(decisions); });
    expect(useImportStore.getState().isConfirming).toBe(true);
    act(() => { void result.current.confirm(decisions); });
    expect(api.confirmSession).toHaveBeenCalledTimes(1);
    await act(async () => resolveConfirm(task("confirm-task")));
  });

  it("unlocks the commit bar when a confirmation task finished before IPC returned", async () => {
    api.createSession.mockResolvedValue(session(projectA.projectId, [item("ready.md", "preview_ready")]));
    let resolveConfirm!: (value: BackendTask) => void;
    api.confirmSession.mockReturnValue(new Promise<BackendTask>((resolve) => { resolveConfirm = resolve; }));
    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));
    await waitFor(() => expect(result.current.session?.items).toHaveLength(1));

    act(() => { void result.current.confirm([{ itemId: "ready.md", conflictAction: "create_new", expectedWikiHash: null }]); });
    await waitFor(() => expect(result.current.isConfirming).toBe(true));

    const completed = task("confirm-race", projectA.projectId, "succeeded");
    useTaskStore.getState().upsertTask(completed);
    await act(async () => resolveConfirm(task("confirm-race")));

    await waitFor(() => expect(result.current.isConfirming).toBe(false));
    expect(useTaskStore.getState().tasks.find((entry) => entry.id === completed.id)?.status).toBe("succeeded");
  });

  it("refreshes the session when a started import task reaches a terminal state", async () => {
    api.createSession.mockResolvedValue(session(projectA.projectId));
    api.addUrl.mockResolvedValue(session(projectA.projectId, [item("url-1")]));
    const started = task("url-task");
    api.startItems.mockResolvedValue([started]);
    const completed = session(projectA.projectId, [item("url-1", "preview_ready")]);
    api.getSession.mockResolvedValueOnce(completed);
    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));
    await waitFor(() => expect(result.current.bootstrapState).toBe("ready"));

    await act(async () => result.current.addUrl("https://example.com/article"));
    await act(async () => notifyTaskEventListeners({
      eventId: "event-url-task",
      eventType: "task_completed",
      projectId: projectA.projectId,
      taskId: started.id,
      timestamp: "2026-07-14T00:00:00Z",
      payload: { ...started, status: "succeeded" },
    }));

    await waitFor(() => expect(result.current.session?.items[0].status).toBe("preview_ready"));
    expect(api.getSession).toHaveBeenCalledWith({
      projectId: projectA.projectId,
      projectRootPath: projectA.rootPath,
      sessionId: "session-project-a",
    });
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

  it("routes login, private-target, and capability gates through the current session identity", async () => {
    const gated = item("gated-url", "waiting_login");
    api.createSession.mockResolvedValue(session(projectA.projectId, [gated]));
    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));
    await waitFor(() => expect(result.current.session?.items).toHaveLength(1));

    await act(async () => result.current.beginLogin("gated-url", "wechat"));
    api.getSession.mockResolvedValueOnce(session(projectA.projectId, [item("gated-url", "failed")]));
    await act(async () => result.current.completeLogin("gated-url", "connector-1"));
    await act(async () => result.current.revokeLogin("connector-1"));
    await act(async () => result.current.authorizePrivateTarget("gated-url", "https://private.example.com/article"));
    await act(async () => result.current.getCapabilityRequirement("gated-url"));
    await act(async () => result.current.installCapability("gated-url", "browser-runtime"));

    expect(api.beginLogin).toHaveBeenCalledWith(expect.objectContaining({ sessionId: "session-project-a", itemId: "gated-url", platform: "wechat" }));
    expect(api.completeLogin).toHaveBeenCalledWith(expect.objectContaining({ importSessionId: "session-project-a", connectorSessionId: "connector-1" }));
    expect(api.revokeLogin).toHaveBeenCalledWith({ sessionId: "connector-1" });
    expect(api.authorizePrivateTarget).toHaveBeenCalledWith(expect.objectContaining({ url: "https://private.example.com/article" }));
    expect(api.getCapabilityRequirement).toHaveBeenCalledWith(expect.objectContaining({ itemId: "gated-url" }));
    expect(api.installCapability).toHaveBeenCalledWith(expect.objectContaining({ capabilityId: "browser-runtime", acknowledgeInstall: true }));
    expect(useTaskStore.getState().tasks).toContainEqual(expect.objectContaining({ id: "capability-task" }));
  });

  it("routes migration and history reads through the current project identity", async () => {
    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));
    await waitFor(() => expect(result.current.bootstrapState).toBe("ready"));

    const inventory = await act(async () => result.current.scanMigration());
    const migrationPlan = await act(async () => result.current.planMigration(inventory!));
    const confirmation: MigrationConfirmation = { planFingerprint: "plan-fingerprint", token: "opaque-token", acknowledgeNoGitRollback: true };
    await act(async () => result.current.applyMigration(migrationPlan!, confirmation));
    await act(async () => result.current.getMigrationStatus());
    await act(async () => result.current.resumeMigration(migrationPlan!, confirmation));
    await act(async () => result.current.listHistory("cursor-1"));

    expect(api.scanMigration).toHaveBeenCalledWith({ projectId: projectA.projectId, projectRootPath: projectA.rootPath });
    expect(api.planMigration).toHaveBeenCalledWith({ projectId: projectA.projectId, projectRootPath: projectA.rootPath, inventory });
    expect(api.applyMigration).toHaveBeenCalledWith({ projectId: projectA.projectId, projectRootPath: projectA.rootPath, plan: migrationPlan, confirmation });
    expect(api.getMigrationStatus).toHaveBeenCalledWith({ projectId: projectA.projectId, projectRootPath: projectA.rootPath });
    expect(api.resumeMigration).toHaveBeenCalledWith({ projectId: projectA.projectId, projectRootPath: projectA.rootPath, plan: migrationPlan, confirmation });
    expect(api.listHistory).toHaveBeenCalledWith({ projectId: projectA.projectId, projectRootPath: projectA.rootPath, cursor: "cursor-1", limit: 50 });
    expect(useTaskStore.getState().tasks.map((entry) => entry.id)).toEqual(expect.arrayContaining(["migration-task", "migration-resume-task"]));
  });
});
