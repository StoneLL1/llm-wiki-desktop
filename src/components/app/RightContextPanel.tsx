import { lazy, Suspense, useState } from "react";
import { useTranslation } from "react-i18next";
import { GraphInspector } from "../../features/graph/GraphInspector";
import { ImportRightPanel } from "../../features/import/ImportRightPanel";
import { WorkflowsRightPanel } from "../../features/workflows/WorkflowsRightPanel";
import { RelatedPagesPanel } from "../../features/wiki/RelatedPagesPanel";
import { SourceRightPanel } from "../../features/wiki/SourceRightPanel";
import { useWikiStore } from "../../features/wiki/wikiStore";
import { latestAssistantMessage, useChatStore } from "../../stores/chatStore";
import { useNavigationStore } from "../../stores/navigationStore";
import { useGraphStore } from "../../stores/graphStore";
import { useProjectStore } from "../../stores/projectStore";
import { useTaskStore } from "../../stores/taskStore";
import { useImportStore } from "../../stores/importStore";
import { useProjectStatus } from "../../hooks/useProjectStatus";
import type { GraphState, IndexState } from "../../types/project";
import { RightPanelHeader } from "./RightPanelHeader";
import { ViewErrorBoundary } from "./ViewErrorBoundary";
import { ViewFallback } from "./ViewFallback";

// PageChatPanel imports ChatView's MessageBubble/StreamingBubble, which render
// MessageContent (react-markdown + remark/rehype + katex + highlight). It is
// only mounted in the Wiki "ask AI" assistant mode, so defer the whole chat
// rendering chain out of the first-screen graph.
const PageChatPanel = lazy(() =>
  import("../../features/chat/PageChatPanel").then((m) => ({ default: m.PageChatPanel })),
);

