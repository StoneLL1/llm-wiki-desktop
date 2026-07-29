import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import type { AiCapabilitiesWorkflow } from "../../hooks/useAiCapabilities";
import { useImportStore } from "../../stores/importStore";
import { useTaskStore } from "../../stores/taskStore";
import { useToastStore } from "../../stores/toastStore";
import type { AgentCandidateView } from "../../types/importV2Agent";
import type { CommitItemDecision, ImportItem, ImportSession } from "../../types/importV2";
import {
  canOpenHistoricalResult,
  type ImportHistoryAction,
  type ImportHistoryEntry,
  type ImportHistoryPage,
  type ImportWorkbenchPreferences,
} from "../../types/importV2Presentation";
import { ImportCommitBar } from "./ImportCommitBar";
import { ImportActionGroups, type ImportActionGroup } from "./ImportActionGroups";
import { ImportCompletionSummary } from "./ImportCompletionSummary";
import { ImportCapabilitiesPanel } from "./ImportCapabilitiesPanel";
import { ImportBatchStatus } from "./ImportBatchStatus";
import { ImportDiscoveryStatus } from "./ImportDiscoveryStatus";
import { ImportHistoryPanel } from "./ImportHistoryPanel";
import { ImportHistoryDetailDialog } from "./ImportHistoryDetailDialog";
import { ImportMarkdownPreviewDialog, type ImportPreviewIdentity } from "./ImportMarkdownPreviewDialog";
import { ImportMergeResolutionDialog } from "./ImportMergeResolutionDialog";
import { ImportQueue } from "./ImportQueue";
import { ImportSourceMethods } from "./ImportSourceMethods";
import { ImportV2Dialogs } from "./ImportV2Dialogs";
import { ImportV2Header, type ImportV2Section } from "./ImportV2Header";
import { presentImportItem, type ImportItemAction } from "./importStatusPresentation";
import type { ImportWorkflow } from "./useImportWorkflow";
import type { ImportCandidateDiffIntent } from "./ImportCandidateDiffDialog";

