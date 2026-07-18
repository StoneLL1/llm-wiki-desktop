import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import type { AiCapabilitiesWorkflow } from "../../hooks/useAiCapabilities";
import { useImportStore } from "../../stores/importStore";
import { useTaskStore } from "../../stores/taskStore";
import { useToastStore } from "../../stores/toastStore";
import type { AgentCandidateView } from "../../types/importV2Agent";
import type { CommitConflictAction, CommitItemDecision, ImportItem, ImportSession } from "../../types/importV2";
import { canOpenHistoricalResult, type ImportHistoryAction, type ImportHistoryEntry, type ImportHistoryPage } from "../../types/importV2Presentation";
import { ImportCommitBar } from "./ImportCommitBar";
import { ImportBatchStatus } from "./ImportBatchStatus";
import { ImportDiscoveryStatus } from "./ImportDiscoveryStatus";
import { ImportHistoryPanel } from "./ImportHistoryPanel";
import { ImportHistoryDetailDialog } from "./ImportHistoryDetailDialog";
import { ImportMarkdownPreviewDialog, type ImportPreviewIdentity } from "./ImportMarkdownPreviewDialog";
import { ImportMigrationNotice } from "./ImportMigrationNotice";
import { ImportQueue } from "./ImportQueue";
import { ImportSourceMethods } from "./ImportSourceMethods";
import { ImportV2Dialogs } from "./ImportV2Dialogs";
import { ImportV2Header } from "./ImportV2Header";
import type { ImportItemAction } from "./importStatusPresentation";
import type { ImportWorkflow } from "./useImportWorkflow";
import type { ImportCandidateDiffIntent } from "./ImportCandidateDiffDialog";

const EMPTY_CAPABILITIES: AiCapabilitiesWorkflow = { agents: [], providers: [], refreshing: false, refresh: async () => undefined };

export interface ImportViewProps {
  workflow: ImportWorkflow;
  capabilities?: AiCapabilitiesWorkflow;
}

export function buildCandidateSelectionRequest(view: AgentCandidateView, intent: ImportCandidateDiffIntent) {
  const usesExplicitMarkdown = view.diff.needsThreeWayMerge && (intent.kind === "choose_agent" || intent.kind === "apply_merged");
  return {
    itemId: view.itemId,
    candidateId: intent.candidateId,
    mergedMarkdown: usesExplicitMarkdown ? intent.mergedMarkdown ?? view.diff.agentMarkdown : null,
    expectedCurrentWikiSha256: usesExplicitMarkdown
      ? view.diff.currentMarkdownSha256 ?? null
      : null,
  };
}

function itemById(items: readonly ImportItem[], itemId: string | null): ImportItem | null {
  return itemId ? items.find((item) => item.itemId === itemId) ?? null : null;
}

