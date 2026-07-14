import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";

import type { AiCapabilitiesWorkflow } from "../../hooks/useAiCapabilities";
import { useImportStore } from "../../stores/importStore";
import type { AgentCandidateView } from "../../types/importV2Agent";
import type { CommitItemDecision, ImportItem } from "../../types/importV2";
import type { ImportHistoryPage } from "../../types/importV2Presentation";
import { ImportCommitBar } from "./ImportCommitBar";
import { ImportHistoryPanel } from "./ImportHistoryPanel";
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

function itemById(items: readonly ImportItem[], itemId: string | null): ImportItem | null {
  return itemId ? items.find((item) => item.itemId === itemId) ?? null : null;
}

export function ImportView({ workflow, capabilities = EMPTY_CAPABILITIES }: ImportViewProps) {
  const { t } = useTranslation();
  const session = workflow.session;
  const selectedItem = itemById(session?.items ?? [], workflow.selectedItemId);
  const [migrationOpen, setMigrationOpen] = useState(false);
  const [privateItemId, setPrivateItemId] = useState<string | null>(null);
  const [candidateView, setCandidateView] = useState<AgentCandidateView | null>(null);
  const [history, setHistory] = useState<ImportHistoryPage | null>(null);

  useEffect(() => {
    if (workflow.bootstrapState !== "ready") {
      setHistory(null);
      return;
    }
    let current = true;
    void workflow.listHistory().then((page) => {
      if (current) setHistory(page);
    }).catch(() => undefined);
    return () => { current = false; };
  }, [workflow.bootstrapState, session?.sessionId, workflow.listHistory]);

  const selectedReadyCount = useMemo(() => session?.items.filter((item) => item.selected && item.status === "preview_ready").length ?? 0, [session]);
  const decisions = useMemo<CommitItemDecision[]>(() => (session?.items ?? [])
    .filter((item) => item.selected && item.status === "preview_ready")
    .map((item) => ({ itemId: item.itemId, conflictAction: null, expectedWikiHash: null })), [session]);

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
    if (intent.kind === "discard") {
      await workflow.discardAgentCandidate(candidateView.itemId, intent.candidateId);
    } else {
      const markdown = intent.kind === "choose_agent" || intent.kind === "apply_merged" ? candidateView.diff.agentMarkdown : null;
      await workflow.selectAgentCandidate({ itemId: candidateView.itemId, candidateId: intent.candidateId, mergedMarkdown: markdown, expectedCurrentWikiSha256: null });
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

  async function loadMoreHistory(cursor: string) {
    const next = await workflow.listHistory(cursor);
    if (!next) return;
    setHistory((current) => current ? { ...next, entries: [...current.entries, ...next.entries], legacyReadOnly: [...current.legacyReadOnly, ...next.legacyReadOnly] } : next);
  }

  const privateItem = itemById(session?.items ?? [], privateItemId);
  const blocked = workflow.bootstrapState === "blocked" || workflow.bootstrapState === "error";
  // Migration is read-only metadata reconciliation. All current imports use
  // V2, so an inactive/unknown migration record must not disable V2 commits.
  const writesBlocked = blocked;

  if (workflow.bootstrapState === "loading") {
    return <div className="import-v2-layout"><ImportV2Header session={null} /><div role="status" className="import-v2-state">{t("importV2.state.loading")}</div></div>;
  }

  return (
    <div className="import-v2-layout">
      <ImportV2Header session={session} />
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
            <ImportSourceMethods onAddPaths={workflow.addPaths} onAddUrl={workflow.addUrl} />
            <ImportQueue
              items={workflow.visibleItems}
              counts={workflow.counts}
              progress={workflow.progress}
              selectedItemId={workflow.selectedItemId}
              filter={workflow.filter}
              onFilterChange={workflow.setFilter}
              onSelectItem={workflow.selectItem}
              onSetItemSelected={(itemId, selected) => { void workflow.setItemSelected(itemId, selected); }}
              onAction={(action, itemId) => { void handleAction(action, itemId); }}
            />
            <ImportHistoryPanel page={history} onLoadMore={(cursor) => { void loadMoreHistory(cursor); }} />
          </>
        )}
      </div>
      <ImportCommitBar selectedReadyCount={selectedReadyCount} unresolvedActionCount={workflow.counts.needsAction} isConfirming={workflow.isConfirming} disabled={writesBlocked} onConfirm={() => { void workflow.confirm(decisions); }} />
      <ImportV2Dialogs
        workflow={workflow}
        capabilities={capabilities}
        readiness={workflow.readiness}
        selectedItem={selectedItem}
        privateItem={privateItem}
        migrationOpen={migrationOpen}
        onCloseMigration={() => setMigrationOpen(false)}
        candidateView={candidateView}
        onCloseCandidate={() => setCandidateView(null)}
        onCandidateIntent={(intent) => { void handleCandidateIntent(intent); }}
        onClosePrivate={() => setPrivateItemId(null)}
        onCompareCandidate={(itemId) => { void compareCandidate(itemId); }}
        onDiscardCandidate={(itemId) => { void discardCandidate(itemId); }}
      />
    </div>
  );
}
