import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { i18next } from "../../i18n";
import { useAppCapabilityStore } from "../../stores/appCapabilityStore";
import { useTaskStore } from "../../stores/taskStore";
import { useToastStore } from "../../stores/toastStore";
import type { ImportCompletion, ImportItem, ImportItemResolution, ImportSession } from "../../types/importV2";
import type { AgentCandidateView } from "../../types/importV2Agent";
import type { ImportFrontendReadiness, ImportHistoryDetailPage, ImportHistoryPage } from "../../types/importV2Presentation";
import type { ImportWorkflow } from "./useImportWorkflow";
import { buildCandidateSelectionRequest, ImportView } from "./ImportView";
import type { ImportCandidateDiffIntent } from "./ImportCandidateDiffDialog";

function item(itemId: string, status: ImportItem["status"], selected = false): ImportItem {
  return {
    itemId,
    input: { kind: itemId.startsWith("url") ? "url" : "file", displayName: itemId, locator: itemId, normalizedLocator: null },
    status,
    selected,
    taskId: null,
    progress: status === "extracting" ? { current: 2, total: 4, label: null } : null,
    attempts: [],
    preview: status === "preview_ready" ? { title: itemId, markdown: { kind: "markdown", relativePath: "wiki/item.md", sha256: "a", sizeBytes: 10 }, assets: [], sourceSnapshot: { kind: "source_snapshot", relativePath: "raw/item", sha256: "b", sizeBytes: 10 }, quality: { level: "pass", metrics: [], warnings: [] } } : null,
    issue: status === "failed" ? { code: "EXTRACT_FAILED", message: "Needs review", stage: "extract", retryable: true, userActionRequired: true, recoveryActions: ["retry"], availableActions: [] } : null,
  };
}

function session(items: ImportItem[]): ImportSession {
  return { schemaVersion: 2, sessionId: "session-a", projectId: "project-a", status: "draft", resourceMode: "balanced", createdAt: "2026-07-13T00:00:00Z", updatedAt: "2026-07-13T00:00:00Z", items };
}

function workflow(overrides: Partial<ImportWorkflow> = {}): ImportWorkflow {
  const current = session([]);
  return {
    projectKey: "project-a\0D:/wiki/project-a",
    session: current,
    readiness: { backendVersion: "2.0.0", active: true, migrationStatus: "applied", unfinishedSessionId: current.sessionId, legacyHistoryAvailable: false },
    bootstrapState: "ready",
    visibleItems: current.items,
    counts: { all: 0, active: 0, ready: 0, needsAction: 0, failed: 0, completed: 0 },
    progress: { completed: 0, total: 0, active: 0 },
    selectedItemId: null,
    filter: "all",
    addPaths: vi.fn(),
    addText: vi.fn(),
    confirmCollection: vi.fn(),
    dismissCollection: vi.fn(),
    planRemoteMediaRetention: vi.fn(),
    confirmRemoteMediaRetention: vi.fn(),
    dismissRemoteMediaRetention: vi.fn(),
    addUrl: vi.fn(),
    setItemSelected: vi.fn(),
    startItems: vi.fn(),
    retryItem: vi.fn(),
    cancelItem: vi.fn(),
    skipItem: vi.fn(),
    authorizeLocalAsr: vi.fn(),
    authorizeLocalOcr: vi.fn(),
    selectSubtitle: vi.fn(),
    confirm: vi.fn(),
    restrictedCommitPending: false,
    confirmRestrictedContent: vi.fn(),
    dismissRestrictedContent: vi.fn(),
    viewImportedSources: vi.fn(),
    updateWiki: vi.fn(),
    refreshSession: vi.fn(),
    selectItem: vi.fn(),
    setFilter: vi.fn(),
    loadPreview: vi.fn(),
    loadMergeContext: vi.fn(),
    setItemResolution: vi.fn(),
    stageManualMerge: vi.fn(),
    loadSession: vi.fn(),
    loadCompletion: vi.fn(),
    invokeLocalAgent: vi.fn(),
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
    listHistory: vi.fn().mockResolvedValue({ entries: [], legacyReadOnly: [], nextCursor: null, warnings: [] }),
    loadHistoryDetail: vi.fn(),
    isConfirming: false,
    requestClipboard: vi.fn(),
    ...overrides,
    collectionPreview: overrides.collectionPreview ?? null,
    loadCollectionPage: overrides.loadCollectionPage ?? vi.fn(),
    remoteMediaRetentionPlan: overrides.remoteMediaRetentionPlan ?? null,
    completion: overrides.completion ?? null,
    getAsrEnablementPlan: overrides.getAsrEnablementPlan ?? vi.fn(),
  };
}

