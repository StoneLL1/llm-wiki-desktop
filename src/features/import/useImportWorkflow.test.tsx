import { act, render, renderHook, waitFor } from "@testing-library/react";
import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

import { defaultProject, useProjectStore } from "../../stores/projectStore";
import { importProjectKey, useImportStore } from "../../stores/importStore";
import { useNavigationStore } from "../../stores/navigationStore";
import { handleTaskEvent, useTaskStore } from "../../stores/taskStore";
import { useToastStore } from "../../stores/toastStore";
import type { TaskLauncher } from "../../hooks/useTaskLauncher";
import type { ImportCompletion, ImportItem, ImportSession } from "../../types/importV2";
import type { ImportFrontendReadiness } from "../../types/importV2Presentation";
import type { BackendTask } from "../../types/task";
import type { ProjectSessionAuthority } from "../../types/project";
import type { ImportHistoryPage } from "../../types/importV2Presentation";
import type { LegacyInventory, MigrationConfirmation, MigrationPlan, MigrationReport } from "../../types/importV2Migration";
import type { AppView } from "../../stores/navigationStore";
import {
  clearPendingTaskEvents,
  dispatchTaskEvent as notifyTaskEventListeners,
  registerTaskEventOwner,
} from "../../services/taskEventDispatcher";
import { useWikiStore } from "../wiki/wikiStore";