const EMPTY_CAPABILITIES: AiCapabilitiesWorkflow = { agents: [], providers: [], refreshing: false, refresh: async () => undefined };
const DEFAULT_WORKBENCH_PREFERENCES: ImportWorkbenchPreferences = {
  schemaVersion: 1,
  activeSection: "workbench",
  queueFilter: "all",
  workbenchScrollTop: 0,
  capabilitiesScrollTop: 0,
  historyScrollTop: 0,
  sourceMethodsExpanded: true,
};

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
  const actionRequest = useImportStore((state) => state.actionRequest);
  const clearActionRequest = useImportStore((state) => state.clearActionRequest);
  const [activeSection, setActiveSection] = useState<ImportV2Section>("workbench");
  const [preferencesHydrationRevision, setPreferencesHydrationRevision] = useState(0);
  const [sourceMethodsExpanded, setSourceMethodsExpanded] = useState(true);
  const [sourceMatrixExpanded, setSourceMatrixExpanded] = useState(false);
  const [privateItemId, setPrivateItemId] = useState<string | null>(null);
  const [asrItemIds, setAsrItemIds] = useState<readonly string[]>([]);
  const [subtitleItemId, setSubtitleItemId] = useState<string | null>(null);
  const [isCancellingDiscovery, setIsCancellingDiscovery] = useState(false);
  const [pendingActionItemIds, setPendingActionItemIds] = useState<ReadonlySet<string>>(new Set());
  const pendingActionItemIdsRef = useRef(new Set<string>());
  const [candidateView, setCandidateView] = useState<AgentCandidateView | null>(null);
  const [mergeItemId, setMergeItemId] = useState<string | null>(null);
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
  const scrollRef = useRef<HTMLDivElement>(null);
  const preferencesRef = useRef<ImportWorkbenchPreferences>(DEFAULT_WORKBENCH_PREFERENCES);
  const preferencesSaveTimerRef = useRef<number | null>(null);
  const confirmingProjectKeyRef = useRef(workflow.projectKey);
  const activeProjectKeyRef = useRef(workflow.projectKey);
  activeProjectKeyRef.current = workflow.projectKey;
  const sourcePlatforms = useMemo(() => {
    const labels: Record<string, string> = {
      http: t("importV2.platform.http"),
      wechat: t("importV2.platform.wechat"),
      zhihu: t("importV2.platform.zhihu"),
      bilibili: t("importV2.platform.bilibili"),
      xiaohongshu: t("importV2.platform.xiaohongshu"),
      douyin: t("importV2.platform.douyin"),
      x: t("importV2.platform.x"),
    };
    if (workflow.readiness?.platforms) {
      return workflow.readiness.platforms.map((platform) => ({
        id: platform.id,
        label: labels[platform.id] ?? platform.id,
        available: platform.available,
        reasonCode: platform.reasonCode,
      }));
    }
    return Object.keys(labels).map((id) => ({
      id,
      label: labels[id],
      available: false,
      reasonCode: "status_unknown",
    }));
  }, [t, workflow.readiness]);
  const sourceAbilities = useMemo(() => {
    const labels: Record<string, string> = {
      subtitle: t("importV2.ability.subtitle"),
      local_asr: t("importV2.ability.localAsr"),
      ocr: t("importV2.ability.ocr"),
      keyframes: t("importV2.ability.keyframes"),
    };
    return (workflow.readiness?.abilities ?? Object.keys(labels).map((id) => ({ id, available: false, reasonCode: "status_unknown" }))).map((ability) => ({
      ...ability,
      label: labels[ability.id] ?? ability.id,
    }));
  }, [t, workflow.readiness]);

  const loadHistory = useCallback(async () => {
    const requestId = ++historyRequestRef.current;
    const requestProjectKey = workflow.projectKey;
    setHistoryError(false);
    try {
      const page = await workflow.listHistory();
      if (activeProjectKeyRef.current === requestProjectKey && historyRequestRef.current === requestId) setHistory(page);
    } catch {
      if (activeProjectKeyRef.current === requestProjectKey && historyRequestRef.current === requestId) {
        setHistory(null);
        setHistoryError(true);
      }
    }
  }, [workflow.listHistory, workflow.projectKey]);

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
  }, [loadHistory, workflow.bootstrapState, workflow.completion?.batchId, workflow.projectKey, session?.sessionId]);

  useEffect(() => {
    if (confirmingProjectKeyRef.current === workflow.projectKey && confirmingRef.current && !workflow.isConfirming) {
      void loadHistory();
    }
    confirmingProjectKeyRef.current = workflow.projectKey;
    confirmingRef.current = workflow.isConfirming;
  }, [loadHistory, workflow.isConfirming, workflow.projectKey]);

  useEffect(() => {
    const defaults = { ...DEFAULT_WORKBENCH_PREFERENCES };
    preferencesRef.current = defaults;
    setActiveSection(defaults.activeSection);
    setSourceMethodsExpanded(defaults.sourceMethodsExpanded);
    setSourceMatrixExpanded(false);
    workflow.setFilter(defaults.queueFilter);
    setPrivateItemId(null);
    setAsrItemIds([]);
    setSubtitleItemId(null);
    setCandidateView(null);
    setMergeItemId(null);
    setHistoryDetail(null);
    setHistoryPreviewIdentity(null);
    setHistoryResultUnavailable(false);
    historyLoadLock.current = false;
    setHistoryLoadingMore(false);
    historyEntryBusyRef.current = null;
    setOpeningHistoryEntryId(null);
    pendingActionItemIdsRef.current = new Set();
    setPendingActionItemIds(new Set());
    if (preferencesSaveTimerRef.current !== null) {
      window.clearTimeout(preferencesSaveTimerRef.current);
      preferencesSaveTimerRef.current = null;
    }
    let current = true;
    void workflow.loadWorkbenchPreferences?.().then((preferences) => {
      if (!current || activeProjectKeyRef.current !== workflow.projectKey) return;
      preferencesRef.current = preferences;
      setActiveSection(preferences.activeSection);
      setSourceMethodsExpanded(preferences.sourceMethodsExpanded);
      workflow.setFilter(preferences.queueFilter);
      setPreferencesHydrationRevision((revision) => revision + 1);
    }).catch(() => undefined);
    return () => {
      current = false;
      if (preferencesSaveTimerRef.current !== null) {
        window.clearTimeout(preferencesSaveTimerRef.current);
        preferencesSaveTimerRef.current = null;
        void workflow.saveWorkbenchPreferences?.(preferencesRef.current).catch(() => undefined);
      }
    };
  }, [workflow.projectKey]);

  const persistPreferences = useCallback((next: ImportWorkbenchPreferences) => {
    preferencesRef.current = next;
    if (preferencesSaveTimerRef.current !== null) {
      window.clearTimeout(preferencesSaveTimerRef.current);
    }
    preferencesSaveTimerRef.current = window.setTimeout(() => {
      preferencesSaveTimerRef.current = null;
      void workflow.saveWorkbenchPreferences?.(next).catch(() => undefined);
    }, 250);
  }, [workflow.saveWorkbenchPreferences]);

  const handleSectionChange = useCallback((section: ImportV2Section) => {
    const current = preferencesRef.current;
    const scrollTop = Math.max(0, Math.round(scrollRef.current?.scrollTop ?? 0));
    const withCurrentScroll = {
      ...current,
      ...(activeSection === "workbench" ? { workbenchScrollTop: scrollTop } : {}),
      ...(activeSection === "capabilities" ? { capabilitiesScrollTop: scrollTop } : {}),
      ...(activeSection === "history" ? { historyScrollTop: scrollTop } : {}),
      activeSection: section,
    };
    persistPreferences(withCurrentScroll);
    setActiveSection(section);
  }, [activeSection, persistPreferences]);

  useEffect(() => {
    const scrollTop = activeSection === "workbench"
      ? preferencesRef.current.workbenchScrollTop
      : activeSection === "capabilities"
        ? preferencesRef.current.capabilitiesScrollTop
        : preferencesRef.current.historyScrollTop;
    const frame = window.requestAnimationFrame(() => {
      if (scrollRef.current) scrollRef.current.scrollTop = scrollTop;
    });
    return () => window.cancelAnimationFrame(frame);
  }, [activeSection, preferencesHydrationRevision, workflow.projectKey]);

  useEffect(() => {
    const status = workflow.discoveryTask?.status;
    if (!status || status === "cancelling" || status === "succeeded" || status === "failed" || status === "cancelled") {
      setIsCancellingDiscovery(false);
    }
  }, [workflow.discoveryTask?.id, workflow.discoveryTask?.status]);

  const commitCounts = useMemo(() => {
    const selected = (session?.items ?? []).filter(
      (item) => item.selected && presentImportItem(item).committable,
    );
    return {
      selected: selected.length,
      newSources: selected.filter(
        (item) => !item.preview?.resolution || item.preview.resolution.kind === "new_source",
      ).length,
      updates: selected.filter(
        (item) =>
          item.preview?.resolution?.kind === "same_source_new_version"
          || (
            item.preview?.resolution?.kind === "needs_three_way_merge"
            && Boolean(item.preview.resolution.defaultResolution)
          ),
      ).length,
      warnings: selected.filter((item) => item.preview?.quality.level === "warning").length,
      pending: (session?.items ?? []).filter(
        (item) => {
          const userState = presentImportItem(item).userState;
          return userState === "needs_action" || userState === "failed";
        },
      ).length,
    };
  }, [session]);
  const decisions = useMemo<CommitItemDecision[]>(() => (session?.items ?? [])
    .filter((item) => item.selected && presentImportItem(item).committable)
    .map((item) => ({
      itemId: item.itemId,
      resolution: item.preview?.resolution?.defaultResolution ?? null,
    })), [session]);

  async function compareCandidate(itemId: string) {
    const requestProjectKey = workflow.projectKey;
    const item = itemById(session?.items ?? [], itemId);
    if (!item?.taskId) return;
    const view = await workflow.acceptAgentCandidate(itemId, item.taskId);
    if (view && activeProjectKeyRef.current === requestProjectKey) setCandidateView(view);
  }

  async function discardCandidate(itemId: string) {
    const requestProjectKey = workflow.projectKey;
    const view = candidateView?.itemId === itemId
      ? candidateView
      : await (async () => {
        const item = itemById(session?.items ?? [], itemId);
        return item?.taskId ? workflow.acceptAgentCandidate(itemId, item.taskId) : null;
      })();
    if (!view || activeProjectKeyRef.current !== requestProjectKey) return;
    await workflow.discardAgentCandidate(itemId, view.candidate.candidateId);
    if (activeProjectKeyRef.current !== requestProjectKey) return;
    setCandidateView(null);
    await workflow.refreshSession();
  }

  async function handleCandidateIntent(intent: ImportCandidateDiffIntent) {
    const requestProjectKey = workflow.projectKey;
    const view = candidateView;
    if (!view) return;
    const itemId = view.itemId;
    if (intent.kind === "discard" || intent.kind === "choose_deterministic" || intent.kind === "keep_current" || intent.kind === "create_new") {
      await workflow.discardAgentCandidate(itemId, intent.candidateId);
      if (activeProjectKeyRef.current !== requestProjectKey) return;
    } else {
      await workflow.selectAgentCandidate(buildCandidateSelectionRequest(view, intent));
      if (activeProjectKeyRef.current !== requestProjectKey) return;
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
        await workflow.retryItem(itemId, action);
        return;
      case "enable_ocr":
        await workflow.authorizeLocalOcr(itemId);
        return;
      case "skip":
        await workflow.skipItem(itemId);
        return;
      case "authorize_local_asr":
        setAsrItemIds([itemId]);
        return;
      case "select_subtitle":
        setSubtitleItemId(itemId);
        return;
      case "cancel":
        await workflow.cancelItem(itemId);
        return;
      case "preview_markdown":
        useImportStore.getState().openPreview(itemId);
        return;
      case "preserve_remote_media":
        await workflow.planRemoteMediaRetention(itemId);
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
        if (!agent) {
          pushToast("warning", t("importV2.workflow.localAgentUnavailable"));
          return;
        }
        await workflow.invokeLocalAgent(itemId, "manual", agent.kind);
        return;
      }
      case "view_log":
        if (item.taskId) useTaskStore.getState().openDrawer(item.taskId);
        return;
      case "compare_candidate":
        await compareCandidate(itemId);
        return;
      case "resolve_merge":
        setMergeItemId(itemId);
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
    const requestProjectKey = workflow.projectKey;
    pendingActionItemIdsRef.current.add(itemId);
    setPendingActionItemIds(new Set(pendingActionItemIdsRef.current));
    try {
      await handleAction(action, itemId);
    } finally {
      if (activeProjectKeyRef.current === requestProjectKey) {
        pendingActionItemIdsRef.current.delete(itemId);
        setPendingActionItemIds(new Set(pendingActionItemIdsRef.current));
      }
    }
  }

  useEffect(() => {
    if (!actionRequest) return;
    clearActionRequest(actionRequest.requestId);
    void handleActionRequest(
      actionRequest.action as ImportItemAction,
      actionRequest.itemId,
    ).catch(() => undefined);
  }, [actionRequest, clearActionRequest]);

  async function handleActionGroup(group: ImportActionGroup) {
    const firstItemId = group.itemIds[0];
    if (!firstItemId) return;
    switch (group.kind) {
      case "login":
        useImportStore.getState().openLogin(firstItemId);
        return;
      case "capability":
        useImportStore.getState().openCapability(firstItemId);
        return;
      case "asr":
        setAsrItemIds(group.itemIds);
        return;
      case "ocr":
        if (workflow.authorizeLocalOcrGroup) {
          await workflow.authorizeLocalOcrGroup(group.itemIds);
        } else {
          for (const itemId of group.itemIds) await workflow.authorizeLocalOcr(itemId);
        }
        return;
      case "resume":
        await workflow.startItems(group.itemIds, "retry");
        return;
    }
  }

  async function loadMoreHistory(cursor: string) {
    if (historyLoadLock.current) return;
    const requestProjectKey = workflow.projectKey;
    const requestId = historyRequestRef.current;
    historyLoadLock.current = true;
    setHistoryLoadingMore(true);
    try {
      const next = await workflow.listHistory(cursor);
      if (!next || activeProjectKeyRef.current !== requestProjectKey || requestId !== historyRequestRef.current) return;
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
      if (activeProjectKeyRef.current === requestProjectKey) pushToast("error", t("importV2.history.error"));
    } finally {
      if (activeProjectKeyRef.current === requestProjectKey) {
        historyLoadLock.current = false;
        setHistoryLoadingMore(false);
      }
    }
  }

  async function openHistoryEntry(entryId: string, action: ImportHistoryAction) {
    if (historyEntryBusyRef.current) return;
    const requestProjectKey = workflow.projectKey;
    const entry = history?.entries.find((candidate) => candidate.id === entryId);
    if (!entry?.sessionId) return;
    historyEntryBusyRef.current = entryId;
    setOpeningHistoryEntryId(entryId);
    setHistoryResultUnavailable(false);
    try {
      const historicalSession = await workflow.loadSession(entry.sessionId, entry.batchId);
      if (activeProjectKeyRef.current !== requestProjectKey) return;
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
      if (action === "update_wiki" && entry.batchId) {
        const completion = await workflow.loadCompletion(entry.sessionId, entry.batchId);
        if (activeProjectKeyRef.current !== requestProjectKey) return;
        if (completion) {
          await workflow.updateWiki(completion);
        } else {
          pushToast("info", t("importV2.history.resultUnavailable"));
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
      if (activeProjectKeyRef.current === requestProjectKey) {
        historyEntryBusyRef.current = null;
        setOpeningHistoryEntryId(null);
      }
    }
  }

  const privateItem = itemById(session?.items ?? [], privateItemId);
  const mergeItem = itemById(session?.items ?? [], mergeItemId);
  const blocked = workflow.bootstrapState === "blocked" || workflow.bootstrapState === "error";
  const discoveryActive = workflow.discoveryTask?.status === "queued" || workflow.discoveryTask?.status === "running" || workflow.discoveryTask?.status === "cancelling";
  // Migration is read-only metadata reconciliation. All current imports use
  // V2, so an inactive/unknown migration record must not disable V2 commits.
  const writesBlocked = blocked;
  const pendingItemIds = useMemo(() => new Set([...(workflow.pendingItemIds ?? []), ...pendingActionItemIds]), [pendingActionItemIds, workflow.pendingItemIds]);

  if (workflow.bootstrapState === "loading") {
    return <div className="import-v2-layout"><ImportV2Header session={null} progress={workflow.progress} discoveryTask={workflow.discoveryTask} syncing={workflow.isSyncingSession} activeSection={activeSection} onSectionChange={handleSectionChange} /><div role="status" className="import-v2-state">{t("importV2.state.loading")}</div></div>;
  }

  return (
    <div className="import-v2-layout">
      <ImportV2Header session={session} progress={workflow.progress} discoveryTask={workflow.discoveryTask} syncing={workflow.isSyncingSession} activeSection={activeSection} onSectionChange={handleSectionChange} />
      <div
        ref={scrollRef}
        className="import-v2-scroll app-pane-scrollbar"
        onScroll={(event) => {
          const scrollTop = Math.max(0, Math.round(event.currentTarget.scrollTop));
          const current = preferencesRef.current;
          persistPreferences({
            ...current,
            ...(activeSection === "workbench" ? { workbenchScrollTop: scrollTop } : {}),
            ...(activeSection === "capabilities" ? { capabilitiesScrollTop: scrollTop } : {}),
            ...(activeSection === "history" ? { historyScrollTop: scrollTop } : {}),
          });
        }}
      >
        {blocked ? (
          <div role="alert" className="import-v2-state import-v2-state--blocked">
            <strong>{workflow.bootstrapState === "error" ? t("importV2.state.error") : t("importV2.state.blocked")}</strong>
            {workflow.bootstrapState === "error" && workflow.bootstrapError ? <p className="m-0 mt-2 text-[11px] text-[var(--text-secondary)]">{workflow.bootstrapError}</p> : null}
            {workflow.bootstrapState === "error" && workflow.retryBootstrap ? <button type="button" className="btn btn--sm mt-3" onClick={workflow.retryBootstrap}>{t("importV2.state.retry")}</button> : null}
          </div>
        ) : activeSection === "workbench" ? (
          <>
            {workflow.completion ? (
              <ImportCompletionSummary
                completion={workflow.completion}
                onViewSources={() => {
                  void workflow.viewImportedSources();
                }}
                onViewSource={(wikiPath) => {
                  void workflow.viewImportedSources(workflow.completion, wikiPath);
                }}
                onUpdateWiki={() => {
                  void workflow.updateWiki();
                }}
                onRetryFailure={(itemId) => {
                  void workflow.retryItem(itemId);
                }}
              />
            ) : null}
            <ImportSourceMethods
              onAddPaths={workflow.addPaths}
              onAddText={workflow.addText}
              onAddUrl={workflow.addUrl}
              addingPaths={workflow.isAddingPaths}
              addingText={workflow.isAddingText}
              addingUrl={Boolean(workflow.isAddingUrl) || discoveryActive}
              sessionSyncing={workflow.isSyncingSession}
              files={workflow.readiness?.files?.map((file) => ({ ...file, label: file.id.toUpperCase() }))}
              platforms={sourcePlatforms}
              abilities={sourceAbilities}
              expanded={sourceMethodsExpanded}
              onExpandedChange={(expanded) => {
                setSourceMethodsExpanded(expanded);
                persistPreferences({ ...preferencesRef.current, sourceMethodsExpanded: expanded });
              }}
              matrixExpanded={sourceMatrixExpanded}
              onMatrixExpandedChange={setSourceMatrixExpanded}
              onManageCapabilities={() => handleSectionChange("capabilities")}
            />
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
              onConfirmLargeData={(paths) => workflow.addPaths(paths, true)}
              confirmingLargeData={workflow.isAddingPaths}
            />
            <ImportActionGroups
              items={session?.items ?? []}
              pendingItemIds={pendingItemIds}
              onRun={(group) => {
                void handleActionGroup(group).catch(() => undefined);
              }}
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
              onFilterChange={(filter) => {
                const normalized = filter === "completed" ? "all" : filter;
                workflow.setFilter(normalized);
                persistPreferences({ ...preferencesRef.current, queueFilter: normalized });
              }}
              onSelectItem={workflow.selectItem}
              onSetItemSelected={(itemId, selected) => { void workflow.setItemSelected(itemId, selected); }}
              pendingItemIds={pendingItemIds}
              onCopyLocator={workflow.requestClipboard}
              sessionSyncing={workflow.isSyncingSession}
              discoveryTask={workflow.discoveryTask}
              resetKey={session?.sessionId}
              onAction={(action, itemId) => { void handleActionRequest(action, itemId).catch(() => undefined); }}
            />
          </>
        ) : activeSection === "history" ? (
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
        ) : (
          <ImportCapabilitiesPanel
            capabilities={workflow.readiness?.capabilities ?? []}
            items={session?.items ?? []}
            onAction={(action, itemId) => {
              void handleActionRequest(action, itemId).catch(() => undefined);
            }}
          />
        )}
      </div>
      {activeSection === "workbench" ? <ImportCommitBar counts={commitCounts} isConfirming={workflow.isConfirming} disabled={writesBlocked} onConfirm={() => { void workflow.confirm(decisions); }} /> : null}
      <ImportV2Dialogs
        workflow={workflow}
        privateItem={privateItem}
        asrItem={itemById(session?.items ?? [], asrItemIds[0] ?? null)}
        asrItemIds={asrItemIds}
        subtitleItem={itemById(session?.items ?? [], subtitleItemId)}
        candidateView={candidateView}
        onCloseCandidate={() => setCandidateView(null)}
        onCandidateIntent={(intent) => { void handleCandidateIntent(intent); }}
        onClosePrivate={() => setPrivateItemId(null)}
        onCloseAsr={() => setAsrItemIds([])}
        onCloseSubtitle={() => setSubtitleItemId(null)}
      />
      <ImportMergeResolutionDialog
        open={Boolean(mergeItem)}
        itemId={mergeItem?.itemId ?? null}
        title={mergeItem?.input.displayName ?? ""}
        loadContext={workflow.loadMergeContext}
        onChoose={workflow.setItemResolution}
        onSaveMerged={workflow.stageManualMerge}
        onClose={() => setMergeItemId(null)}
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