const MERGE_BINDING = {
  sourceId: "source-a",
  candidateHash: "candidate-hash",
  currentHash: "current-hash",
  targetVersionId: "version-a",
};

function mergeItem(defaultResolution?: ImportItemResolution): ImportItem {
  return {
    ...item("merge.md", "needs_merge", true),
    preview: {
      title: "merge.md",
      markdown: { kind: "markdown", relativePath: "candidate.md", sha256: "candidate-hash", sizeBytes: 10 },
      assets: [],
      sourceSnapshot: { kind: "source_snapshot", relativePath: "source.json", sha256: "source-hash", sizeBytes: 10 },
      quality: { level: "pass", metrics: [], warnings: [] },
      resolution: {
        kind: "needs_three_way_merge",
        binding: MERGE_BINDING,
        ...(defaultResolution ? { defaultResolution } : {}),
      },
    },
    issue: {
      code: "IMPORT_V2_COMMIT_CONFLICT",
      message: "merge required",
      stage: "commit",
      retryable: false,
      userActionRequired: true,
      recoveryActions: [],
      availableActions: [],
    },
  };
}

beforeEach(async () => {
  await i18next.changeLanguage("en");
  useToastStore.setState({ toasts: [] });
  useAppCapabilityStore.getState().resetForTests();
});