export function ImportView({ workflow, capabilities = EMPTY_CAPABILITIES }: ImportViewProps) {
  const { t } = useTranslation();
  const taskList = useTaskStore((state) => state.tasks);
  const openTaskDrawer = useTaskStore((state) => state.openDrawer);
  const pushToast = useToastStore((state) => state.pushToast);
  const session = workflow.session;
  const [migrationOpen, setMigrationOpen] = useState(false);
  const [privateItemId, setPrivateItemId] = useState<string | null>(null);
  const [isCancellingDiscovery, setIsCancellingDiscovery] = useState(false);
  const [pendingActionItemIds, setPendingActionItemIds] = useState<ReadonlySet<string>>(new Set());
  const pendingActionItemIdsRef = useRef(new Set<string>());
  const [candidateView, setCandidateView] = useState<AgentCandidateView | null>(null);
  const [history, setHistory] = useState<ImportHistoryPage | null>(null);
  const [historyError, setHistoryError] = useState(false);
  const [historyLoadingMore, setHistoryLoadingMore] = useState(false);
  const [openingHistoryEntryId, setOpeningHistoryEntryId] = useState<string | null>(null);
  const historyEntryBusyRef = useRef<string | null>(null);
  const [historyDetail, setHistoryDetail] = useState<{ entry: ImportHistoryEntry; session: ImportSession } | null>(null);
  const [historyResultUnavailable, setHistoryResultUnavailable] = useState(false);
  const [historyPreviewIdentity, setHistoryPreviewIdentity] = useState<ImportPreviewIdentity | null>(null);
  const historyLoadLock = useRef(false);
  const historyRequestRef = useRef(0);
  const confirmingRef = useRef(false);
  const [conflictAction, setConflictAction] = useState<Exclude<CommitConflictAction, "apply_merged_candidate">>("create_new");
  const [itemConflictActions, setItemConflictActions] = useState<Record<string, { action: CommitConflictAction; expectedWikiHash: string | null }>>({});
  const sourcePlatforms = useMemo(() => {
    const labels: Record<string, string> = {
      http: t("importV2.platform.http"),
      wechat: t("importV2.platform.wechat"),
      zhihu: t("importV2.platform.zhihu"),
      bilibili: t("importV2.platform.bilibili"),
      xiaohongshu: t("importV2.platform.xiaohongshu"),
      x: t("importV2.platform.x"),
    };
    if (workflow.readiness?.platforms) {
      return workflow.readiness.platforms.map((platform) => ({
        label: labels[platform.id] ?? platform.id,
        available: platform.available,
        reasonCode: platform.reasonCode,
      }));
    }
    return Object.keys(labels).map((id) => ({
      label: labels[id],
      available: false,
      reasonCode: "status_unknown",
    }));
  }, [t, workflow.readiness]);

  const loadHistory = useCallback(async () => {
    const requestId = ++historyRequestRef.current;
    setHistoryError(false);
    try {
      const page = await workflow.listHistory();
      if (historyRequestRef.current === requestId) setHistory(page);
    } catch {
      if (historyRequestRef.current === requestId) {
        setHistory(null);
        setHistoryError(true);
      }
    }
  }, [workflow.listHistory]);

  useEffect(() => {
    if (workflow.bootstrapState !== "ready") {
      historyRequestRef.current += 1;
      setHistory(null);
      setHistoryError(false);
      return;
    }
    setHistory(null);
    void loadHistory();
    return () => { historyRequestRef.current += 1; };
  }, [loadHistory, workflow.bootstrapState, session?.sessionId]);

  useEffect(() => {
    if (confirmingRef.current && !workflow.isConfirming) {
      void loadHistory();
    }
    confirmingRef.current = workflow.isConfirming;
  }, [loadHistory, workflow.isConfirming]);

  useEffect(() => {
    setHistoryDetail(null);
    setHistoryPreviewIdentity(null);
    historyEntryBusyRef.current = null;
    setOpeningHistoryEntryId(null);
  }, [session?.projectId]);

  useEffect(() => {
    setItemConflictActions({});
  }, [session?.sessionId]);

  useEffect(() => {
    const status = workflow.discoveryTask?.status;
    if (!status || status === "cancelling" || status === "succeeded" || status === "failed" || status === "cancelled") {
      setIsCancellingDiscovery(false);
    }
  }, [workflow.discoveryTask?.id, workflow.discoveryTask?.status]);

  const selectedReadyCount = useMemo(() => session?.items.filter((item) => item.selected && item.status === "preview_ready").length ?? 0, [session]);
  const decisions = useMemo<CommitItemDecision[]>(() => (session?.items ?? [])
    .filter((item) => item.selected && item.status === "preview_ready")
    .map((item) => {
      const itemAction = itemConflictActions[item.itemId];
      return {
        itemId: item.itemId,
        conflictAction: itemAction?.action ?? conflictAction,
        expectedWikiHash: itemAction?.expectedWikiHash ?? null,
      };
    }), [conflictAction, itemConflictActions, session]);

  async function compareCandidate(itemId: string) {
    const item = itemById(session?.items ?? [], itemId);
    if (!item?.taskId) return;
    const view = await workflow.acceptAgentCandidate(itemId, item.taskId);
    if (view) setCandidateView(view);
  }

  async function discardCandidate(itemId: string) {
    const view = candidateView?.itemId === itemId
      ? candidateView
      : await (async () => {
        const item = itemById(session?.items ?? [], itemId);
        return item?.taskId ? workflow.acceptAgentCandidate(itemId, item.taskId) : null;
      })();
    if (!view) return;
    await workflow.discardAgentCandidate(itemId, view.candidate.candidateId);
    setCandidateView(null);
    await workflow.refreshSession();
  }

  async function handleCandidateIntent(intent: ImportCandidateDiffIntent) {
    if (!candidateView) return;
    const itemId = candidateView.itemId;
    if (intent.kind === "discard" || intent.kind === "choose_deterministic" || intent.kind === "keep_current" || intent.kind === "create_new") {
      await workflow.discardAgentCandidate(itemId, intent.candidateId);
      if (intent.kind === "keep_current" || intent.kind === "create_new") {
        setItemConflictActions((current) => ({
          ...current,
          [itemId]: {
            action: intent.kind === "keep_current" ? "keep_wiki" : "create_new",
            expectedWikiHash: null,
          },
        }));
      } else {
        setItemConflictActions((current) => {
          const next = { ...current };
          delete next[itemId];
          return next;
        });
      }
    } else {
      await workflow.selectAgentCandidate(buildCandidateSelectionRequest(candidateView, intent));
      if (intent.kind === "apply_merged") {
        setItemConflictActions((current) => ({
          ...current,
          [itemId]: {
            action: "apply_merged_candidate",
            expectedWikiHash: candidateView.diff.currentMarkdownSha256 ?? null,
          },
        }));
      } else {
        setItemConflictActions((current) => {
          const next = { ...current };
          delete next[itemId];
          return next;
        });
      }
    }
    setCandidateView(null);
    await workflow.refreshSession();
  }

  async function handleAction(action: ImportItemAction, itemId: string) {
    workflow.selectItem(itemId);
    const item = itemById(session?.items ?? [], itemId);
    if (!item) return;
    switch (action) {
      case "inspect":
        return;
      case "start":
        await workflow.startItems([itemId]);
        return;
      case "retry":
        await workflow.retryItem(itemId);
        return;
      case "retry_route":
      case "switch_route":
      case "switch_parser":
      case "enable_ocr":
        await workflow.retryItem(itemId, action);
        return;
      case "skip":
        await workflow.skipItem(itemId);
        return;
      case "authorize_local_asr":
        await workflow.authorizeLocalAsr(itemId);
        return;
      case "cancel":
        await workflow.cancelItem(itemId);
        return;
      case "preview_markdown":
        useImportStore.getState().openPreview(itemId);
        return;
      case "begin_login":
        useImportStore.getState().openLogin(itemId);
        return;
      case "authorize_private_target":
        setPrivateItemId(itemId);
        return;
      case "view_capability":
        useImportStore.getState().openCapability(itemId);
        return;
      case "invoke_local_agent": {
        const agent = capabilities.agents.find((candidate) => candidate.state === "installed" && candidate.isDefault) ?? capabilities.agents.find((candidate) => candidate.state === "installed");
        if (agent) await workflow.invokeLocalAgent(itemId, "manual", agent.kind);
        return;
      }
      case "request_byok":
        useImportStore.getState().openByok(itemId);
        return;
      case "view_log":
        if (item.taskId) useTaskStore.getState().openDrawer(item.taskId);
        return;
      case "compare_candidate":
      case "resolve_merge":
        await compareCandidate(itemId);
        return;
      case "discard_candidate":
        await discardCandidate(itemId);
        return;
      case "open_result":
        useImportStore.getState().openPreview(itemId);
        return;
    }
  }

  async function handleActionRequest(action: ImportItemAction, itemId: string) {
    if (pendingActionItemIdsRef.current.has(itemId)) return;
    pendingActionItemIdsRef.current.add(itemId);
    setPendingActionItemIds(new Set(pendingActionItemIdsRef.current));
    try {
      await handleAction(action, itemId);
    } finally {
      pendingActionItemIdsRef.current.delete(itemId);
      setPendingActionItemIds(new Set(pendingActionItemIdsRef.current));
    }
  }

  async function loadMoreHistory(cursor: string) {
    if (historyLoadLock.current) return;
    const requestId = historyRequestRef.current;
    historyLoadLock.current = true;
    setHistoryLoadingMore(true);
    try {
      const next = await workflow.listHistory(cursor);
      if (!next || requestId !== historyRequestRef.current) return;
      setHistory((current) => {
        if (!current) return next;
        const entries = new Map(current.entries.map((entry) => [entry.id, entry]));
        next.entries.forEach((entry) => entries.set(entry.id, entry));
        const legacyReadOnly = new Map(current.legacyReadOnly.map((entry) => [entry.id, entry]));
        next.legacyReadOnly.forEach((entry) => legacyReadOnly.set(entry.id, entry));
        const warnings = new Map(current.warnings.concat(next.warnings).map((warning) => [`${warning.code}:${warning.evidencePath}`, warning]));
        return { ...next, entries: [...entries.values()], legacyReadOnly: [...legacyReadOnly.values()], warnings: [...warnings.values()] };
      });
    } catch {
      pushToast("error", t("importV2.history.error"));
    } finally {
      historyLoadLock.current = false;
      setHistoryLoadingMore(false);
    }
  }

  async function openHistoryEntry(entryId: string, action: ImportHistoryAction) {
    if (historyEntryBusyRef.current) return;
    const entry = history?.entries.find((candidate) => candidate.id === entryId);
    if (!entry?.sessionId) return;
    historyEntryBusyRef.current = entryId;
    setOpeningHistoryEntryId(entryId);
    setHistoryResultUnavailable(false);
    try {
      const historicalSession = await workflow.loadSession(entry.sessionId, entry.batchId);
      if (!historicalSession) {
        pushToast("info", t("importV2.history.resultUnavailable"));
        return;
      }
      if (action === "view_logs") {
        const currentTasks = useTaskStore.getState().tasks;
        const taskId = [entry.taskId, ...entry.itemIds
          .map((itemId) => historicalSession.items.find((item) => item.itemId === itemId)?.taskId)
        ].find((candidate): candidate is string => Boolean(candidate) && currentTasks.some((task) => task.id === candidate));
        if (taskId) {
          openTaskDrawer(taskId);
        } else {
          pushToast("info", t("importV2.history.logsUnavailable"));
        }
        return;
      }
      if (action === "open_result") {
        const previewItem = canOpenHistoricalResult(entry)
          ? historicalSession.items.find((item) => entry.itemIds.includes(item.itemId) && item.status === "completed" && item.preview)
          : undefined;
        if (previewItem) {
          setHistoryPreviewIdentity({ sessionId: historicalSession.sessionId, itemId: previewItem.itemId, candidateId: null, historyBatchId: entry.batchId });
          return;
        }
        pushToast("info", t("importV2.history.resultUnavailable"));
        setHistoryResultUnavailable(true);
      }
      setHistoryDetail({ entry, session: historicalSession });
    } finally {
      historyEntryBusyRef.current = null;
      setOpeningHistoryEntryId(null);
    }
  }

  const privateItem = itemById(session?.items ?? [], privateItemId);
  const blocked = workflow.bootstrapState === "blocked" || workflow.bootstrapState === "error";
  const discoveryActive = workflow.discoveryTask?.status === "queued" || workflow.discoveryTask?.status === "running" || workflow.discoveryTask?.status === "cancelling";
  // Migration is read-only metadata reconciliation. All current imports use
  // V2, so an inactive/unknown migration record must not disable V2 commits.
  const writesBlocked = blocked;
  const pendingItemIds = useMemo(() => new Set([...(workflow.pendingItemIds ?? []), ...pendingActionItemIds]), [pendingActionItemIds, workflow.pendingItemIds]);

  if (workflow.bootstrapState === "loading") {
    return <div className="import-v2-layout"><ImportV2Header session={null} progress={workflow.progress} discoveryTask={workflow.discoveryTask} syncing={workflow.isSyncingSession} /><div role="status" className="import-v2-state">{t("importV2.state.loading")}</div></div>;
  }

  return (
    <div className="import-v2-layout">
      <ImportV2Header session={session} progress={workflow.progress} discoveryTask={workflow.discoveryTask} syncing={workflow.isSyncingSession} />
      <div className="import-v2-scroll app-pane-scrollbar">
        <ImportMigrationNotice readiness={workflow.readiness} unavailable={Boolean(workflow.readinessWarning)} onOpenMigration={() => setMigrationOpen(true)} />
        {blocked ? (
          <div role="alert" className="import-v2-state import-v2-state--blocked">
            <strong>{workflow.bootstrapState === "error" ? t("importV2.state.error") : t("importV2.state.blocked")}</strong>
            {workflow.bootstrapState === "error" && workflow.bootstrapError ? <p className="m-0 mt-2 text-[11px] text-[var(--text-secondary)]">{workflow.bootstrapError}</p> : null}
            {workflow.bootstrapState === "error" && workflow.retryBootstrap ? <button type="button" className="btn btn--sm mt-3" onClick={workflow.retryBootstrap}>{t("importV2.state.retry")}</button> : null}
          </div>
        ) : (
          <>
            <ImportSourceMethods onAddPaths={workflow.addPaths} onAddUrl={workflow.addUrl} addingPaths={workflow.isAddingPaths} addingUrl={Boolean(workflow.isAddingUrl) || discoveryActive} sessionSyncing={workflow.isSyncingSession} platforms={sourcePlatforms} />
            <ImportDiscoveryStatus
              task={workflow.discoveryTask ?? null}
              scan={workflow.discoveryScan}
              unavailable={workflow.discoveryTaskUnavailable}
              cancelling={isCancellingDiscovery}
              onCancel={() => {
                if (isCancellingDiscovery) return;
                const cancel = workflow.cancelDiscovery;
                if (!cancel) return;
                setIsCancellingDiscovery(true);
                void cancel().catch(() => setIsCancellingDiscovery(false));
              }}
              onDismiss={() => workflow.dismissDiscovery?.()}
            />
            {(workflow.batches ?? (workflow.batch ? [workflow.batch] : [])).map((batch) => (
              <ImportBatchStatus
                key={batch.id}
                batch={batch}
                isCancelling={workflow.isBatchCancelling?.(batch.id) ?? false}
                onCancel={(batchId) => {
                  const cancel = workflow.cancelBatch;
                  if (cancel) void cancel(batchId).catch(() => undefined);
                }}
                onRetryFailed={(batchId) => {
                  const retry = workflow.retryBatch;
                  if (retry) void retry(batchId).catch(() => undefined);
                }}
                onDismiss={(batchId) => workflow.dismissBatch?.(batchId)}
                onViewTask={(taskId) => {
                  if (taskList.some((task) => task.id === taskId)) {
                    openTaskDrawer(taskId);
                  } else {
                    pushToast("info", t("importV2.batch.taskUnavailable"));
                  }
                }}
              />
            ))}
            <ImportQueue
              items={workflow.visibleItems}
              counts={workflow.counts}
              progress={workflow.progress}
              selectedItemId={workflow.selectedItemId}
              filter={workflow.filter}
              onFilterChange={workflow.setFilter}
              onSelectItem={workflow.selectItem}
              onSetItemSelected={(itemId, selected) => { void workflow.setItemSelected(itemId, selected); }}
              pendingItemIds={pendingItemIds}
              onCopyLocator={workflow.requestClipboard}
              sessionSyncing={workflow.isSyncingSession}
              discoveryTask={workflow.discoveryTask}
              resetKey={session?.sessionId}
              onAction={(action, itemId) => { void handleActionRequest(action, itemId).catch(() => undefined); }}
            />
            <ImportHistoryPanel
              page={history}
              loading={workflow.bootstrapState === "ready" && history === null && !historyError}
              error={historyError}
              onRetry={() => { void loadHistory(); }}
              loadingMore={historyLoadingMore}
              onLoadMore={(cursor) => { void loadMoreHistory(cursor); }}
              openingEntryId={openingHistoryEntryId}
              onOpenEntry={(entryId, action) => { void openHistoryEntry(entryId, action); }}
            />
          </>
        )}
      </div>
      <ImportCommitBar selectedReadyCount={selectedReadyCount} unresolvedActionCount={workflow.counts.needsAction} isConfirming={workflow.isConfirming} disabled={writesBlocked} conflictAction={conflictAction} onConflictActionChange={setConflictAction} onConfirm={() => { void workflow.confirm(decisions); }} />
      <ImportV2Dialogs
        workflow={workflow}
        capabilities={capabilities}
        readiness={workflow.readiness}
        privateItem={privateItem}
        migrationOpen={migrationOpen}
        onCloseMigration={() => setMigrationOpen(false)}
        candidateView={candidateView}
        onCloseCandidate={() => setCandidateView(null)}
        onCandidateIntent={(intent) => { void handleCandidateIntent(intent); }}
        onClosePrivate={() => setPrivateItemId(null)}
      />
      <ImportHistoryDetailDialog
        open={Boolean(historyDetail)}
        entry={historyDetail?.entry ?? null}
        session={historyDetail?.session ?? null}
        resultUnavailable={historyResultUnavailable}
        onClose={() => {
          setHistoryDetail(null);
          setHistoryResultUnavailable(false);
        }}
        onPreview={(itemId) => {
          if (!historyDetail) return;
          setHistoryDetail(null);
          setHistoryResultUnavailable(false);
          setHistoryPreviewIdentity({ sessionId: historyDetail.session.sessionId, itemId, candidateId: null, historyBatchId: historyDetail.entry.batchId });
        }}
        canViewLogs={(taskId) => taskList.some((task) => task.id === taskId)}
        onViewLogs={(taskId) => {
          setHistoryDetail(null);
          openTaskDrawer(taskId);
        }}
      />
      <ImportMarkdownPreviewDialog
        open={Boolean(historyPreviewIdentity)}
        identity={historyPreviewIdentity}
        loadContent={workflow.loadPreview}
        onClose={() => setHistoryPreviewIdentity(null)}
      />
    </div>
  );
}