export function RightContextPanel() {
  const { t } = useTranslation();
  const activeView = useNavigationStore((state) => state.activeView);
  const rightPanelMode = useNavigationStore((state) => state.rightPanelMode);
  const setActiveView = useNavigationStore((state) => state.setActiveView);
  const requestWorkflowLaunch = useNavigationStore((state) => state.requestWorkflowLaunch);
  const closeWikiAssistant = useNavigationStore((state) => state.closeWikiAssistant);
  const currentProject = useProjectStore((state) => state.currentProject);
  const pendingAction = useProjectStore((state) => state.pendingAction);
  const tasks = useTaskStore((state) => state.tasks);
  const importSession = useImportStore((state) => state.session);
  const importSelectedItemId = useImportStore((state) => state.selectedItemId);
  const wikiContent = useWikiStore((state) => state.page);
  const wikiPage = wikiContent?.meta ?? null;
  const wikiTree = useWikiStore((state) => state.tree);
  const openWikiPage = useWikiStore((state) => state.openPage);
  const graphData = useGraphStore((state) => state.data);
  const graphStatus = useGraphStore((state) => state.status);
  const graphCached = useGraphStore((state) => state.cached);
  const graphLayoutStale = useGraphStore((state) => state.layoutStale);
  const graphSelectedId = useGraphStore((state) => state.selectedNodeId);
  const graphFocusedId = useGraphStore((state) => state.focusedNodeId);
  const graphSearch = useGraphStore((state) => state.search);
  const setGraphSelectedNode = useGraphStore((state) => state.setSelectedNode);
  const setGraphFocusedNode = useGraphStore((state) => state.setFocusedNodeId);
  const graphTypeFilter = useGraphStore((state) => state.typeFilter);
  const graphDegreeThreshold = useGraphStore((state) => state.degreeThreshold);
  const toggleGraphType = useGraphStore((state) => state.toggleTypeFilter);
  const setGraphDegreeThreshold = useGraphStore((state) => state.setDegreeThreshold);
  const graphExportPng = useGraphStore((state) => state.exportPng);
  const graphRecomputeLayout = useGraphStore((state) => state.recomputeLayout);
  const chatSession = useChatStore((state) => state.activeSession);
  const chatLoadingSession = useChatStore((state) => state.loadingSession);
  const chatSendTaskId = useChatStore((state) => state.sendTaskId);
  const chatSendSessionId = useChatStore((state) => state.sendSessionId);
  const chatSendStarting = useChatStore((state) => state.sendStarting);
  // Subscribe (don't getState) so the save button re-renders when the
  // per-message saveStatus map or the saveAnswer action identity changes.
  const chatSaveStatus = useChatStore((state) => state.saveStatus);
  const chatSaveInFlightMessageId = useChatStore((state) => state.saveInFlightMessageId);
  const chatConvenienceMutationKey = useChatStore((state) => state.convenienceMutationKey);
  const chatSaveAnswer = useChatStore((state) => state.saveAnswer);
  const chatOverwriteRequest = useChatStore((state) => state.overwriteRequest);
  const [chatCopied, setChatCopied] = useState(false);

  const status = useProjectStatus(currentProject.projectId, currentProject.rootPath);

  if (activeView === "workflows") {
    return <WorkflowsRightPanel />;
  }

  const openPage = (path: string) => {
    void openWikiPage(currentProject.projectId, currentProject.rootPath, path);
  };

  const viewWikiPageInGraph = () => {
    if (!wikiPage) return;
    const node = graphData?.nodes.find((candidate) => candidate.path === wikiPage.path);
    setGraphSelectedNode(node?.id ?? wikiPage.path);
    setActiveView("graph");
  };

  if (activeView === "chat") {
    const chatSendTask = chatSendTaskId
      ? tasks.find((task) => task.id === chatSendTaskId) ?? null
      : null;
    const chatGenerating = chatLoadingSession || chatSendStarting || Boolean(
      chatSession?.id &&
        chatSendSessionId === chatSession.id &&
        chatSendTask &&
        (chatSendTask.status === "queued" ||
          chatSendTask.status === "running" ||
          chatSendTask.status === "cancelling"),
    );
    // Do not present the previous answer's citations as if they belonged to
    // the answer currently streaming. The transcript shows the live delta;
    // this panel waits for the terminal reload before exposing new metadata.
    const latestAssistant = chatGenerating ? null : latestAssistantMessage(chatSession);
    const citations = latestAssistant?.citations ?? [];
    const diagnostics = latestAssistant?.retrievalDiagnostics ?? null;
    const route = latestAssistant?.route ?? null;
    const provider = latestAssistant?.provider ?? null;
    const wikiCount = currentProject.wikiPageCount;
    const saveStatus = latestAssistant
      ? (chatSaveStatus[latestAssistant.id] ?? "idle")
      : "idle";
    const saveAnswer = chatSaveAnswer;
    const { projectId, rootPath } = currentProject;

    const handleSave = () => {
      if (!chatSession?.id || !latestAssistant) return;
      void saveAnswer(projectId, rootPath, chatSession.id, latestAssistant.id);
    };

    const handleCopyMarkdown = () => {
      if (!latestAssistant || !navigator.clipboard) return;
      void navigator.clipboard
        .writeText(latestAssistant.content)
        .then(() => {
          setChatCopied(true);
          window.setTimeout(() => setChatCopied(false), 1600);
        })
        .catch(() => setChatCopied(false));
    };

    const providerLabel = provider ? t(`provider.name.${provider}`) : null;
    const routeLabel = route
      ? route === "agent"
        ? t("chat.composer.route.agent")
        : providerLabel
          ? t("rightpanel.route.byokLabel", { provider: providerLabel })
          : t("chat.composer.route.byok")
      : null;

    return (
      <aside
        id="right-context-panel"
        aria-label={t("chat.citations.title")}
        className="right-panel"
      >
        <RightPanelHeader title={t("chat.citations.title")} />
        <div className="app-pane-scrollbar min-h-0 flex-1 overflow-y-auto">
          <div className="px-4 py-3">
            {/* 引用与来源 */}
            <div className="border-b border-[var(--border-subtle)] py-3">
              <h4 className="mb-2 text-[11px] font-semibold uppercase tracking-[0.06em] text-[var(--text-muted)]">
                {t("chat.citations.title")}
                {citations.length > 0 && (
                  <span className="ml-1 font-normal normal-case text-[var(--text-muted)]">{citations.length}</span>
                )}
              </h4>
              {citations.length === 0 ? (
                <p className="m-0 text-[11px] text-[var(--text-muted)]">
                  {chatGenerating ? t("chat.citations.updating") : t("chat.citations.empty")}
                </p>
              ) : (
                <div className="flex flex-col gap-1.5">
                  {citations.map((citation, index) => (
                    <button
                      key={citation.sourceId ?? citation.pagePath}
                      type="button"
                      onClick={() => {
                        setActiveView("wiki");
                        void openWikiPage(projectId, rootPath, citation.pagePath);
                      }}
                      className="flex w-full items-center gap-2 rounded-[var(--radius-sm)] border border-[var(--border-subtle)] p-2 text-left hover:bg-[var(--surface-muted)]"
                    >
                      <span className="flex h-[18px] w-[18px] items-center justify-center rounded-full bg-[var(--accent-soft)] text-[10.5px] font-semibold text-[var(--accent-hover)] font-mono shrink-0">
                        {citation.sourceId ?? `S${index + 1}`}
                      </span>
                      <div className="min-w-0 flex-1">
                        <div className="flex min-w-0 items-center gap-1.5">
                          <span className="truncate text-[12px] font-medium text-[var(--text-primary)]">{citation.title}</span>
                          {citation.isPinned ? (
                            <span className="shrink-0 rounded-[var(--radius-sm)] bg-[var(--accent-soft)] px-1.5 py-0.5 text-[10px] font-medium text-[var(--accent-hover)]">
                              {t("chat.citations.currentPage")}
                            </span>
                          ) : null}
                        </div>
                        <div className="text-[10.5px] text-[var(--text-muted)] font-mono">{citation.pagePath}</div>
                      </div>
                    </button>
                  ))}
                </div>
              )}
              {diagnostics && (diagnostics.invalidCitationIds?.length || diagnostics.hasUnverified) ? (
                <div
                  className="mt-2 flex flex-col gap-1 rounded-[var(--radius-sm)] border border-[var(--warning)] bg-[var(--warning-soft)] px-2.5 py-2 text-[11px] text-[var(--text-secondary)]"
                  role="status"
                >
                  {diagnostics.invalidCitationIds?.length ? (
                    <span>
                      {t("chat.trust.invalidCitations", {
                        ids: diagnostics.invalidCitationIds.join(", "),
                      })}
                    </span>
                  ) : null}
                  {diagnostics.hasUnverified ? <span>{t("chat.trust.unverified")}</span> : null}
                </div>
              ) : null}
            </div>

            {/* 执行路径 */}
            <div className="border-b border-[var(--border-subtle)] py-3">
              <h4 className="mb-2 text-[11px] font-semibold uppercase tracking-[0.06em] text-[var(--text-muted)]">
                {t("chat.citations.route")}
              </h4>
              <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-2 text-[12px]">
                {routeLabel ? (
                  <>
                    <dt className="font-medium text-[var(--text-muted)]">{t("chat.citations.routePath")}</dt>
                    <dd className="m-0 text-[var(--accent-hover)]">{routeLabel}</dd>
                  </>
                ) : null}
                <dt className="font-medium text-[var(--text-muted)]">{t("rightpanel.index.pages")}</dt>
                <dd className="m-0 font-mono text-[11.5px] text-[var(--text-primary)]">
                  {citations.length} / {wikiCount}
                </dd>
              </dl>
            </div>

            {/* 操作 */}
            <div className="py-3">
              <h4 className="mb-2 text-[11px] font-semibold uppercase tracking-[0.06em] text-[var(--text-muted)]">
                {t("chat.citations.actions")}
              </h4>
              <div className="flex flex-col gap-1.5">
                <button
                  type="button"
                  onClick={handleSave}
                  disabled={
                    !latestAssistant ||
                    saveStatus === "saving" ||
                    saveStatus === "saved" ||
                    Boolean(chatSaveInFlightMessageId || chatOverwriteRequest || chatConvenienceMutationKey)
                  }
                  className="flex h-[28px] items-center gap-2 rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface-raised)] px-3 text-[12px] hover:bg-[var(--surface-muted)] disabled:opacity-40"
                >
                  {saveStatus === "saved" ? t("chat.thread.saveDone") : t("chat.thread.saveAnswer")}
                </button>
                <button
                  type="button"
                  onClick={handleCopyMarkdown}
                  disabled={!latestAssistant}
                  className="flex h-[28px] items-center gap-2 rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface-raised)] px-3 text-[12px] hover:bg-[var(--surface-muted)] disabled:opacity-40"
                >
                  {chatCopied ? t("chat.citations.copied") : t("chat.citations.copyMd")}
                </button>
                <button
                  type="button"
                  onClick={() => setActiveView("exports")}
                  disabled={!latestAssistant}
                  className="flex h-[28px] items-center gap-2 rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface-raised)] px-3 text-[12px] hover:bg-[var(--surface-muted)] disabled:opacity-40"
                >
                  {t("chat.citations.generateCard")}
                </button>
              </div>
            </div>
          </div>
        </div>
      </aside>
    );
  }

  if (activeView === "wiki") {
    if (
      wikiPage?.pageType === "source" &&
      wikiPage.sourceBinding?.sourceId &&
      wikiPage.sourceBinding.sourceId === wikiPage.sourceId
    ) {
      return (
        <aside
          id="right-context-panel"
          aria-label={t("source.panel.title")}
          className="right-panel"
        >
          <RightPanelHeader title={t("source.panel.title")} />
          <div className="app-pane-scrollbar min-h-0 flex-1 overflow-y-auto">
            <SourceRightPanel
              projectId={currentProject.projectId}
              rootPath={currentProject.rootPath}
              sourceId={wikiPage.sourceBinding.sourceId}
              onOpenPage={openPage}
              onMutation={(path) => {
                void useWikiStore
                  .getState()
                  .reload(currentProject.projectId, currentProject.rootPath)
                  .then(() => {
                    if (path) openPage(path);
                  });
              }}
            />
          </div>
        </aside>
      );
    }

    if (rightPanelMode === "wikiAssistant") {
      return (
        <aside
          id="right-context-panel"
          aria-label={t("wiki.askAi.panelTitle")}
          className="right-panel"
        >
          <RightPanelHeader title={t("wiki.askAi.panelTitle")} />
          <div className="min-h-0 flex-1 overflow-hidden">
            <ViewErrorBoundary>
              <Suspense fallback={<ViewFallback />}>
                <PageChatPanel
                  page={wikiContent}
                  projectId={currentProject.projectId}
                  rootPath={currentProject.rootPath}
                  onShowRelatedPages={closeWikiAssistant}
                  onOpenCitation={openPage}
                />
              </Suspense>
            </ViewErrorBoundary>
          </div>
        </aside>
      );
    }

    return (
      <aside
        id="right-context-panel"
        aria-label={t("wiki.related.title")}
        className="right-panel"
      >
        <RightPanelHeader title={t("wiki.related.title")} />
        <div className="app-pane-scrollbar min-h-0 flex-1 overflow-y-auto">
          <RelatedPagesPanel
            page={wikiPage}
            pages={wikiTree?.pages ?? []}
            onOpenPage={openPage}
            onViewAllBacklinks={viewWikiPageInGraph}
            onGenerateHtml={() => {
              if (!wikiPage) return;
              requestWorkflowLaunch({
                projectId: currentProject.projectId,
                projectRootPath: currentProject.rootPath,
                kind: "generate_content",
                origin: "wiki",
                scopePreset: {
                  kind: "generate_content",
                  artifactType: "beautiful_read",
                  pagePaths: [wikiPage.path],
                  outputPath: null,
                },
              });
            }}
            onGenerateCard={() => {
              if (!wikiPage) return;
              requestWorkflowLaunch({
                projectId: currentProject.projectId,
                projectRootPath: currentProject.rootPath,
                kind: "generate_content",
                origin: "wiki",
                scopePreset: {
                  kind: "generate_content",
                  artifactType: "knowledge_card",
                  pagePaths: [wikiPage.path],
                  outputPath: null,
                },
              });
            }}
            onViewInGraph={viewWikiPageInGraph}
            onCopyWikilink={() => {
              if (wikiPage) void navigator.clipboard.writeText(`[[${wikiPage.title}]]`);
            }}
          />
        </div>
      </aside>
    );
  }

  if (activeView === "graph") {
    const selectedNode = graphData?.nodes.find((node) => node.id === graphSelectedId) ?? null;
    return (
      <aside
        id="right-context-panel"
        aria-label={t("graph.inspector.title")}
        className="right-panel"
      >
        <RightPanelHeader title={t("graph.inspector.title")} />
        <div className="app-pane-scrollbar min-h-0 flex-1 overflow-y-auto">
          {graphData ? (
            <GraphInspector
              node={selectedNode}
              data={graphData}
              typeFilter={graphTypeFilter}
              degreeThreshold={graphDegreeThreshold}
              search={graphSearch}
              focusedNodeId={graphFocusedId}
              layoutStale={graphLayoutStale}
              cached={graphCached}
              status={graphStatus}
              onOpenPage={() => {
                if (!selectedNode) return;
                setActiveView("wiki");
                void openWikiPage(
                  currentProject.projectId,
                  currentProject.rootPath,
                  selectedNode.path,
                );
              }}
              onFocusNode={setGraphFocusedNode}
              onOpenNeighbor={(nodeId) => setGraphSelectedNode(nodeId)}
              onToggleType={toggleGraphType}
              onDegreeThresholdChange={setGraphDegreeThreshold}
              onExportPng={() => graphExportPng?.()}
              onRecomputeLayout={() => graphRecomputeLayout?.()}
            />
          ) : (
            <p className="px-4 py-3 text-[12px] leading-5 text-[var(--text-muted)]">
              {t("graph.inspector.empty")}
            </p>
          )}
        </div>
      </aside>
    );
  }

  if (activeView === "import") {
    const selectedItem = importSession?.items.find((item) => item.itemId === importSelectedItemId) ?? null;
    return (
      <ImportRightPanel
        selectedItem={selectedItem}
        sessionId={importSession?.sessionId ?? null}
        projectId={currentProject.projectId}
        projectRootPath={currentProject.rootPath}
        onPreviewMarkdown={(itemId) => useImportStore.getState().openPreview(itemId)}
        onPrimaryAction={(action, itemId) =>
          useImportStore.getState().requestAction(itemId, action)}
      />
    );
  }

  const health = currentProject.health;
  const pendingCount = pendingAction ? 1 : 0;

  const gitBranch = status?.git?.branch ?? null;
  const gitHead = status?.git?.head ?? null;
  const installedAgents = (status?.agents ?? []).filter((a) => a.state === "installed");
  const configuredProviders = (status?.providers ?? []).filter(
    (provider) =>
      provider.config.enabled &&
      (provider.hasSecret || provider.config.provider === "ollama"),
  );

  return (
    <aside
      id="right-context-panel"
      aria-label={t("shell.projectInfo")}
      className="right-panel"
    >
      <RightPanelHeader title={t("shell.projectInfo")} />

      <div className="app-pane-scrollbar min-h-0 flex-1 overflow-y-auto px-4 py-3">
        {/* 路径 */}
        <div className="border-b border-[var(--border-subtle)] py-3">
          <h4 className="mb-2 text-[11px] font-semibold uppercase tracking-[0.06em] text-[var(--text-muted)]">{t("rightpanel.section.paths")}</h4>
          <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-2 text-[12px]">
            <dt className="font-medium text-[var(--text-muted)]">{t("rightpanel.path.root")}</dt>
            <dd className="m-0 truncate font-mono text-[11.5px] text-[var(--text-primary)]">{currentProject.rootPath}</dd>
            <dt className="font-medium text-[var(--text-muted)]">{t("rightpanel.path.schema")}</dt>
            <dd className="m-0 font-mono text-[11.5px]">{health.hasSchema ? "schema.md" : "—"}</dd>
            <dt className="font-medium text-[var(--text-muted)]">{t("rightpanel.path.purpose")}</dt>
            <dd className="m-0 font-mono text-[11.5px]">{health.hasPurpose ? "purpose.md" : "—"}</dd>
            <dt className="font-medium text-[var(--text-muted)]">{t("rightpanel.path.gitBranch")}</dt>
            <dd className="m-0 font-mono text-[11.5px]">{gitBranch ?? "—"}</dd>
            <dt className="font-medium text-[var(--text-muted)]">{t("rightpanel.path.gitHead")}</dt>
            <dd className="m-0 font-mono text-[11.5px]">{gitHead ?? "—"}</dd>
          </dl>
        </div>

        {/* 索引状态 */}
        <div className="border-b border-[var(--border-subtle)] py-3">
          <h4 className="mb-2 text-[11px] font-semibold uppercase tracking-[0.06em] text-[var(--text-muted)]">{t("rightpanel.section.indexState")}</h4>
          <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-2 text-[12px]">
            <dt className="font-medium text-[var(--text-muted)]">{t("rightpanel.index.pages")}</dt>
            <dd className="m-0 font-mono text-[11.5px] text-[var(--text-primary)]">{currentProject.wikiPageCount}</dd>
            <dt className="font-medium text-[var(--text-muted)]">{t("rightpanel.index.index")}</dt>
            <dd className="m-0 rounded-[var(--radius-pill)] bg-[var(--accent-soft)] px-2 py-0.5 text-[var(--accent-hover)]" style={{ display: "inline-block" }}>
              {t(`status.indexState.${currentProject.indexState as IndexState}`)}
            </dd>
            <dt className="font-medium text-[var(--text-muted)]">{t("rightpanel.index.graphCache")}</dt>
            <dd className="m-0 font-mono text-[11.5px]">{t(`dashboard.graph.state.${currentProject.graphState as GraphState}`)}</dd>
            <dt className="font-medium text-[var(--text-muted)]">{t("rightpanel.index.pending")}</dt>
            <dd className="m-0 font-mono text-[11.5px]" style={{ color: pendingCount > 0 ? "var(--warning)" : "var(--text-secondary)" }}>
              {pendingCount > 0 ? t("rightpanel.pending.count", { count: pendingCount }) : t("rightpanel.pending.none")}
            </dd>
          </dl>
        </div>

        {/* 执行路径 */}
        <div className="border-b border-[var(--border-subtle)] py-3">
          <h4 className="mb-2 text-[11px] font-semibold uppercase tracking-[0.06em] text-[var(--text-muted)]">{t("rightpanel.section.route")}</h4>
          <div className="flex flex-col gap-2 text-[12px]">
            {installedAgents.length === 0 && configuredProviders.length === 0 ? (
              <p className="m-0 text-[11px] text-[var(--text-muted)]">{t("rightpanel.route.noAgents")}</p>
            ) : (
              <>
                {installedAgents.map((agent) => (
                  <div key={agent.kind} className="flex items-center gap-2">
                    <span className="dotstatus dotstatus--ok" aria-hidden="true" />
                    <span className="font-mono">{agent.kind}</span>
                    <span className="ml-auto font-mono text-[11px] text-[var(--text-muted)]">{agent.version ?? "—"}</span>
                    {agent.isDefault ? (
                      <span className="inline-flex h-[18px] items-center rounded-[var(--radius-sm)] bg-[var(--accent-soft)] px-[7px] text-[10.5px] font-medium text-[var(--accent-hover)]">{t("rightpanel.route.default")}</span>
                    ) : null}
                  </div>
                ))}
                {configuredProviders.map((provider) => (
                  <div key={provider.config.provider} className="flex items-center gap-2">
                    <span className="dotstatus dotstatus--ok" aria-hidden="true" />
                    <span className="font-mono">{t("rightpanel.route.byokLabel", { provider: provider.config.provider })}</span>
                  </div>
                ))}
              </>
            )}
          </div>
        </div>

        {/* 背景任务 */}
        <div className="border-b border-[var(--border-subtle)] py-3">
          <h4 className="mb-2 text-[11px] font-semibold uppercase tracking-[0.06em] text-[var(--text-muted)]">{t("rightpanel.section.tasks")}</h4>
          <div className="flex flex-col gap-2 text-[12px]">
            {tasks.filter((t) => t.status === "running").length === 0 ? (
              tasks.filter((t) => t.status === "queued").length === 0 ? (
                <p className="m-0 text-[11px] text-[var(--text-muted)]">{t("rightpanel.tasks.none")}</p>
              ) : (
                tasks.filter((t) => t.status === "queued").map((task) => (
                  <div key={task.id} className="flex items-center gap-2">
                    <span className="dotstatus dotstatus--ok" aria-hidden="true" />
                    <span className="min-w-0 flex-1 truncate">{task.title}</span>
                    <span className="shrink-0 text-[var(--text-muted)]">{t(`task.status.${task.status}`)}</span>
                  </div>
                ))
              )
            ) : (
              tasks.filter((t) => t.status === "running" || t.status === "queued").slice(0, 5).map((task) => (
                <div key={task.id} className="flex items-center gap-2">
                  <span className={`dotstatus ${task.status === "running" ? "dotstatus--busy" : "dotstatus--ok"}`} aria-hidden="true" />
                  <span className="min-w-0 flex-1 truncate">{task.title}</span>
                  {task.progress != null ? (
                    <span className="shrink-0 font-mono text-[var(--text-muted)]">
                      {task.progress.total != null && task.progress.total > 0
                        ? `${Math.round((task.progress.current / task.progress.total) * 100)}%`
                        : (task.progress.label ?? task.progress.current)}
                    </span>
                  ) : (
                    <span className="shrink-0 text-[var(--text-muted)]">{t(`task.status.${task.status}`)}</span>
                  )}
                </div>
              ))
            )}
            {tasks.filter((t) => t.status === "succeeded" || t.status === "failed").length > 0 && tasks.filter((t) => t.status === "running" || t.status === "queued").length === 0 ? (
              <p className="m-0 text-[11px] text-[var(--text-muted)]">{t("rightpanel.tasks.noOthers")}</p>
            ) : null}
          </div>
        </div>

      </div>
    </aside>
  );
}