const api = vi.hoisted(() => ({
  getReadiness: vi.fn(),
  getPreviewContent: vi.fn(),
  createSession: vi.fn(),
  getSession: vi.fn(),
  getSessionOverview: vi.fn(),
  listSessionItems: vi.fn(),
  startSessionRecovery: vi.fn(),
  addPaths: vi.fn(),
  getScanResult: vi.fn(),
  acceptScan: vi.fn(),
  discardScan: vi.fn(),
  addText: vi.fn(),
  addUrl: vi.fn(),
  discoverCollection: vi.fn(),
  addCollectionItems: vi.fn(),
  getRemoteMediaRetentionPlan: vi.fn(),
  confirmRemoteMediaRetention: vi.fn(),
  getRestrictedContentStatus: vi.fn(),
  setSelection: vi.fn(),
  startItems: vi.fn(),
  startBatch: vi.fn(),
  cancelItem: vi.fn(),
  cancelBatch: vi.fn(),
  skipItem: vi.fn(),
  confirmSession: vi.fn(),
  startAgentAssistance: vi.fn(),
  acceptAgentCandidate: vi.fn(),
  selectAgentCandidate: vi.fn(),
  discardAgentCandidate: vi.fn(),
  beginLogin: vi.fn(),
  completeLogin: vi.fn(),
  revokeLogin: vi.fn(),
  authorizePrivateTarget: vi.fn(),
  authorizeLocalAsr: vi.fn(),
  authorizeLocalOcr: vi.fn(),
  selectSubtitle: vi.fn(),
  getCapabilityRequirement: vi.fn(),
  installCapability: vi.fn(),
  scanMigration: vi.fn(),
  planMigration: vi.fn(),
  applyMigration: vi.fn(),
  getMigrationStatus: vi.fn(),
  resumeMigration: vi.fn(),
  listHistory: vi.fn(),
  getCompletion: vi.fn(),
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

const authorityA: ProjectSessionAuthority = {
  projectId: projectA.projectId,
  canonicalRootPath: projectA.rootPath,
  canonicalIdentityKey: "identity-a",
  identityRevision: "identity-revision-a",
  authorityRevision: "authority-revision-a",
  format: "native_current",
  trust: "trusted",
  filesystemAccess: "writable",
  health: "healthy",
  layout: {
    appStateRoot: ".app",
    evidenceRoot: "raw",
    sourceWriteRoot: "wiki/sources",
    wikiWriteRoot: "wiki",
    exportRoot: "exports",
    taskStateRoot: ".app/tasks",
    workflowStateRoot: ".app/workflows",
    importStateRoot: ".app/import",
    graphCachePath: ".app/graph-cache.json",
    markdownRoots: [{ path: "wiki", role: "wiki" }],
  },
  confidence: "high",
  capabilities: ["read_markdown", "project_write"],
  warnings: [],
  layoutWarnings: [],
  git: { isRepository: true, branch: "main", head: "abc", hasChanges: false },
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
    cancel: vi.fn().mockResolvedValue(true),
  };
}

function overviewFor(value: ImportSession) {
  const active = value.items.filter((entry) => ["queued", "inspecting", "extracting", "validating", "committing"].includes(entry.status)).length;
  const ready = value.items.filter((entry) => entry.selected && entry.status === "preview_ready").length;
  return {
    ...value,
    items: undefined,
    itemCount: value.items.length,
    semanticRevision: 1,
    selectionRevision: 1,
    confirmationDigest: `digest-${value.sessionId}`,
    counts: {
      all: value.items.filter((entry) => entry.status !== "completed" && entry.status !== "skipped").length,
      active,
      ready,
      needsAction: value.items.filter((entry) => ["waiting_capability", "waiting_login", "waiting_authorization", "needs_merge"].includes(entry.status)).length,
      failed: value.items.filter((entry) => entry.status === "failed").length,
      completed: value.items.filter((entry) => entry.status === "completed").length,
      waiting: value.items.filter((entry) => ["waiting_capability", "waiting_login", "waiting_authorization"].includes(entry.status)).length,
      processed: value.items.filter((entry) => ["preview_ready", "needs_merge", "completed", "failed", "cancelled", "skipped"].includes(entry.status)).length,
      cancelled: value.items.filter((entry) => entry.status === "cancelled").length,
    },
    selection: { selected: ready, newSources: ready, updates: 0, warnings: 0, pending: 0, restricted: value.items.filter((entry) => entry.selected && entry.restrictedContent).length },
    indexState: "ready" as const,
  };
}

const completion: ImportCompletion = {
  sessionId: "session-project-a",
  batchId: "batch-complete",
  newSources: [{
    sourceId: "source-new",
    versionId: "version-new",
    wikiPath: "wiki/sources/local/资料甲.md",
    contentHash: "a".repeat(64),
  }],
  updatedSources: [{
    sourceId: "source-updated",
    versionId: "version-updated",
    wikiPath: "wiki/sources/web/example.test/资料乙.md",
    contentHash: "b".repeat(64),
  }],
  duplicateSkips: [{
    itemId: "duplicate.md",
    sourceId: "source-duplicate",
    versionId: "version-duplicate",
    contentHash: "c".repeat(64),
  }],
  warnings: [],
  failures: [],
};

let unregisterTaskEventOwner: (() => void) | null = null;

beforeAll(() => {
  unregisterTaskEventOwner = registerTaskEventOwner(handleTaskEvent);
});

afterAll(() => {
  clearPendingTaskEvents();
  unregisterTaskEventOwner?.();
});

beforeEach(() => {
  clearPendingTaskEvents();
  vi.clearAllMocks();
  tauriInvoke.mockResolvedValue([]);
  Object.defineProperty(window, "__TAURI_INTERNALS__", { value: {}, configurable: true });
  useImportStore.getState().reset();
  useToastStore.setState({ toasts: [] });
  useProjectStore.setState({ authority: null });
  useTaskStore.setState({
    activeProjectId: projectA.projectId,
    activeProjectRootPath: projectA.rootPath,
    taskById: {},
    taskIdsByProject: {},
    runningCountByProject: {},
    taskFacts: {},
    tasks: [],
    logs: {},
    activities: {},
    taskOutputs: {},
    drawerOpen: false,
    selectedTaskId: null,
    runningCount: 0,
    tasksHydrated: false,
  });
  api.getReadiness.mockResolvedValue(readiness);
  api.createSession.mockResolvedValue(session(projectA.projectId));
  api.getSession.mockResolvedValue(session(projectA.projectId));
  let windowSession: Promise<ImportSession> | null = null;
  api.getSessionOverview.mockImplementation(async (request: { sessionId: string }) => {
    windowSession = Promise.resolve(api.getSession(request));
    return overviewFor(await windowSession);
  });
  api.listSessionItems.mockImplementation(async (request: { sessionId: string }) => {
    const value = await (windowSession ?? Promise.resolve(api.getSession(request)));
    return {
      sessionId: value.sessionId,
      snapshotRevision: 1,
      items: value.items,
      nextCursor: null,
      total: value.items.length,
    };
  });
  api.startSessionRecovery.mockResolvedValue({
    ...task("session-recovery"),
    operation: { kind: "import_recovery", sessionId: "session-recover" },
  });
  api.getScanResult.mockResolvedValue({
    files: [],
    skipped: [],
    truncated: false,
    totals: { fileCount: 0, totalBytes: 0, estimatedOutputFiles: 0, requiresConfirmation: false, reasons: [] },
  });
  api.getPreviewContent.mockResolvedValue({ sessionId: `session-${projectA.projectId}`, itemId: "file.md", candidateId: null, title: "file.md", markdown: "# Preview", truncated: false, totalBytes: 9, sha256: "hash" });
  api.addPaths.mockResolvedValue(task("add-paths"));
  api.addText.mockResolvedValue(session(projectA.projectId, [item("clipboard-1")]));
  api.addUrl.mockResolvedValue(session(projectA.projectId, [item("url-1")]));
  api.discoverCollection.mockResolvedValue(null);
  api.addCollectionItems.mockResolvedValue(session(projectA.projectId));
  api.getRemoteMediaRetentionPlan.mockResolvedValue({
    itemId: "item-1",
    estimatedBytes: 0,
    availableDiskBytes: null,
    enoughDisk: null,
    quality: "best_available",
  });
  api.confirmRemoteMediaRetention.mockResolvedValue(session(projectA.projectId));
  api.getRestrictedContentStatus.mockResolvedValue({ confirmationRequired: false });
  api.setSelection.mockResolvedValue(session(projectA.projectId, [item("file.md", "preview_ready")]));
  api.startItems.mockResolvedValue([]);
  api.startBatch.mockResolvedValue({
    ...task("batch-operation"),
    batchId: "batch-operation",
    operation: {
      kind: "import_batch",
      sessionId: `session-${projectA.projectId}`,
      itemCount: 1,
    },
  });
  api.cancelItem.mockResolvedValue(session(projectA.projectId));
  api.acceptScan.mockResolvedValue({
    session: session(projectA.projectId),
    scan: {
      files: [],
      skipped: [],
      truncated: false,
      totals: { fileCount: 0, totalBytes: 0, estimatedOutputFiles: 0, requiresConfirmation: false, reasons: [] },
      acceptedAt: "2026-08-06T00:00:00Z",
    },
  });
  api.discardScan.mockResolvedValue({
    files: [],
    skipped: [],
    truncated: false,
    totals: { fileCount: 0, totalBytes: 0, estimatedOutputFiles: 0, requiresConfirmation: false, reasons: [] },
  });
  api.cancelBatch.mockResolvedValue([]);
  api.skipItem.mockResolvedValue(session(projectA.projectId));
  api.confirmSession.mockResolvedValue(task("confirm"));
  api.startAgentAssistance.mockResolvedValue(task("agent-task", projectA.projectId, "queued"));
  api.acceptAgentCandidate.mockResolvedValue({});
  api.selectAgentCandidate.mockImplementation(async ({ itemId }: { itemId: string }) => ({
    projectId: projectA.projectId,
    sessionId: `session-${projectA.projectId}`,
    itemId,
    candidateId: "candidate-1",
    item: item(itemId, "preview_ready"),
    completion: null,
  }));
  api.discardAgentCandidate.mockImplementation(async ({ itemId }: { itemId: string }) => ({
    projectId: projectA.projectId,
    sessionId: `session-${projectA.projectId}`,
    itemId,
    candidateId: "candidate-1",
    item: item(itemId, "failed"),
    completion: null,
  }));
  api.beginLogin.mockResolvedValue({ sessionId: "connector-1", platform: "wechat", state: "waiting_login" });
  api.completeLogin.mockResolvedValue({
    connectorSession: { sessionId: "connector-1", platform: "wechat", state: "authenticated", accountSummary: "Aletta" },
    resumedItemIds: ["item-1"],
    tasks: [task("resumed-task", projectA.projectId, "queued")],
  });
  api.revokeLogin.mockResolvedValue(undefined);
  api.authorizePrivateTarget.mockResolvedValue("grant-opaque");
  api.authorizeLocalAsr.mockResolvedValue(undefined);
  api.authorizeLocalOcr.mockResolvedValue(undefined);
  api.selectSubtitle.mockResolvedValue(session(projectA.projectId));
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
  const migrationReport: MigrationReport = {
    reportVersion: 1,
    planVersion: migrationPlan.planVersion,
    planFingerprint: migrationConfirmation.planFingerprint,
    inventoryFingerprint: inventory.fingerprint,
    status: "dry_run_ready",
    summary: migrationPlan.summary,
    automaticLinks: [],
    proposedRecords: [],
    conflicts: [],
    legacyUnmanaged: [],
    warnings: [],
    affectedMetadataPaths: [".app/source-index-v2.json"],
    untouchedContentPaths: ["raw/", "wiki/"],
    rollbackStatement: "Restore the Git checkpoint.",
    requiredConfirmation: true,
  };
  const history: ImportHistoryPage = { entries: [], legacyReadOnly: [], nextCursor: null, warnings: [] };
  api.scanMigration.mockResolvedValue(inventory);
  api.planMigration.mockResolvedValue({ plan: migrationPlan, report: migrationReport, confirmation: migrationConfirmation });
  api.applyMigration.mockResolvedValue(task("migration-task", projectA.projectId, "queued"));
  api.getMigrationStatus.mockResolvedValue({ status: "dry_run_ready", planFingerprint: migrationConfirmation.planFingerprint, report: null });
  api.resumeMigration.mockResolvedValue(task("migration-resume-task", projectA.projectId, "queued"));
  api.listHistory.mockResolvedValue(history);
  api.getCompletion.mockResolvedValue(completion);
});

describe("useImportWorkflow", () => {
  it("does not re-render an inactive Workflows route for unrelated task updates", async () => {
    const taskLauncher = launcher();
    let renders = 0;
    function InactiveImportController() {
      renders += 1;
      useImportWorkflow(projectA, "workflows", taskLauncher);
      return null;
    }
    render(<InactiveImportController />);
    await act(async () => { await Promise.resolve(); });
    const initialRenders = renders;

    act(() => {
      for (let index = 0; index < 100; index += 1) {
        useTaskStore.setState({ tasks: [task(`unrelated-${index}`)] });
      }
    });

    expect(renders - initialRenders).toBe(0);
  });

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

  it("creates a new session before adding after the previous session completed", async () => {
    const ended = {
      ...session(projectA.projectId, [item("done.md", "completed")]),
      status: "completed" as const,
    };
    const fresh = {
      ...session(projectA.projectId),
      sessionId: "session-fresh",
    };
    api.createSession
      .mockResolvedValueOnce(ended)
      .mockResolvedValueOnce(fresh);
    api.addText.mockResolvedValue({
      ...fresh,
      items: [item("clipboard-new")],
    });
    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));

    await waitFor(() => expect(result.current.session?.status).toBe("completed"));
    await act(async () => result.current.addText("# New", "new.md"));

    expect(api.createSession).toHaveBeenCalledTimes(2);
    expect(api.addText).toHaveBeenCalledWith(expect.objectContaining({
      sessionId: "session-fresh",
      sourceName: "new.md",
    }));
    expect(result.current.session?.sessionId).toBe("session-fresh");
  });

  it("shares one session renewal across concurrent additions without dropping either intent", async () => {
    const ended = {
      ...session(projectA.projectId, [item("done.md", "completed")]),
      status: "completed" as const,
    };
    const fresh = {
      ...session(projectA.projectId),
      sessionId: "session-fresh",
    };
    let resolveFresh!: (value: ImportSession) => void;
    const freshSession = new Promise<ImportSession>((resolve) => {
      resolveFresh = resolve;
    });
    api.createSession
      .mockResolvedValueOnce(ended)
      .mockReturnValueOnce(freshSession);
    api.addText.mockResolvedValue({
      ...fresh,
      items: [item("clipboard-new")],
    });
    api.addUrl.mockResolvedValue({
      ...fresh,
      items: [item("clipboard-new"), item("url-new")],
    });
    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));

    await waitFor(() => expect(result.current.session?.status).toBe("completed"));
    await act(async () => {
      const textAddition = result.current.addText("# New", "new.md");
      const urlAddition = result.current.addUrl("https://example.com/new");
      resolveFresh(fresh);
      await Promise.all([textAddition, urlAddition]);
    });

    expect(api.createSession).toHaveBeenCalledTimes(2);
    expect(api.addText).toHaveBeenCalledWith(expect.objectContaining({
      sessionId: "session-fresh",
      sourceName: "new.md",
    }));
    expect(api.addUrl).toHaveBeenCalledWith(expect.objectContaining({
      sessionId: "session-fresh",
      url: "https://example.com/new",
    }));
    expect(result.current.session?.items.map((entry) => entry.itemId)).toEqual([
      "clipboard-new",
      "url-new",
    ]);
  });

  it("does not drop a second path addition while the first discovery task is still active", async () => {
    const firstDiscovered = item("first-discovered.md");
    const secondDiscovered = item("second-discovered.md");
    api.addPaths
      .mockResolvedValueOnce(task("scan-first"))
      .mockResolvedValueOnce(task("scan-second"));
    api.getSession
      .mockResolvedValueOnce(session(projectA.projectId, [firstDiscovered]))
      .mockResolvedValueOnce(session(projectA.projectId, [firstDiscovered, secondDiscovered]));
    api.startBatch
      .mockResolvedValueOnce(task("first-discovered-task"))
      .mockResolvedValueOnce(task("second-discovered-task"));
    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));

    await waitFor(() => expect(result.current.bootstrapState).toBe("ready"));
    let additions!: Promise<void[]>;
    act(() => {
      additions = Promise.all([
        result.current.addPaths(["D:\\sources\\first"]),
        result.current.addPaths(["D:\\sources\\second"]),
      ]);
    });
    await waitFor(() => expect(api.addPaths).toHaveBeenCalledTimes(1));
    const firstCompleted = task("scan-first", projectA.projectId, "succeeded");
    useTaskStore.getState().upsertTask(firstCompleted);
    await act(async () => notifyTaskEventListeners({
      eventId: "scan-first-completed",
      eventType: "task_completed",
      projectId: projectA.projectId,
      taskId: firstCompleted.id,
      timestamp: firstCompleted.updatedAt,
      payload: firstCompleted,
    }));
    await waitFor(() => expect(api.addPaths).toHaveBeenCalledTimes(2));
    expect(api.startBatch).toHaveBeenNthCalledWith(1, expect.objectContaining({
      itemIds: ["first-discovered.md"],
    }));
    const secondCompleted = task("scan-second", projectA.projectId, "succeeded");
    useTaskStore.getState().upsertTask(secondCompleted);
    await act(async () => notifyTaskEventListeners({
      eventId: "scan-second-completed",
      eventType: "task_completed",
      projectId: projectA.projectId,
      taskId: secondCompleted.id,
      timestamp: secondCompleted.updatedAt,
      payload: secondCompleted,
    }));
    await act(async () => additions);
    await waitFor(() => expect(api.startBatch).toHaveBeenCalledTimes(2));
    expect(api.startBatch).toHaveBeenNthCalledWith(2, expect.objectContaining({
      itemIds: ["second-discovered.md"],
    }));

    expect(api.addPaths).toHaveBeenCalledTimes(2);
    expect(api.addPaths).toHaveBeenNthCalledWith(1, expect.objectContaining({
      sessionId: "session-project-a",
      sourcePaths: ["D:\\sources\\first"],
    }));
    expect(api.addPaths).toHaveBeenNthCalledWith(2, expect.objectContaining({
      sessionId: "session-project-a",
      sourcePaths: ["D:\\sources\\second"],
    }));
  });

  it("loads an existing session as a bounded first page and appends the next cursor page", async () => {
    const first = item("first.md");
    const second = item("second.md");
    const existing = session(projectA.projectId, [first, second]);
    api.getReadiness.mockResolvedValue({ ...readiness, unfinishedSessionId: existing.sessionId });
    api.getSessionOverview.mockResolvedValue(overviewFor(existing));
    api.listSessionItems
      .mockResolvedValueOnce({
        sessionId: existing.sessionId,
        snapshotRevision: 1,
        items: [first],
        nextCursor: "cursor-1",
        total: 2,
      })
      .mockResolvedValueOnce({
        sessionId: existing.sessionId,
        snapshotRevision: 1,
        items: [second],
        nextCursor: null,
        total: 2,
      });

    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));
    await waitFor(() => expect(result.current.session?.items.map((entry) => entry.itemId)).toEqual(["first.md"]));
    expect(result.current.hasMoreItems).toBe(true);

    await act(async () => result.current.loadMoreItems?.());

    expect(result.current.session?.items.map((entry) => entry.itemId)).toEqual(["first.md", "second.md"]);
    expect(result.current.hasMoreItems).toBe(false);
    expect(api.listSessionItems).toHaveBeenLastCalledWith({
      projectId: projectA.projectId,
      projectRootPath: projectA.rootPath,
      sessionId: existing.sessionId,
      filter: "all",
      cursor: "cursor-1",
      limit: 200,
    });
    expect(api.startSessionRecovery).not.toHaveBeenCalled();
  });

  it("rejects an async session mutation after the project authority revision changes", async () => {
    useProjectStore.setState({ authority: authorityA });
    let resolveAddition!: (value: ImportSession) => void;
    api.addText.mockReturnValue(new Promise<ImportSession>((resolve) => {
      resolveAddition = resolve;
    }));
    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));
    await waitFor(() => expect(result.current.bootstrapState).toBe("ready"));

    let addition!: Promise<void>;
    act(() => {
      addition = result.current.addText("# Stale", "stale.md");
    });
    await waitFor(() => expect(api.addText).toHaveBeenCalledTimes(1));
    act(() => {
      useProjectStore.setState({
        authority: { ...authorityA, authorityRevision: "authority-revision-b" },
      });
    });
    await act(async () => {
      resolveAddition(session(projectA.projectId, [item("stale.md")]));
      await addition;
    });

    expect(useImportStore.getState().session?.items.some((value) => value.itemId === "stale.md")).toBe(false);
  });

  it("keeps the session and batch context when the user visits another workspace view", async () => {
    const queued = item("queued.md");
    const started = task("queued-task");
    api.createSession.mockResolvedValue(session(projectA.projectId, [queued]));
    api.startBatch.mockResolvedValue(started);
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

  it("binds streamed ASR task progress to the live import item", async () => {
    const queued = item("video.mp4");
    const started = task("asr-task");
    api.createSession.mockResolvedValue(session(projectA.projectId, [queued]));
    api.startBatch.mockResolvedValue(started);
    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));

    await waitFor(() => expect(result.current.bootstrapState).toBe("ready"));
    await act(async () => result.current.startItems(["video.mp4"]));
    expect(result.current.session?.items[0].taskId).toBe("asr-task");

    const recognizing = {
      ...started,
      status: "running" as const,
      progress: { current: 48, total: 100, label: "asr.recognizing" },
      updatedAt: "2026-07-13T00:00:01Z",
    };
    useTaskStore.getState().upsertTask(recognizing);
    await act(async () => notifyTaskEventListeners({
      eventId: "asr-progress",
      eventType: "task_updated",
      projectId: projectA.projectId,
      taskId: recognizing.id,
      timestamp: recognizing.updatedAt,
      payload: recognizing,
    }));

    expect(result.current.session?.items[0]).toMatchObject({
      status: "extracting",
      taskId: "asr-task",
      progress: { current: 48, total: 100, label: "asr.recognizing" },
    });

    const stale = {
      ...recognizing,
      progress: { current: 30, total: 100, label: "asr.recognizing" },
      updatedAt: started.updatedAt,
    };
    await act(async () => notifyTaskEventListeners({
      eventId: "stale-asr-progress",
      eventType: "task_updated",
      projectId: projectA.projectId,
      taskId: stale.id,
      timestamp: stale.updatedAt,
      payload: stale,
    }));
    expect(result.current.session?.items[0].progress).toEqual(recognizing.progress);
  });

  it("recovers the unfinished session without creating a competing draft", async () => {
    api.getReadiness.mockResolvedValue({ ...readiness, unfinishedSessionId: "session-recover" });
    api.getSession.mockResolvedValue({ ...session(projectA.projectId, [item("recover.md")]), sessionId: "session-recover" });
    api.getSessionOverview.mockResolvedValue({
      ...overviewFor({ ...session(projectA.projectId, [item("recover.md")]), sessionId: "session-recover" }),
      indexState: "rebuild_required" as const,
    });
    let recoveryFactPublications = 0;
    const unsubscribe = useTaskStore.subscribe((state, previous) => {
      if (state.taskById["session-recovery"] !== previous.taskById["session-recovery"]) {
        recoveryFactPublications += 1;
      }
    });

    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));

    await waitFor(() => expect(result.current.session?.sessionId).toBe("session-recover"));
    expect(api.createSession).not.toHaveBeenCalled();
    expect(api.getSession).toHaveBeenCalledWith(expect.objectContaining({
      projectId: projectA.projectId,
      projectRootPath: projectA.rootPath,
      sessionId: "session-recover",
    }));
    expect(api.startSessionRecovery).toHaveBeenCalledWith({
      projectId: projectA.projectId,
      projectRootPath: projectA.rootPath,
      sessionId: "session-recover",
    });
    await waitFor(() => expect(
      useTaskStore.getState().tasks.find((entry) => entry.id === "session-recovery"),
    ).toMatchObject({ operation: { kind: "import_recovery", sessionId: "session-recover" } }));
    unsubscribe();
    expect(recoveryFactPublications).toBe(1);
  });

  it("refreshes the unfinished session when its recovery task reaches terminal state", async () => {
    api.getReadiness.mockResolvedValue({ ...readiness, unfinishedSessionId: "session-recover" });
    const initial = { ...session(projectA.projectId, [item("recover.md")]), sessionId: "session-recover" };
    const recoveredItem = item("recover.md", "paused");
    api.getSession
      .mockResolvedValueOnce(initial)
      .mockResolvedValueOnce({ ...initial, items: [recoveredItem] });
    api.getSessionOverview.mockResolvedValue({
      ...overviewFor(initial),
      indexState: "rebuild_required" as const,
    });

    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));
    await waitFor(() => expect(result.current.session?.sessionId).toBe("session-recover"));

    const recoveryTask: BackendTask = {
      ...task("session-recovery", projectA.projectId, "succeeded"),
      operation: { kind: "import_recovery", sessionId: "session-recover" },
    };
    await act(async () => notifyTaskEventListeners({
      eventId: "session-recovery-completed",
      eventType: "task_completed",
      projectId: projectA.projectId,
      taskId: recoveryTask.id,
      timestamp: recoveryTask.updatedAt,
      payload: recoveryTask,
    }));

    await waitFor(() => expect(result.current.session?.items[0]?.status).toBe("paused"));
    expect(api.getSession).toHaveBeenCalledTimes(2);
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

  it("keeps a late project A task globally without attaching it to project B import UI", async () => {
    const queued = item("queued.md");
    api.createSession.mockImplementation(({ projectId }: { projectId: string }) =>
      Promise.resolve(session(projectId, projectId === projectA.projectId ? [queued] : [])));
    let resolveStart!: (value: BackendTask) => void;
    api.startBatch.mockReturnValue(new Promise<BackendTask>((resolve) => { resolveStart = resolve; }));
    const taskLauncher = launcher();
    const { result, rerender } = renderHook(
      ({ project }) => useImportWorkflow(project, "import", taskLauncher),
      { initialProps: { project: projectA } },
    );
    await waitFor(() => expect(result.current.session?.projectId).toBe(projectA.projectId));

    act(() => { void result.current.startItems(["queued.md"]); });
    expect(result.current.pendingItemIds?.has("queued.md")).toBe(true);
    rerender({ project: projectB });
    await waitFor(() => expect(result.current.session?.projectId).toBe(projectB.projectId));

    await act(async () => resolveStart(task("late-project-a-task")));
    expect(useTaskStore.getState().tasks).toContainEqual(task("late-project-a-task"));
    expect(result.current.pendingItemIds?.size).toBe(0);
    expect(result.current.batch).toBeNull();
  });

  it("adds a URL, starts only newly queued items, and upserts returned task facts", async () => {
    const existing = item("existing.md", "preview_ready");
    api.createSession.mockResolvedValue(session(projectA.projectId, [existing]));
    api.addUrl.mockResolvedValue(session(projectA.projectId, [existing, item("url-1")]));
    const started = {
      ...task("url-task"),
      batchId: "url-task",
      title: "Import example.com",
      operation: {
        kind: "import_batch" as const,
        sessionId: "session-project-a",
        itemCount: 1,
        sourceLabel: "example.com",
      },
    };
    api.startBatch.mockResolvedValue(started);
    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));
    await waitFor(() => expect(result.current.session?.items).toHaveLength(1));

    await act(async () => result.current.addUrl("https://example.com/article"));
    expect(api.addUrl).toHaveBeenCalledWith({
      projectId: projectA.projectId,
      projectRootPath: projectA.rootPath,
      sessionId: "session-project-a",
      url: "https://example.com/article",
    });
    expect(api.startBatch).toHaveBeenCalledWith({
      projectId: projectA.projectId,
      projectRootPath: projectA.rootPath,
      sessionId: "session-project-a",
      itemIds: ["url-1"],
    });
    expect(useTaskStore.getState().tasks).toContainEqual(started);
  });

  it("rejects URL addition when operation startup fails so the form can retain the URL", async () => {
    api.addUrl.mockResolvedValue(session(projectA.projectId, [item("url-retry")]));
    api.startBatch.mockRejectedValue(new Error("operation task unavailable"));
    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));
    await waitFor(() => expect(result.current.bootstrapState).toBe("ready"));

    await act(async () => {
      await expect(result.current.addUrl("https://example.com/retry")).rejects.toThrow(
        "operation task unavailable",
      );
    });

    expect(result.current.pendingItemIds?.has("url-retry")).toBe(false);
    expect(useToastStore.getState().toasts).toHaveLength(1);
  });

  it("adds reviewed clipboard text and starts only the new queued item", async () => {
    const existing = item("existing.md", "preview_ready");
    api.createSession.mockResolvedValue(session(projectA.projectId, [existing]));
    api.addText.mockResolvedValue(session(projectA.projectId, [existing, item("clipboard-1")]));
    api.startBatch.mockResolvedValue(task("clipboard-task"));
    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));
    await waitFor(() => expect(result.current.session?.items).toHaveLength(1));

    await act(async () => result.current.addText("# Pasted\n\nText", "pasted.md"));

    expect(api.addText).toHaveBeenCalledWith({
      projectId: projectA.projectId,
      projectRootPath: projectA.rootPath,
      sessionId: "session-project-a",
      sourceName: "pasted.md",
      content: "# Pasted\n\nText",
    });
    expect(api.startBatch).toHaveBeenCalledWith({
      projectId: projectA.projectId,
      projectRootPath: projectA.rootPath,
      sessionId: "session-project-a",
      itemIds: ["clipboard-1"],
    });
  });

  it("reconciles a successful URL addition when another session mutation finishes first", async () => {
    const existing = item("existing.md", "preview_ready");
    api.createSession.mockResolvedValue(session(projectA.projectId, [existing]));
    let resolveAddUrl!: (value: ImportSession) => void;
    api.addUrl.mockReturnValue(new Promise<ImportSession>((resolve) => { resolveAddUrl = resolve; }));
    const selectionSession = session(projectA.projectId, [{ ...existing, selected: false }]);
    const latestSession = session(projectA.projectId, [{ ...existing, selected: false }, item("url-late")]);
    api.setSelection.mockResolvedValue(selectionSession);
    api.getSession.mockResolvedValue(latestSession);
    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));
    await waitFor(() => expect(result.current.session?.items).toHaveLength(1));

    act(() => { void result.current.addUrl("https://example.com/late"); });
    await act(async () => result.current.setItemSelected("existing.md", false));
    await act(async () => resolveAddUrl(session(projectA.projectId, [existing, item("url-late")])));

    await waitFor(() => expect(result.current.session?.items.map((entry) => entry.itemId)).toEqual([
      "existing.md",
      "url-late",
    ]));
    expect(api.startBatch).toHaveBeenCalledWith({
      projectId: projectA.projectId,
      projectRootPath: projectA.rootPath,
      sessionId: `session-${projectA.projectId}`,
      itemIds: ["url-late"],
    });
  });

  it("keeps the source entry busy through discovery and surfaces task progress", async () => {
    const scanTask = task("scan-task");
    api.addPaths.mockResolvedValue(scanTask);
    const taskLauncher = launcher();
    const { result } = renderHook(() => useImportWorkflow(projectA, "import", taskLauncher));
    await waitFor(() => expect(result.current.bootstrapState).toBe("ready"));

    let addition!: Promise<void>;
    act(() => {
      addition = result.current.addPaths(["D:\\知识库\\资料"]);
    });
    await waitFor(() => expect(api.addPaths).toHaveBeenCalledTimes(1));
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
    await act(async () => addition);
    await waitFor(() => expect(result.current.isAddingPaths).toBe(false));
    expect(result.current.discoveryTask?.result?.summary).toContain("Added 12 files");
    await waitFor(() => expect(result.current.discoveryScan?.skipped[0]?.reason).toBe("project_internal"));
  });

  it("accepts the saved aggregate scan without rescanning source paths", async () => {
    const scanTask = task("aggregate-scan");
    const queued = item("accepted.md", "queued");
    api.addPaths.mockResolvedValue(scanTask);
    const aggregateScan = {
      files: [],
      skipped: [],
      truncated: false,
      totals: {
        fileCount: 1_200,
        totalBytes: 2_500_000_000,
        estimatedOutputFiles: 2_400,
        requiresConfirmation: true,
        reasons: ["file_count", "total_bytes", "estimated_output_files"],
      },
      confirmationToken: "aggregate-token",
    };
    api.getScanResult.mockResolvedValue(aggregateScan);
    api.getSession.mockResolvedValue(session(projectA.projectId));
    api.acceptScan.mockResolvedValue({
      session: session(projectA.projectId, [queued]),
      scan: { ...aggregateScan, acceptedAt: "2026-08-06T00:00:01Z" },
    });
    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));
    await waitFor(() => expect(result.current.bootstrapState).toBe("ready"));

    const addition = result.current.addPaths(["D:\\sources\\large-folder"]);
    await waitFor(() => expect(api.addPaths).toHaveBeenCalledTimes(1));
    await act(async () => notifyTaskEventListeners({
      eventId: "event-aggregate-scan-complete",
      eventType: "task_completed",
      projectId: projectA.projectId,
      taskId: scanTask.id,
      timestamp: "2026-08-06T00:00:00Z",
      payload: { ...scanTask, status: "succeeded" },
    }));
    await act(async () => addition);
    await waitFor(() => expect(result.current.discoveryScan?.confirmationToken).toBe("aggregate-token"));

    await act(async () => result.current.confirmDiscovery?.());
    expect(api.acceptScan).toHaveBeenCalledWith({
      projectId: projectA.projectId,
      projectRootPath: projectA.rootPath,
      sessionId: "session-project-a",
      taskId: scanTask.id,
      confirmationToken: "aggregate-token",
      acknowledgeAggregate: true,
    });
    expect(api.addPaths).toHaveBeenCalledTimes(1);
    expect(api.startBatch).toHaveBeenCalledWith(expect.objectContaining({ itemIds: [queued.itemId] }));
  });

  it("keeps large spreadsheets pending after aggregate acceptance and acknowledges them separately", async () => {
    const scanTask = task("aggregate-with-large-scan");
    const safe = item("safe.md", "queued");
    const risky = item("large.csv", "queued");
    const aggregateScan = {
      files: [{
        sourcePath: "D:/sources/large.csv",
        relativePath: "large.csv",
        displayName: "large.csv",
        format: "csv" as const,
        contentKind: "document" as const,
        sizeBytes: 12_000_000,
        identity: {
          extension: "csv",
          magic: "delimited-text",
          mime: "text/csv",
          detectionMethod: "structured_text" as const,
          extensionMismatch: false,
        },
        sourceIdentity: { sha256: "a".repeat(64), sizeBytes: 12_000_000, modifiedNs: 1 },
        largeData: {
          rowCount: 20_000,
          estimatedOutputFiles: 5,
          totalBytes: 12_000_000,
          requiresConfirmation: true,
          estimateComplete: true,
        },
      }],
      skipped: [],
      truncated: false,
      totals: {
        fileCount: 1_200,
        totalBytes: 2_500_000_000,
        estimatedOutputFiles: 2_400,
        requiresConfirmation: true,
        reasons: ["file_count" as const, "total_bytes" as const, "estimated_output_files" as const],
      },
      confirmationToken: "two-stage-token",
    };
    api.addPaths.mockResolvedValue(scanTask);
    api.getScanResult.mockResolvedValue(aggregateScan);
    api.getSession.mockResolvedValue(session(projectA.projectId));
    api.acceptScan
      .mockResolvedValueOnce({
        session: session(projectA.projectId, [safe]),
        scan: { ...aggregateScan, aggregateConfirmedAt: "2026-08-06T00:00:01Z" },
      })
      .mockResolvedValueOnce({
        session: session(projectA.projectId, [safe, risky]),
        scan: {
          ...aggregateScan,
          aggregateConfirmedAt: "2026-08-06T00:00:01Z",
          acceptedAt: "2026-08-06T00:00:02Z",
        },
      });
    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));
    await waitFor(() => expect(result.current.bootstrapState).toBe("ready"));

    const addition = result.current.addPaths(["D:\\sources\\large-folder"]);
    await waitFor(() => expect(api.addPaths).toHaveBeenCalledTimes(1));
    await act(async () => notifyTaskEventListeners({
      eventId: "event-two-stage-scan-complete",
      eventType: "task_completed",
      projectId: projectA.projectId,
      taskId: scanTask.id,
      timestamp: "2026-08-06T00:00:00Z",
      payload: { ...scanTask, status: "succeeded" },
    }));
    await act(async () => addition);
    await waitFor(() => expect(result.current.discoveryScan?.confirmationToken).toBe("two-stage-token"));

    await act(async () => result.current.confirmDiscovery?.());
    expect(api.acceptScan).toHaveBeenNthCalledWith(1, expect.objectContaining({
      acknowledgeAggregate: true,
    }));
    expect(result.current.discoveryScan?.aggregateConfirmedAt).toBe("2026-08-06T00:00:01Z");
    expect(result.current.discoveryScan?.acceptedAt).toBeUndefined();

    await act(async () => result.current.confirmDiscovery?.(["D:/sources/large.csv"]));
    expect(api.acceptScan).toHaveBeenNthCalledWith(2, expect.objectContaining({
      sourcePaths: ["D:/sources/large.csv"],
    }));
    expect(api.acceptScan.mock.calls[1][0]).not.toHaveProperty("acknowledgeAggregate");
    expect(result.current.discoveryScan).toBeNull();
    expect(api.addPaths).toHaveBeenCalledTimes(1);
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

    let addition!: Promise<void>;
    act(() => {
      addition = result.current.addPaths(["D:\\sources\\notes.md"]);
    });

    await waitFor(() => expect(listTasksCalls).toBeGreaterThanOrEqual(2), { timeout: 2_000 });
    await act(async () => addition);
    await waitFor(() => expect(result.current.session?.items.map((entry) => entry.input.displayName)).toEqual(["notes.md"]), { timeout: 2_000 });
    expect(api.startBatch).toHaveBeenCalledWith({
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
    api.startBatch.mockResolvedValueOnce(firstTask).mockResolvedValueOnce(secondTask);
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
    api.startBatch
      .mockResolvedValueOnce({ ...task("first-task"), batchId: "batch-a" })
      .mockResolvedValueOnce({ ...task("second-task"), batchId: "batch-b" });
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

  it("reattaches live progress events to a recovered import item", async () => {
    const recovered = item("recover.mp4", "extracting");
    recovered.taskId = "recover-task";
    api.getReadiness.mockResolvedValue({ ...readiness, unfinishedSessionId: "session-recover" });
    api.getSession.mockResolvedValue({ ...session(projectA.projectId, [recovered]), sessionId: "session-recover" });
    const running = {
      ...task("recover-task", projectA.projectId, "running"),
      batchId: "batch-recovered",
      progress: { current: 37, total: 100, label: "asr.recognizing" },
    };
    useTaskStore.getState().upsertTask(running);

    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));

    await waitFor(() => expect(result.current.session?.items[0].progress).toEqual(running.progress));

    const advanced = {
      ...running,
      progress: { current: 63, total: 100, label: "asr.recognizing" },
      updatedAt: "2026-07-13T00:00:01Z",
    };
    await act(async () => {
      notifyTaskEventListeners({
        eventId: "recovered-asr-progress",
        eventType: "task_updated",
        projectId: projectA.projectId,
        taskId: advanced.id,
        timestamp: advanced.updatedAt,
        payload: advanced,
      });
    });

    expect(useTaskStore.getState().taskById[advanced.id]?.progress).toEqual(advanced.progress);
    await waitFor(() => expect(result.current.session?.items[0].progress).toEqual(advanced.progress));
  });

  it("recovers a review-ready batch that is waiting for confirmation", async () => {
    const recovered = item("review.md", "preview_ready");
    recovered.taskId = "review-task";
    api.getReadiness.mockResolvedValue({ ...readiness, unfinishedSessionId: "session-review" });
    api.getSession.mockResolvedValue({ ...session(projectA.projectId, [recovered]), sessionId: "session-review" });
    useTaskStore.getState().upsertTask({
      ...task("review-task", projectA.projectId, "waiting_for_confirmation"),
      batchId: "batch-review",
    });

    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));

    await waitFor(() => expect(result.current.batches?.[0]).toMatchObject({
      id: "batch-review",
      waitingForConfirmation: 1,
      reviewReady: 1,
    }));
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

  it("settles and continues a persisted discovery task from its recovered task snapshot", async () => {
    const recoveredSession = { ...session(projectA.projectId), sessionId: "session-recover", discoveryTaskId: "scan-task" };
    const discoveredSession = { ...recoveredSession, items: [item("notes.md")] };
    api.getReadiness.mockResolvedValue({ ...readiness, unfinishedSessionId: "session-recover" });
    api.getSession.mockResolvedValueOnce(recoveredSession).mockResolvedValue(discoveredSession);
    useTaskStore.getState().upsertTask(task("scan-task", projectA.projectId, "succeeded"));

    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));

    await waitFor(() => expect(result.current.session?.items.map((entry) => entry.itemId)).toEqual(["notes.md"]));
    await waitFor(() => expect(api.startBatch).toHaveBeenCalledWith({
      projectId: projectA.projectId,
      projectRootPath: projectA.rootPath,
      sessionId: "session-recover",
      itemIds: ["notes.md"],
    }));
  });

  it("passes a selected recovery route through retry and persists skip as a session update", async () => {
    const failed = item("failed.pdf", "failed");
    api.createSession.mockResolvedValue(session(projectA.projectId, [failed]));
    api.startBatch.mockResolvedValue(task("retry-operation"));
    api.getSession.mockResolvedValue(session(projectA.projectId, [failed]));
    api.skipItem.mockResolvedValue(session(projectA.projectId, [item("failed.pdf", "skipped")]));
    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));
    await waitFor(() => expect(result.current.session?.items).toHaveLength(1));

    await act(async () => result.current.retryItem("failed.pdf", "enable_ocr"));
    expect(api.startBatch).toHaveBeenCalledWith(expect.objectContaining({
      itemIds: ["failed.pdf"],
      recoveryAction: "enable_ocr",
    }));
    await act(async () => notifyTaskEventListeners({
      eventId: "event-retry-failed",
      eventType: "task_failed",
      projectId: projectA.projectId,
      taskId: "retry-operation",
      timestamp: "2026-07-14T00:00:00Z",
      payload: { ...task("retry-operation"), status: "failed" },
    }));
    await waitFor(() => expect(result.current.pendingItemIds?.has("failed.pdf")).toBe(false));

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
    let resolveStart!: (value: BackendTask) => void;
    api.startBatch.mockReturnValue(new Promise<BackendTask>((resolve) => { resolveStart = resolve; }));
    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));
    await waitFor(() => expect(result.current.session?.items).toHaveLength(1));

    act(() => { void result.current.startItems(["queued.md"]); });
    await waitFor(() => expect(result.current.pendingItemIds?.has("queued.md")).toBe(true));
    act(() => { void result.current.startItems(["queued.md"]); });
    expect(api.startBatch).toHaveBeenCalledTimes(1);

    const started = task("queued-task");
    await act(async () => resolveStart(started));
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

  it("applies one operation patch, releases the cohort, and refreshes once at terminal", async () => {
    const queued = item("batch.md", "queued");
    const operation = {
      ...task("operation-task"),
      batchId: "import-v2-operation:session-project-a",
    };
    const patched = { ...queued, status: "preview_ready" as const, taskId: operation.id };
    api.createSession.mockResolvedValue(session(projectA.projectId, [queued]));
    api.startBatch.mockResolvedValue(operation);
    api.getSession.mockResolvedValue(session(projectA.projectId, [patched]));
    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));
    await waitFor(() => expect(result.current.bootstrapState).toBe("ready"));

    await act(async () => result.current.startItems([queued.itemId]));
    expect(result.current.pendingItemIds?.has(queued.itemId)).toBe(true);
    await act(async () => notifyTaskEventListeners({
      eventId: "event-operation-terminal-patch",
      eventType: "import_session_patch",
      projectId: projectA.projectId,
      taskId: operation.id,
      timestamp: "2026-08-06T00:00:00Z",
      payload: {
        projectId: projectA.projectId,
        projectRootPath: projectA.rootPath,
        sessionId: "session-project-a",
        batchId: operation.id,
        items: [patched],
        counts: { total: 1, processed: 1, succeeded: 1, waiting: 0, failed: 0, cancelled: 0 },
      },
    }));

    await waitFor(() => expect(result.current.session?.items[0].status).toBe("preview_ready"));
    expect(result.current.pendingItemIds?.has(queued.itemId)).toBe(false);
    expect(result.current.batch).toMatchObject({
      id: operation.id,
      total: 1,
      active: 0,
      reviewReady: 1,
      failed: 0,
    });
    await waitFor(() => expect(api.getSession).toHaveBeenCalledTimes(1));

    await act(async () => notifyTaskEventListeners({
      eventId: "event-operation-task-terminal",
      eventType: "task_completed",
      projectId: projectA.projectId,
      taskId: operation.id,
      timestamp: "2026-08-06T00:00:01Z",
      payload: { ...operation, status: "succeeded" },
    }));
    expect(api.getSession).toHaveBeenCalledTimes(1);
  });

  it("buffers a matching pre-response patch but ignores an unbound stale patch", async () => {
    const queued = item("early.md", "queued");
    const operation = {
      ...task("early-operation"),
      batchId: "import-v2-operation:session-project-a",
    };
    const patched = { ...queued, status: "failed" as const, taskId: operation.id };
    api.createSession.mockResolvedValue(session(projectA.projectId, [queued]));
    api.getSession.mockResolvedValue(session(projectA.projectId, [patched]));
    let resolveStart!: (value: BackendTask) => void;
    api.startBatch.mockReturnValue(new Promise<BackendTask>((resolve) => { resolveStart = resolve; }));
    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));
    await waitFor(() => expect(result.current.bootstrapState).toBe("ready"));

    await act(async () => notifyTaskEventListeners({
      eventId: "event-stale-unbound-patch",
      eventType: "import_session_patch",
      projectId: projectA.projectId,
      taskId: "old-operation",
      timestamp: "2026-08-06T00:00:00Z",
      payload: {
        projectId: projectA.projectId,
        projectRootPath: projectA.rootPath,
        sessionId: "session-project-a",
        batchId: "old-operation",
        items: [{ ...queued, status: "cancelled" as const }],
        counts: { total: 1, processed: 1, succeeded: 0, waiting: 0, failed: 0, cancelled: 1 },
      },
    }));
    expect(result.current.session?.items[0].status).toBe("queued");

    act(() => { void result.current.startItems([queued.itemId]); });
    await waitFor(() => expect(api.startBatch).toHaveBeenCalledTimes(1));
    await act(async () => notifyTaskEventListeners({
      eventId: "event-early-bound-patch",
      eventType: "import_session_patch",
      projectId: projectA.projectId,
      taskId: operation.id,
      timestamp: "2026-08-06T00:00:01Z",
      payload: {
        projectId: projectA.projectId,
        projectRootPath: projectA.rootPath,
        sessionId: "session-project-a",
        batchId: operation.id,
        items: [patched],
        counts: { total: 1, processed: 1, succeeded: 0, waiting: 0, failed: 1, cancelled: 0 },
      },
    }));
    expect(result.current.session?.items[0].status).toBe("queued");

    await act(async () => resolveStart(operation));
    await waitFor(() => expect(result.current.session?.items[0].status).toBe("failed"));
    expect(result.current.pendingItemIds?.has(queued.itemId)).toBe(false);
  });

  it("rejects operation patches for a stale project, root, session, or epoch", async () => {
    const queued = item("guarded.md", "queued");
    const operation = {
      ...task("guarded-operation"),
      batchId: "import-v2-operation:session-project-a",
    };
    const patched = { ...queued, status: "failed" as const, taskId: operation.id };
    api.createSession.mockResolvedValue(session(projectA.projectId, [queued]));
    api.startBatch.mockResolvedValue(operation);
    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));
    await waitFor(() => expect(result.current.bootstrapState).toBe("ready"));
    await act(async () => result.current.startItems([queued.itemId]));

    const notifyPatch = async (
      eventProjectId: string,
      payloadProjectId: string,
      projectRootPath: string,
      sessionId: string,
      eventId: string,
    ) => notifyTaskEventListeners({
      eventId,
      eventType: "import_session_patch",
      projectId: eventProjectId,
      taskId: operation.id,
      timestamp: "2026-08-06T00:00:00Z",
      payload: {
        projectId: payloadProjectId,
        projectRootPath,
        sessionId,
        batchId: operation.id,
        items: [patched],
        counts: { total: 1, processed: 1, succeeded: 0, waiting: 0, failed: 1, cancelled: 0 },
      },
    });

    await act(async () => notifyPatch(projectB.projectId, projectA.projectId, projectA.rootPath, "session-project-a", "wrong-event-project"));
    await act(async () => notifyPatch(projectA.projectId, projectB.projectId, projectA.rootPath, "session-project-a", "wrong-payload-project"));
    await act(async () => notifyPatch(projectA.projectId, projectA.projectId, projectB.rootPath, "session-project-a", "wrong-root"));
    await act(async () => notifyPatch(projectA.projectId, projectA.projectId, projectA.rootPath, "session-project-b", "wrong-session"));
    expect(result.current.session?.items[0].status).toBe("queued");

    const projectKey = importProjectKey(projectA.projectId, projectA.rootPath);
    act(() => {
      const nextEpoch = useImportStore.getState().beginSessionEpoch(projectKey);
      useImportStore.getState().attachSession(projectKey, session(projectA.projectId, [queued]), nextEpoch);
    });
    await act(async () => notifyPatch(projectA.projectId, projectA.projectId, projectA.rootPath, "session-project-a", "stale-epoch"));
    expect(useImportStore.getState().session?.items[0].status).toBe("queued");
  });

  it("rejects an operation patch after the authority revision changes", async () => {
    useProjectStore.setState({ authority: authorityA });
    const queued = item("authority-guarded.md", "queued");
    const operation = {
      ...task("authority-operation"),
      batchId: "import-v2-operation:session-project-a",
    };
    api.createSession.mockResolvedValue(session(projectA.projectId, [queued]));
    api.startBatch.mockResolvedValue(operation);
    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));
    await waitFor(() => expect(result.current.bootstrapState).toBe("ready"));
    await act(async () => result.current.startItems([queued.itemId]));

    act(() => {
      useProjectStore.setState({
        authority: { ...authorityA, authorityRevision: "authority-revision-b" },
      });
    });
    await act(async () => notifyTaskEventListeners({
      eventId: "stale-authority-patch",
      eventType: "import_session_patch",
      projectId: projectA.projectId,
      taskId: operation.id,
      timestamp: "2026-08-06T00:00:00Z",
      payload: {
        projectId: projectA.projectId,
        projectRootPath: projectA.rootPath,
        sessionId: "session-project-a",
        batchId: operation.id,
        items: [{ ...queued, status: "failed", taskId: operation.id }],
        counts: { total: 1, processed: 1, succeeded: 0, waiting: 0, failed: 1, cancelled: 0 },
      },
    }));

    expect(result.current.session?.items.some((value) => value.status === "failed") ?? false).toBe(false);
  });

  it("releases the item lock when parsing pauses for user confirmation", async () => {
    const queued = item("review.md");
    api.createSession.mockResolvedValue(session(projectA.projectId, [queued]));
    let resolveStart!: (value: BackendTask) => void;
    api.startBatch.mockReturnValue(new Promise<BackendTask>((resolve) => { resolveStart = resolve; }));
    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));
    await waitFor(() => expect(result.current.session?.items).toHaveLength(1));

    act(() => { void result.current.startItems(["review.md"]); });
    const started = task("review-task");
    await act(async () => resolveStart(started));
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
    api.startBatch.mockResolvedValue(started);
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

  it("consumes an exact-duplicate completion after its preview task left the pending map", async () => {
    const duplicate = item("duplicate.md");
    api.createSession.mockResolvedValue(session(projectA.projectId, [duplicate]));
    const started = task("duplicate-task");
    api.startBatch.mockResolvedValue(started);
    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));
    await waitFor(() => expect(result.current.session?.items).toHaveLength(1));

    await act(async () => result.current.startItems(["duplicate.md"]));
    const waiting = { ...started, status: "waiting_for_confirmation" as const };
    useTaskStore.getState().upsertTask(waiting);
    await act(async () => notifyTaskEventListeners({
      eventId: "duplicate-waiting",
      eventType: "confirmation_requested",
      projectId: projectA.projectId,
      taskId: waiting.id,
      timestamp: waiting.updatedAt,
      payload: waiting,
    }));
    await waitFor(() => expect(result.current.pendingItemIds?.has("duplicate.md")).toBe(false));

    const succeeded: BackendTask = {
      ...started,
      status: "succeeded",
      result: {
        summary: "Duplicate already exists; its locator was recorded.",
        affectedPaths: [".app/import-history/batch-complete.json"],
        reference: {
          type: "import_v2_session_preview",
          sessionId: completion.sessionId,
          batchId: completion.batchId,
          completion,
        },
      },
    };
    useTaskStore.getState().upsertTask(succeeded);
    await act(async () => notifyTaskEventListeners({
      eventId: "duplicate-completed",
      eventType: "task_completed",
      projectId: projectA.projectId,
      taskId: succeeded.id,
      timestamp: succeeded.updatedAt,
      payload: succeeded,
    }));

    await waitFor(() => expect(result.current.completion).toEqual(completion));
  });

  it("rescans committed Sources for a partial batch without showing a completion summary", async () => {
    const scan = vi.fn().mockResolvedValue(undefined);
    const originalScan = useWikiStore.getState().scan;
    useWikiStore.setState({ scan });
    const taskLauncher = launcher();
    try {
      const { result } = renderHook(() => useImportWorkflow(projectA, "import", taskLauncher));
      await waitFor(() => expect(result.current.bootstrapState).toBe("ready"));
      const partial = {
        ...task("partial-confirm", projectA.projectId, "succeeded"),
        result: {
          summary: "Committed 1 import item; 1 remains.",
          affectedPaths: [".app/import-history/batch-partial.json"],
          reference: {
            type: "import_v2_session_preview" as const,
            sessionId: `session-${projectA.projectId}`,
            batchId: "batch-partial",
            completion: null,
          },
        },
      };
      useTaskStore.getState().upsertTask(partial);
      await act(async () => notifyTaskEventListeners({
        eventId: "partial-confirm-completed",
        eventType: "task_completed",
        projectId: projectA.projectId,
        taskId: partial.id,
        timestamp: partial.updatedAt,
        payload: partial,
      }));

      await waitFor(() => expect(scan).toHaveBeenCalledTimes(1));
      expect(result.current.completion).toBeNull();
    } finally {
      useWikiStore.setState({ scan: originalScan });
    }
  });

  it("keeps a child task settled when its waiting snapshot arrived before IPC returned", async () => {
    const queued = item("fast.md");
    api.createSession.mockResolvedValue(session(projectA.projectId, [queued]));
    const started = task("fast-item-task");
    api.startBatch.mockResolvedValue(started);
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

    const decisions = [{ itemId: "ready.md", resolution: null }];
    act(() => { void result.current.confirm(decisions); });
    expect(useImportStore.getState().isConfirming).toBe(true);
    act(() => { void result.current.confirm(decisions); });
    expect(api.confirmSession).toHaveBeenCalledTimes(1);
    await act(async () => resolveConfirm(task("confirm-task")));
  });

  it("persists an explicit companion subtitle choice before restarting only that item", async () => {
    const waiting = {
      ...item("访谈.mp4", "waiting_authorization"),
      issue: {
        code: "IMPORT_FILE_SUBTITLE_AMBIGUOUS",
        message: "Choose a subtitle.",
        stage: "extract" as const,
        retryable: true,
        userActionRequired: true,
        recoveryActions: ["select_subtitle" as const],
        availableActions: [],
        subtitleCandidates: ["访谈.en.srt", "访谈.zh-CN.srt"],
      },
    };
    const current = session(projectA.projectId, [waiting]);
    api.createSession.mockResolvedValue(current);
    api.getSession.mockResolvedValue(current);
    api.selectSubtitle.mockResolvedValue({
      ...current,
      items: [{ ...waiting, selectedSubtitle: "访谈.zh-CN.srt" }],
    });
    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));
    await waitFor(() => expect(result.current.session?.items).toHaveLength(1));

    await act(async () => result.current.selectSubtitle("访谈.mp4", "访谈.zh-CN.srt"));
    expect(api.selectSubtitle).toHaveBeenCalledWith({
      projectId: projectA.projectId,
      projectRootPath: projectA.rootPath,
      sessionId: `session-${projectA.projectId}`,
      itemId: "访谈.mp4",
      fileName: "访谈.zh-CN.srt",
    });
    expect(api.startBatch).toHaveBeenCalledWith(expect.objectContaining({
      itemIds: ["访谈.mp4"],
    }));
  });

  it("keeps a confirmation task tracked through conflicts and never auto-starts Compile", async () => {
    api.createSession.mockResolvedValue(session(projectA.projectId, [item("ready.md", "preview_ready")]));
    const confirmTask = task("confirm-with-conflict");
    api.confirmSession.mockResolvedValue(confirmTask);
    const taskLauncher = launcher();
    const { result } = renderHook(() => useImportWorkflow(projectA, "import", taskLauncher));
    await waitFor(() => expect(result.current.session?.items).toHaveLength(1));

    await act(async () => result.current.confirm([{ itemId: "ready.md", resolution: null }]));
    const waitingTask = {
      ...confirmTask,
      status: "waiting_for_confirmation" as const,
    };
    await act(async () => notifyTaskEventListeners({
      eventId: "confirm-conflict",
      eventType: "confirmation_requested",
      projectId: projectA.projectId,
      taskId: waitingTask.id,
      timestamp: "2026-07-14T00:00:00Z",
      payload: waitingTask,
    }));

    expect(result.current.isConfirming).toBe(true);
    expect(result.current.completion).toBeNull();

    const completedTask: BackendTask = {
      ...confirmTask,
      status: "succeeded",
      result: {
        summary: "Import complete",
        affectedPaths: ["wiki/sources/local/资料甲.md"],
        reference: {
          type: "import_v2_session_preview",
          sessionId: completion.sessionId,
          batchId: completion.batchId,
          completion,
        },
      },
    };
    await act(async () => notifyTaskEventListeners({
      eventId: "confirm-completed",
      eventType: "task_completed",
      projectId: projectA.projectId,
      taskId: completedTask.id,
      timestamp: "2026-07-14T00:01:00Z",
      payload: completedTask,
    }));

    await waitFor(() => expect(result.current.completion).toEqual(completion));
    expect(result.current.isConfirming).toBe(false);
  });

  it("unlocks the commit bar when a confirmation task finished before IPC returned", async () => {
    api.createSession.mockResolvedValue(session(projectA.projectId, [item("ready.md", "preview_ready")]));
    let resolveConfirm!: (value: BackendTask) => void;
    api.confirmSession.mockReturnValue(new Promise<BackendTask>((resolve) => { resolveConfirm = resolve; }));
    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));
    await waitFor(() => expect(result.current.session?.items).toHaveLength(1));

    act(() => { void result.current.confirm([{ itemId: "ready.md", resolution: null }]); });
    await waitFor(() => expect(result.current.isConfirming).toBe(true));

    const completed = task("confirm-race", projectA.projectId, "succeeded");
    useTaskStore.getState().upsertTask(completed);
    await act(async () => resolveConfirm(task("confirm-race")));

    await waitFor(() => expect(result.current.isConfirming).toBe(false));
    expect(useTaskStore.getState().tasks.find((entry) => entry.id === completed.id)?.status).toBe("succeeded");
  });

  it("unlocks the commit bar from task reconciliation when the terminal event is missed", async () => {
    api.createSession.mockResolvedValue(session(projectA.projectId, [item("ready.md", "preview_ready")]));
    const queuedTask = task("confirm-reconcile");
    api.confirmSession.mockResolvedValue(queuedTask);
    tauriInvoke.mockImplementation(async (command: string) =>
      command === "list_tasks" ? [{ ...queuedTask, status: "succeeded" as const }] : []);
    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));
    await waitFor(() => expect(result.current.session?.items).toHaveLength(1));

    await act(async () => result.current.confirm([
      { itemId: "ready.md", resolution: null },
    ]));
    expect(result.current.isConfirming).toBe(true);

    await waitFor(() => expect(result.current.isConfirming).toBe(false));
    expect(useTaskStore.getState().tasks.find((entry) => entry.id === queuedTask.id)?.status).toBe("succeeded");
  });

  it("refreshes the session when a started import task reaches a terminal state", async () => {
    api.createSession.mockResolvedValue(session(projectA.projectId));
    api.addUrl.mockResolvedValue(session(projectA.projectId, [item("url-1")]));
    const started = task("url-task");
    api.startBatch.mockResolvedValue(started);
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
    api.getSession.mockResolvedValue(selectionSession);
    const cancelledSession = session(projectA.projectId, [
      { ...item("retry.md", "cancelled"), taskId: "retry-task" },
      item("ready.md", "preview_ready"),
    ]);
    api.cancelItem.mockResolvedValue(cancelledSession);
    const taskLauncher = launcher();
    const { result } = renderHook(() => useImportWorkflow(projectA, "import", taskLauncher));
    await waitFor(() => expect(result.current.session?.items).toHaveLength(2));

    await act(async () => result.current.setItemSelected("ready.md", true));
    await act(async () => result.current.cancelItem("retry.md"));
    await act(async () => result.current.retryItem("retry.md"));
    await act(async () => result.current.confirm([{ itemId: "ready.md", resolution: null }]));

    expect(api.setSelection).toHaveBeenCalled();
    expect(api.startBatch).toHaveBeenCalledWith(expect.objectContaining({ itemIds: ["retry.md"] }));
    expect(api.cancelItem).toHaveBeenCalledWith({
      projectId: projectA.projectId,
      projectRootPath: projectA.rootPath,
      sessionId: "session-project-a",
      itemId: "retry.md",
    });
    expect(taskLauncher.cancel).not.toHaveBeenCalled();
    expect(api.confirmSession).toHaveBeenCalledWith({
      projectId: projectA.projectId,
      projectRootPath: projectA.rootPath,
      sessionId: "session-project-a",
      expectedSelectionRevision: 1,
      expectedConfirmationDigest: "digest-session-project-a",
    });
  });

  it("refreshes the selection snapshot before an immediate confirmation", async () => {
    const ready = { ...item("ready-now.md", "preview_ready"), selected: false };
    const selectedSession = session(projectA.projectId, [{ ...ready, selected: true }]);
    api.createSession.mockResolvedValue(session(projectA.projectId, [ready]));
    api.setSelection.mockResolvedValue(selectedSession);
    api.getSession.mockResolvedValue(selectedSession);
    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));
    await waitFor(() => expect(result.current.session?.items[0].selected).toBe(false));

    await act(async () => result.current.setItemSelected("ready-now.md", true));
    await act(async () => result.current.confirm());

    expect(api.confirmSession).toHaveBeenCalledWith({
      projectId: projectA.projectId,
      projectRootPath: projectA.rootPath,
      sessionId: "session-project-a",
      expectedSelectionRevision: 1,
      expectedConfirmationDigest: "digest-session-project-a",
    });
  });

  it("cancels one item through the item command without cancelling its shared operation", async () => {
    const operationId = "shared-operation";
    const first = { ...item("first.md", "extracting"), taskId: operationId };
    const second = { ...item("second.md", "extracting"), taskId: operationId };
    api.createSession.mockResolvedValue(session(projectA.projectId, [first, second]));
    api.cancelItem.mockResolvedValue(session(projectA.projectId, [
      { ...first, status: "cancelled" },
      second,
    ]));
    api.getSession.mockResolvedValue(session(projectA.projectId, [
      { ...first, status: "cancelled" },
      second,
    ]));
    const taskLauncher = launcher();
    const { result } = renderHook(() => useImportWorkflow(projectA, "import", taskLauncher));
    await waitFor(() => expect(result.current.session?.items).toHaveLength(2));

    await act(async () => result.current.cancelItem(first.itemId));

    expect(api.cancelItem).toHaveBeenCalledWith({
      projectId: projectA.projectId,
      projectRootPath: projectA.rootPath,
      sessionId: "session-project-a",
      itemId: first.itemId,
    });
    expect(taskLauncher.cancel).not.toHaveBeenCalled();
    expect(result.current.session?.items).toEqual([
      expect.objectContaining({ itemId: first.itemId, status: "cancelled", taskId: operationId }),
      expect.objectContaining({ itemId: second.itemId, status: "extracting", taskId: operationId }),
    ]);
    expect(useTaskStore.getState().tasks.find((candidate) => candidate.id === operationId)?.status).not.toBe("cancelled");
  });

  it("requires the project acknowledgement for a restricted selected duplicate", async () => {
    const restrictedDuplicate = item("restricted-duplicate.md", "preview_ready");
    restrictedDuplicate.selected = true;
    restrictedDuplicate.restrictedContent = true;
    api.createSession.mockResolvedValue(session(projectA.projectId, [restrictedDuplicate]));
    api.getRestrictedContentStatus.mockResolvedValue({ confirmationRequired: true });
    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));
    await waitFor(() => expect(result.current.session?.items).toHaveLength(1));

    await act(async () => result.current.confirm([
      { itemId: restrictedDuplicate.itemId, resolution: null },
    ]));

    expect(api.getRestrictedContentStatus).toHaveBeenCalledWith({
      projectId: projectA.projectId,
      projectRootPath: projectA.rootPath,
    });
    expect(result.current.restrictedCommitPending).toBe(true);
    expect(api.confirmSession).not.toHaveBeenCalled();
  });

  it("routes explicit local Agent and candidate actions through typed project-scoped contracts", async () => {
    const failed = item("failed.md", "failed");
    api.createSession.mockResolvedValue(session(projectA.projectId, [failed]));
    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));
    await waitFor(() => expect(result.current.session?.items).toHaveLength(1));

    await act(async () => result.current.invokeLocalAgent("failed.md", "manual", "codex"));
    await act(async () => result.current.selectAgentCandidate({ itemId: "failed.md", candidateId: "candidate-1", mergedMarkdown: null, expectedCurrentWikiSha256: null }));
    await act(async () => result.current.discardAgentCandidate("failed.md", "candidate-1"));

    expect(api.startAgentAssistance).toHaveBeenCalledWith(expect.objectContaining({ sessionId: "session-project-a", itemId: "failed.md", trigger: "manual", agentKind: "codex" }));
    expect(api.selectAgentCandidate).toHaveBeenCalledWith(expect.objectContaining({ candidateId: "candidate-1" }));
    expect(api.discardAgentCandidate).toHaveBeenCalledWith(expect.objectContaining({ candidateId: "candidate-1" }));
    expect(useTaskStore.getState().tasks.map((entry) => entry.id)).toContain("agent-task");
  });

  it("consumes completion when an Agent candidate finalizes as an exact duplicate", async () => {
    const failed = item("duplicate.md", "failed");
    const scan = vi.fn().mockResolvedValue(undefined);
    const originalScan = useWikiStore.getState().scan;
    const duplicateCompletion: ImportCompletion = {
      ...completion,
      newSources: [],
      updatedSources: [],
      duplicateSkips: completion.duplicateSkips,
    };
    api.createSession.mockResolvedValue(session(projectA.projectId, [failed]));
    api.selectAgentCandidate.mockResolvedValueOnce({
      projectId: projectA.projectId,
      sessionId: `session-${projectA.projectId}`,
      itemId: failed.itemId,
      candidateId: "candidate-duplicate",
      item: item(failed.itemId, "completed"),
      completion: duplicateCompletion,
    });
    useWikiStore.setState({ scan });
    try {
      const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));
      await waitFor(() => expect(result.current.session?.items).toHaveLength(1));

      await act(async () => result.current.selectAgentCandidate({
        itemId: failed.itemId,
        candidateId: "candidate-duplicate",
        mergedMarkdown: null,
        expectedCurrentWikiSha256: null,
      }));

      expect(result.current.session?.items[0].status).toBe("completed");
      expect(result.current.completion).toEqual(duplicateCompletion);
      expect(scan).toHaveBeenCalledWith(projectA.projectId, projectA.rootPath);
    } finally {
      useWikiStore.setState({ scan: originalScan });
    }
  });

  it("does not rescan project A when its Agent duplicate response arrives after switching projects", async () => {
    const failed = item("duplicate.md", "failed");
    const scan = vi.fn().mockResolvedValue(undefined);
    const originalScan = useWikiStore.getState().scan;
    let resolveSelection!: (value: Awaited<ReturnType<typeof api.selectAgentCandidate>>) => void;
    api.createSession.mockImplementation(({ projectId }: { projectId: string }) =>
      Promise.resolve(session(projectId, projectId === projectA.projectId ? [failed] : [])));
    api.selectAgentCandidate.mockReturnValueOnce(new Promise((resolve) => {
      resolveSelection = resolve;
    }));
    useWikiStore.setState({ scan });
    try {
      const { result, rerender } = renderHook(
        ({ project }) => useImportWorkflow(project, "import", launcher()),
        { initialProps: { project: projectA } },
      );
      await waitFor(() => expect(result.current.session?.projectId).toBe(projectA.projectId));

      let selection!: ReturnType<typeof result.current.selectAgentCandidate>;
      act(() => {
        selection = result.current.selectAgentCandidate({
          itemId: failed.itemId,
          candidateId: "candidate-duplicate",
          mergedMarkdown: null,
          expectedCurrentWikiSha256: null,
        });
      });
      rerender({ project: projectB });
      await waitFor(() => expect(result.current.session?.projectId).toBe(projectB.projectId));

      await act(async () => {
        resolveSelection({
          projectId: projectA.projectId,
          sessionId: `session-${projectA.projectId}`,
          itemId: failed.itemId,
          candidateId: "candidate-duplicate",
          item: item(failed.itemId, "completed"),
          completion,
        });
        await selection;
      });

      expect(scan).not.toHaveBeenCalled();
      expect(result.current.session?.projectId).toBe(projectB.projectId);
    } finally {
      useWikiStore.setState({ scan: originalScan });
    }
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
    await act(async () => result.current.installCapability(
      "gated-url",
      "browser-runtime",
      "fixture-requirement-revision",
    ));

    expect(api.beginLogin).toHaveBeenCalledWith(expect.objectContaining({ sessionId: "session-project-a", itemId: "gated-url", platform: "wechat" }));
    expect(api.completeLogin).toHaveBeenCalledWith(expect.objectContaining({ importSessionId: "session-project-a", connectorSessionId: "connector-1" }));
    expect(api.revokeLogin).toHaveBeenCalledWith({ sessionId: "connector-1", platform: null });
    expect(api.authorizePrivateTarget).toHaveBeenCalledWith(expect.objectContaining({ url: "https://private.example.com/article" }));
    expect(api.getCapabilityRequirement).toHaveBeenCalledWith(expect.objectContaining({ itemId: "gated-url" }));
    expect(api.installCapability).toHaveBeenCalledWith(expect.objectContaining({ capabilityId: "browser-runtime", acknowledgeInstall: true }));
    expect(useTaskStore.getState().tasks).toContainEqual(expect.objectContaining({ id: "capability-task" }));

    const resumed = { ...item("gated-url", "extracting"), taskId: "resumed-import-task" };
    api.getSession.mockResolvedValueOnce(session(projectA.projectId, [resumed]));
    const installed = task("capability-task", projectA.projectId, "succeeded");
    useTaskStore.getState().upsertTask(installed);
    await act(async () => notifyTaskEventListeners({
      eventId: "capability-installed",
      eventType: "task_completed",
      projectId: projectA.projectId,
      taskId: installed.id,
      timestamp: "2026-07-14T00:00:00Z",
      payload: installed,
    }));

    await waitFor(() => expect(result.current.session?.items[0]).toMatchObject({
      status: "extracting",
      taskId: "resumed-import-task",
    }));
  });

  it("routes migration and history reads through the current project identity", async () => {
    const { result } = renderHook(() => useImportWorkflow(projectA, "import", launcher()));
    await waitFor(() => expect(result.current.bootstrapState).toBe("ready"));

    const inventory = await act(async () => result.current.scanMigration());
    const preparation = await act(async () => result.current.planMigration(inventory!));
    const confirmation: MigrationConfirmation = { planFingerprint: "plan-fingerprint", token: "opaque-token", acknowledgeNoGitRollback: true };
    await act(async () => result.current.applyMigration(preparation!.plan, confirmation));
    await act(async () => result.current.getMigrationStatus());
    await act(async () => result.current.resumeMigration(preparation!.plan, confirmation));
    await act(async () => result.current.listHistory("cursor-1"));

    expect(api.scanMigration).toHaveBeenCalledWith({ projectId: projectA.projectId, projectRootPath: projectA.rootPath });
    expect(api.planMigration).toHaveBeenCalledWith({ projectId: projectA.projectId, projectRootPath: projectA.rootPath, inventory });
    expect(api.applyMigration).toHaveBeenCalledWith({ projectId: projectA.projectId, projectRootPath: projectA.rootPath, plan: preparation!.plan, confirmation });
    expect(api.getMigrationStatus).toHaveBeenCalledWith({ projectId: projectA.projectId, projectRootPath: projectA.rootPath });
    expect(api.resumeMigration).toHaveBeenCalledWith({ projectId: projectA.projectId, projectRootPath: projectA.rootPath, plan: preparation!.plan, confirmation });
    expect(api.listHistory).toHaveBeenCalledWith({ projectId: projectA.projectId, projectRootPath: projectA.rootPath, cursor: "cursor-1", limit: 50 });
    expect(useTaskStore.getState().tasks.map((entry) => entry.id)).toEqual(expect.arrayContaining(["migration-task", "migration-resume-task"]));
  });

  it("opens the first imported Source without starting Compile", async () => {
    const taskLauncher = launcher();
    const scan = vi.fn().mockResolvedValue(undefined);
    const openPage = vi.fn().mockResolvedValue(undefined);
    const originalScan = useWikiStore.getState().scan;
    const originalOpenPage = useWikiStore.getState().openPage;
    useWikiStore.setState({ scan, openPage });
    useNavigationStore.getState().setActiveView("import");

    try {
      const { result } = renderHook(() => useImportWorkflow(projectA, "import", taskLauncher));
      await waitFor(() => expect(result.current.bootstrapState).toBe("ready"));

      await act(async () => result.current.viewImportedSources(completion));

      expect(scan).toHaveBeenCalledWith(projectA.projectId, projectA.rootPath);
      expect(openPage).toHaveBeenCalledWith(
        projectA.projectId,
        projectA.rootPath,
        "wiki/sources/local/资料甲.md",
      );
      expect(useNavigationStore.getState().activeView).toBe("wiki");
    } finally {
      useWikiStore.setState({ scan: originalScan, openPage: originalOpenPage });
    }
  });

  it("prepares Update Wiki only from the explicit action with exact changed Source versions", async () => {
    const taskLauncher = launcher();
    useNavigationStore.setState({ workflowLaunchIntent: null, activeView: "import" });
    const { result } = renderHook(() => useImportWorkflow(projectA, "import", taskLauncher));
    await waitFor(() => expect(result.current.bootstrapState).toBe("ready"));

    await act(async () => result.current.updateWiki(completion));

    expect(useNavigationStore.getState().workflowLaunchIntent).toEqual({
      projectId: projectA.projectId,
      projectRootPath: projectA.rootPath,
      kind: "update_wiki",
      origin: "import",
      scopePreset: {
        kind: "update_wiki",
        mode: "changed_sources",
        sourceVersions: [
        {
          sourceId: "source-new",
          versionId: "version-new",
        },
        {
          sourceId: "source-updated",
          versionId: "version-updated",
        },
      ],
      },
    });
  });
});