describe("ImportView V2 composition", () => {
  it("hands a restored durable collection confirmation back to the workflow", async () => {
    const previousTaskState = useTaskStore.getState();
    const restoreCollection = vi.fn();
    const preview = {
      taskId: "collection-task",
      collectionRef: "import-web-collection:durable",
      sourceUrl: "https://space.bilibili.com/42",
      platform: "bilibili",
      title: "Durable collection",
      totalDurationSeconds: null,
      estimatedLoginCount: 0,
      estimatedAsrCount: 1,
      discoveredTotal: 1,
      loadedCount: 1,
      hasMore: false,
      nextCursor: null,
      items: [{ itemRef: "item-1", title: "Entry 1", publicUrl: "https://example.com/1" }],
    };
    useTaskStore.setState({
      tasks: [{
        id: "collection-task",
        taskType: "import",
        projectId: "project-a",
        operation: { kind: "import_collection_discovery", sessionId: "session-a" },
        title: "Discover collection",
        status: "waiting_for_confirmation",
        progress: null,
        startedAt: "2026-08-30T00:00:00Z",
        updatedAt: "2026-08-30T00:00:00Z",
        completedAt: null,
        cancellable: true,
        logPath: null,
        result: {
          summary: "Collection preview ready",
          affectedPaths: [],
          reference: {
            type: "import_collection_preview",
            sessionId: "session-a",
            collectionRef: preview.collectionRef,
            preview,
          },
        },
        error: null,
      }],
    });
    const view = render(<ImportView workflow={workflow({ restoreCollection })} />);
    try {
      await waitFor(() => expect(restoreCollection).toHaveBeenCalledWith(preview));
    } finally {
      view.unmount();
      useTaskStore.setState(previousTaskState, true);
    }
  });

  it("opens the typed per-item Source merge flow instead of the Agent candidate flow", async () => {
    const unresolved = mergeItem();
    const loadMergeContext = vi.fn().mockResolvedValue({
      resolution: { kind: "needs_three_way_merge", binding: MERGE_BINDING },
      baselineMarkdown: "# old",
      currentMarkdown: "# current",
      candidateMarkdown: "# imported",
    });
    const setItemResolution = vi.fn().mockResolvedValue(undefined);
    render(<ImportView workflow={workflow({
      session: session([unresolved]),
      visibleItems: [unresolved],
      counts: { all: 1, active: 0, ready: 0, needsAction: 1, failed: 0, completed: 0 },
      loadMergeContext,
      setItemResolution,
    })} />);

    fireEvent.click(screen.getAllByRole("button", { name: "Resolve merge" })[0]);
    expect(await screen.findByRole("heading", { name: "Resolve update: merge.md" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Use imported update" }));

    await waitFor(() => {
      expect(setItemResolution).toHaveBeenCalledWith("merge.md", {
        kind: "apply_import_candidate",
        ...MERGE_BINDING,
      });
    });
  });

  it("submits a persisted merge decision without blocking other ready items", async () => {
    const resolution: ImportItemResolution = {
      kind: "keep_current_source",
      ...MERGE_BINDING,
    };
    const resolved = mergeItem(resolution);
    const confirm = vi.fn();
    render(<ImportView workflow={workflow({
      session: session([resolved]),
      visibleItems: [resolved],
      counts: { all: 1, active: 0, ready: 1, needsAction: 0, failed: 0, completed: 0 },
      confirm,
    })} />);

    expect(screen.getByText("Update decision selected")).toBeInTheDocument();
    expect(screen.getByText("Updates 1")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Import to Source library (1)" }));
    expect(confirm).toHaveBeenCalledWith();
  });

  it("includes a restricted exact duplicate in the confirmation batch", () => {
    const resolution: ImportItemResolution = {
      kind: "exact_duplicate_skip",
      ...MERGE_BINDING,
    };
    const duplicate = item("restricted-duplicate.md", "preview_ready", true);
    duplicate.restrictedContent = true;
    duplicate.preview = {
      ...duplicate.preview!,
      resolution: {
        kind: "exact_duplicate",
        binding: MERGE_BINDING,
        defaultResolution: resolution,
      },
    };
    const confirm = vi.fn();
    render(<ImportView workflow={workflow({
      session: session([duplicate]),
      visibleItems: [duplicate],
      counts: { all: 1, active: 0, ready: 1, needsAction: 0, failed: 0, completed: 0 },
      confirm,
    })} />);

    fireEvent.click(screen.getByRole("button", { name: "Import to Source library (1)" }));
    expect(confirm).toHaveBeenCalledWith();
  });

  it("binds a three-way merge selection to the Wiki version shown in the diff", () => {
    const view = {
      projectId: "project-a",
      sessionId: "session-a",
      itemId: "item-a",
      candidate: { candidateId: "candidate-a" },
      diff: {
        candidateId: "candidate-a",
        baselineMarkdown: "# Baseline",
        currentMarkdown: "# Current",
        currentMarkdownSha256: "current-hash",
        agentMarkdown: "# Agent",
        unifiedDiff: "@@",
        needsThreeWayMerge: true,
      },
    } as unknown as AgentCandidateView;
    const intent: ImportCandidateDiffIntent = { kind: "apply_merged", candidateId: "candidate-a", mergedMarkdown: "# Human merged" };

    expect(buildCandidateSelectionRequest(view, intent)).toEqual({
      itemId: "item-a",
      candidateId: "candidate-a",
      mergedMarkdown: "# Human merged",
      expectedCurrentWikiSha256: "current-hash",
    });
  });

  it("does not send merge-only fields for a clean Agent candidate", () => {
    const view = {
      itemId: "item-a",
      candidate: { candidateId: "candidate-a" },
      diff: {
        candidateId: "candidate-a",
        baselineMarkdown: "# Baseline",
        currentMarkdown: null,
        currentMarkdownSha256: null,
        agentMarkdown: "# Agent",
        unifiedDiff: "@@",
        needsThreeWayMerge: false,
      },
    } as unknown as AgentCandidateView;

    expect(buildCandidateSelectionRequest(view, { kind: "choose_agent", candidateId: "candidate-a" })).toEqual({
      itemId: "item-a",
      candidateId: "candidate-a",
      mergedMarkdown: null,
      expectedCurrentWikiSha256: null,
    });
  });

  it("renders a loading state without exposing legacy controls", () => {
    render(<ImportView workflow={workflow({ bootstrapState: "loading", session: null })} />);
    expect(screen.getByRole("status")).toHaveTextContent(/loading import workspace/i);
    expect(screen.queryByRole("button", { name: /delete|replace source|legacy/i })).not.toBeInTheDocument();
  });

  it("composes methods, empty queue, and a disabled commit bar for an empty draft", () => {
    render(<ImportView workflow={workflow()} />);
    expect(screen.getByRole("region", { name: /import methods/i })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: /import queue/i })).toHaveTextContent(/no sources/i);
    expect(screen.getByRole("button", { name: /import to source library/i })).toBeDisabled();
  });

  it("keeps history and capability packs in separate workbench sections", async () => {
    const listHistory = vi.fn().mockResolvedValue({ entries: [], legacyReadOnly: [], nextCursor: null, warnings: [] });
    const readiness: ImportFrontendReadiness = {
      backendVersion: "2.0.0",
      active: true,
      migrationStatus: "applied",
      unfinishedSessionId: "session-a",
      legacyHistoryAvailable: false,
      capabilities: [{ capabilityId: "asr-sensevoice-small", route: "media.asr", available: true, reasonCode: null }],
    };
    useAppCapabilityStore.setState({
      initialized: true,
      capabilities: [{
        capabilityId: "asr-sensevoice-small",
        nameKey: "importV2.capabilityName.asr-sensevoice-small",
        purposeKey: "importV2.capabilityPurpose.asr",
        category: "media_asr",
        routes: ["media.asr"],
        formats: ["audio"],
        platformContentTypes: [],
        targetTriple: "x86_64-pc-windows-msvc",
        publisherKeyId: "release-key",
        sourceDomain: "github.com",
        targetVersion: "1.0.0",
        acknowledgementVersion: "ack-v1",
        installAllowed: true,
        distribution: { state: "published" },
        installation: { state: "healthy", healthyVersion: "1.0.0" },
        operation: {},
        update: { state: "none" },
        displayState: "installed",
        compressedBytes: 1,
        installedBytes: 1,
        modelBytes: 1,
        licenseExpression: "Apache-2.0",
        thirdPartyNotices: [],
        runtimeNetwork: false,
        runtimeSubprocess: true,
        runtimeFilesystem: ["app-capability-dir"],
        currentProjectWaitingCount: 0,
      }],
    });
    render(<ImportView workflow={workflow({ readiness, listHistory })} />);

    expect(listHistory).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: /history/i }));
    await waitFor(() => expect(listHistory).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(screen.getByText(/no import history/i)).toBeInTheDocument());
    expect(screen.queryByRole("button", { name: /import to source library/i })).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /capability packs/i }));
    expect(screen.getByText("Local speech recognition")).toBeInTheDocument();
    expect(screen.queryByText("asr-sensevoice-small")).not.toBeInTheDocument();
    expect(screen.getByLabelText("audio · media.asr")).toBeInTheDocument();
    expect(screen.getByText("installed 1.0.0 · target 1.0.0")).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: /import methods/i })).not.toBeInTheDocument();
  });

  it("coalesces the initial History read when a completion is already present", async () => {
    const listHistory = vi.fn().mockResolvedValue({ entries: [], legacyReadOnly: [], nextCursor: null, warnings: [] });
    render(<ImportView workflow={workflow({
      completion: { batchId: "batch-existing", sessionId: "session-a", newSources: [], updatedSources: [], duplicateSkips: [], warnings: [], failures: [] },
      listHistory,
    })} />);

    fireEvent.click(screen.getByRole("button", { name: /history/i }));
    await waitFor(() => expect(listHistory).toHaveBeenCalledTimes(1));
  });

  it("restores and persists the project-scoped tab, filter, scroll, and input collapse state", async () => {
    const setFilter = vi.fn();
    const saveWorkbenchPreferences = vi.fn().mockResolvedValue(undefined);
    const currentWorkflow = workflow({
      setFilter,
      loadWorkbenchPreferences: vi.fn().mockResolvedValue({
        schemaVersion: 1,
        activeSection: "capabilities",
        queueFilter: "failed",
        workbenchScrollTop: 18,
        capabilitiesScrollTop: 42,
        historyScrollTop: 73,
        sourceMethodsExpanded: false,
      }),
      saveWorkbenchPreferences,
      readiness: {
        backendVersion: "2.0.0",
        active: true,
        migrationStatus: "applied",
        unfinishedSessionId: "session-a",
        legacyHistoryAvailable: false,
        capabilities: [],
      },
    });
    const view = render(<ImportView workflow={currentWorkflow} />);

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Capability packs" })).toHaveAttribute(
        "aria-current",
        "page",
      );
    });
    expect(setFilter).toHaveBeenLastCalledWith("failed");
    await waitFor(() => {
      expect(view.container.querySelector<HTMLElement>(".import-v2-scroll")?.scrollTop).toBe(42);
    });

    fireEvent.click(screen.getByRole("button", { name: "Workbench" }));
    expect(await screen.findByRole("button", { name: "Expand add methods" })).toBeInTheDocument();
    await waitFor(() => expect(saveWorkbenchPreferences).toHaveBeenCalledWith(
      expect.objectContaining({
        activeSection: "workbench",
        queueFilter: "failed",
        capabilitiesScrollTop: 42,
        sourceMethodsExpanded: false,
      }),
    ), { timeout: 1_000 });
  });

  it("flushes a pending preference update when the view unmounts", async () => {
    const saveWorkbenchPreferences = vi.fn().mockResolvedValue(undefined);
    const currentWorkflow = workflow({
      loadWorkbenchPreferences: vi.fn().mockResolvedValue({
        schemaVersion: 1,
        activeSection: "capabilities",
        queueFilter: "all",
        workbenchScrollTop: 0,
        capabilitiesScrollTop: 0,
        historyScrollTop: 0,
        sourceMethodsExpanded: true,
      }),
      saveWorkbenchPreferences,
    });
    const view = render(<ImportView workflow={currentWorkflow} />);

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Capability packs" })).toHaveAttribute(
        "aria-current",
        "page",
      );
    });
    fireEvent.click(screen.getByRole("button", { name: "Workbench" }));
    view.unmount();

    expect(saveWorkbenchPreferences).toHaveBeenCalledWith(
      expect.objectContaining({ activeSection: "workbench" }),
    );
  });

  it("restarts Compile from History with the persisted completion identity", async () => {
    const savedCompletion: ImportCompletion = {
      sessionId: "session-history",
      batchId: "batch-history",
      newSources: [{
        sourceId: "source-history",
        versionId: "version-history",
        wikiPath: "wiki/sources/local/历史资料.md",
        contentHash: "d".repeat(64),
      }],
      updatedSources: [],
      duplicateSkips: [],
      warnings: [],
      failures: [],
    };
    const historyPage: ImportHistoryPage = {
      entries: [{
        id: "batch-history",
        title: "Historical import",
        status: "completed",
        sessionId: "session-history",
        batchId: "batch-history",
        taskId: null,
        startedAt: null,
        updatedAt: null,
        completedAt: null,
        legacyReadOnly: false,
        itemCount: 1,
        committedCount: 1,
        failedCount: 0,
        sampleLabels: ["历史资料.md"],
        availableActions: ["update_wiki"],
      }],
      legacyReadOnly: [],
      nextCursor: null,
      warnings: [],
    };
    const loadCompletion = vi.fn().mockResolvedValue(savedCompletion);
    const updateWiki = vi.fn().mockResolvedValue(null);
    render(<ImportView workflow={workflow({
      listHistory: vi.fn().mockResolvedValue(historyPage),
      loadCompletion,
      updateWiki,
    })} />);

    fireEvent.click(screen.getByRole("button", { name: /history/i }));
    fireEvent.click(await screen.findByRole("button", { name: /update wiki/i }));

    await waitFor(() => expect(loadCompletion).toHaveBeenCalledWith("session-history", "batch-history"));
    expect(updateWiki).toHaveBeenCalledWith(savedCompletion);
  });

  it("resolves a historical result from a later bounded detail page", async () => {
    const entry: ImportHistoryPage["entries"][number] = {
      id: "batch-paged-result",
      title: "Paged historical import",
      status: "completed",
      sessionId: "session-paged-result",
      batchId: "batch-paged-result",
      taskId: null,
      startedAt: null,
      updatedAt: null,
      completedAt: null,
      legacyReadOnly: false,
      itemCount: 51,
      committedCount: 51,
      failedCount: 0,
      sampleLabels: ["first.md"],
      availableActions: ["open_detail", "open_result"],
    };
    const firstPage: ImportHistoryDetailPage = {
      entry,
      items: [item("first.md", "completed")],
      nextCursor: "after-50",
      total: 51,
    };
    const previewItem = item("result-51.md", "preview_ready");
    previewItem.status = "completed";
    const secondPage: ImportHistoryDetailPage = {
      entry,
      items: [previewItem],
      nextCursor: null,
      total: 51,
    };
    const loadHistoryDetail = vi.fn()
      .mockResolvedValueOnce(firstPage)
      .mockResolvedValueOnce(secondPage);
    const loadPreview = vi.fn().mockResolvedValue({
      sessionId: "session-paged-result",
      itemId: "result-51.md",
      candidateId: null,
      title: "result-51.md",
      markdown: "# Result",
      truncated: false,
      totalBytes: 8,
      sha256: "hash",
    });
    render(<ImportView workflow={workflow({
      listHistory: vi.fn().mockResolvedValue({ entries: [entry], legacyReadOnly: [], nextCursor: null, warnings: [] }),
      loadHistoryDetail,
      loadPreview,
    })} />);

    fireEvent.click(screen.getByRole("button", { name: /history/i }));
    fireEvent.click(await screen.findByRole("button", { name: /open result/i }));

    await waitFor(() => expect(loadHistoryDetail).toHaveBeenNthCalledWith(2, "batch-paged-result", "after-50"));
    await waitFor(() => expect(loadPreview).toHaveBeenCalledWith(expect.objectContaining({ itemId: "result-51.md" })));
    expect(screen.queryByText(/result is no longer available/i)).not.toBeInTheDocument();
  });

  it("refreshes the first history page when the scoped rebuild task succeeds", async () => {
    const previousTaskState = useTaskStore.getState();
    useTaskStore.setState({
      activeProjectId: "project-a",
      activeProjectRootPath: "D:/wiki/project-a",
      taskById: {},
      taskFacts: {},
      taskIdsByProject: {},
      runningCountByProject: {},
      tasks: [],
      runningCount: 0,
    });
    const listHistory = vi.fn()
      .mockResolvedValueOnce({
        entries: [],
        legacyReadOnly: [],
        nextCursor: null,
        warnings: [{ code: "IMPORT_V2_HISTORY_INDEX_REBUILD_REQUIRED", message: "Preparing history", evidencePath: ".app/import-history/index/manifest.json" }],
      })
      .mockResolvedValueOnce({
        entries: [{
          id: "rebuilt-entry",
          title: "Rebuilt history",
          status: "completed",
          sessionId: "rebuilt-session",
          batchId: "rebuilt-entry",
          taskId: null,
          startedAt: null,
          updatedAt: null,
          completedAt: null,
          legacyReadOnly: false,
          itemCount: 0,
          committedCount: 0,
          failedCount: 0,
          sampleLabels: [],
          availableActions: [],
        }],
        legacyReadOnly: [],
        nextCursor: null,
        warnings: [],
      });
    const view = render(<ImportView workflow={workflow({ listHistory })} />);
    try {
      fireEvent.click(screen.getByRole("button", { name: /history/i }));
      await screen.findByText("Preparing history");

      act(() => useTaskStore.getState().upsertTask({
        id: "history-rebuild-success",
        taskType: "import",
        projectId: "project-a",
        batchId: "history-rebuild-success",
        operation: { kind: "import_history_index_rebuild" },
        title: "Prepare import history",
        status: "succeeded",
        progress: { current: 100, total: null, label: "Preparing import history" },
        startedAt: "2026-08-28T00:00:00Z",
        updatedAt: "2026-08-28T00:01:00Z",
        completedAt: "2026-08-28T00:01:00Z",
        cancellable: true,
        logPath: null,
        result: null,
        error: null,
      }));

      await waitFor(() => expect(listHistory).toHaveBeenCalledTimes(2));
      expect(await screen.findByText("Rebuilt history")).toBeInTheDocument();
    } finally {
      view.unmount();
      useTaskStore.setState(previousTaskState, true);
    }
  });

  it("keeps mixed queue states actionable and selection keyboard accessible", () => {
    const mixed = [item("研究笔记.md", "preview_ready", true), item("processing.pdf", "extracting"), item("failed.docx", "failed")];
    const currentWorkflow = workflow({ session: session(mixed), visibleItems: mixed, counts: { all: 3, active: 1, ready: 1, needsAction: 1, failed: 1, completed: 0 }, progress: { completed: 0, total: 3, active: 1 } });
    const view = render(<ImportView workflow={currentWorkflow} />);
    expect(screen.getAllByText("研究笔记.md").length).toBeGreaterThan(0);
    expect(screen.getAllByText("Ready").length).toBeGreaterThan(0);
    expect(screen.getByRole("button", { name: /import to source library/i })).toBeEnabled();
    expect(screen.getByText("Pending 1")).toBeInTheDocument();
    fireEvent.keyDown(view.getByTestId("import-item-研究笔记.md"), { key: "Enter" });
    expect(currentWorkflow.selectItem).toHaveBeenCalledWith("研究笔记.md");
    expect(screen.getByRole("status")).toHaveTextContent(
      "Queue updated: 3 items, 1 need action, 1 ready.",
    );
  });

  it("explains how to enable explicit Agent recovery when no local Agent is installed", async () => {
    const failed = item("failed.docx", "failed");
    failed.issue = {
      ...failed.issue!,
      availableActions: ["invoke_local_agent"],
    };
    const currentWorkflow = workflow({
      session: session([failed]),
      visibleItems: [failed],
      counts: { all: 1, active: 0, ready: 0, needsAction: 1, failed: 1, completed: 0 },
    });
    render(<ImportView workflow={currentWorkflow} />);

    fireEvent.click(screen.getByRole("button", { name: "More actions for failed.docx" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Agent assistance" }));

    await waitFor(() => expect(useToastStore.getState().toasts).toContainEqual(
      expect.objectContaining({
        tone: "warning",
        message: "No local Agent is installed. Configure an Agent in Settings, then try again.",
      }),
    ));
    expect(currentWorkflow.invokeLocalAgent).not.toHaveBeenCalled();
  });

  it("keeps migration engineering UI out of a blocked normal workbench", () => {
    const readiness: ImportFrontendReadiness = { backendVersion: "2.0.0", active: false, migrationStatus: "awaiting_confirmation", unfinishedSessionId: null, legacyHistoryAvailable: true };
    render(<ImportView workflow={workflow({ readiness, bootstrapState: "blocked", session: null })} />);
    expect(screen.queryByRole("tooltip")).not.toBeInTheDocument();
    expect(screen.queryByText(/migration|fingerprint|dry-run/i)).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /review migration/i })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /switch to v1|legacy write/i })).not.toBeInTheDocument();
  });

  it("keeps the V2 commit action enabled when migration is not active", () => {
    const ready = item("old-project.md", "preview_ready", true);
    const readiness: ImportFrontendReadiness = { backendVersion: "2.0.0", active: false, migrationStatus: "not_scanned", unfinishedSessionId: null, legacyHistoryAvailable: true };
    const currentWorkflow = workflow({
      readiness,
      session: session([ready]),
      visibleItems: [ready],
      counts: { all: 1, active: 0, ready: 1, needsAction: 0, failed: 0, completed: 0 },
      progress: { completed: 0, total: 1, active: 0 },
    });
    render(<ImportView workflow={currentWorkflow} />);

    expect(screen.getByRole("button", { name: /import to source library/i })).toBeEnabled();
  });

  it("ignores a history response from the previous project scope", async () => {
    let resolvePrevious!: (page: ImportHistoryPage) => void;
    const previousHistory = vi.fn(() => new Promise<ImportHistoryPage>((resolve) => {
      resolvePrevious = resolve;
    }));
    const currentHistory = vi.fn().mockResolvedValue({ entries: [], legacyReadOnly: [], nextCursor: null, warnings: [] });
    const previousWorkflow = workflow({ projectKey: "project-a\0D:/wiki/a", listHistory: previousHistory });
    const currentWorkflow = workflow({ projectKey: "project-a\0D:/wiki/b", listHistory: currentHistory });
    const { rerender } = render(<ImportView workflow={previousWorkflow} />);
    expect(previousHistory).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: /history/i }));
    await waitFor(() => expect(previousHistory).toHaveBeenCalledTimes(1));
    rerender(<ImportView workflow={currentWorkflow} />);
    await waitFor(() => expect(currentHistory).toHaveBeenCalledTimes(1));

    await act(async () => {
      resolvePrevious({
        entries: [{
          id: "old-entry",
          title: "Old project history",
          status: "completed",
          sessionId: "old-session",
          batchId: "old-batch",
          taskId: null,
          startedAt: null,
          updatedAt: null,
          completedAt: null,
          legacyReadOnly: false,
          itemCount: 0,
          committedCount: 0,
          failedCount: 0,
          sampleLabels: [],
          availableActions: [],
        }],
        legacyReadOnly: [],
        nextCursor: null,
        warnings: [],
      });
    });

    expect(screen.queryByText("Old project history")).not.toBeInTheDocument();
  });

  it("renders the same shell with Chinese copy", async () => {
    await i18next.changeLanguage("zh-CN");
    render(<ImportView workflow={workflow()} />);
    expect(screen.getByRole("region", { name: /导入方式/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /导入到来源库/ })).toBeDisabled();
  });
});
