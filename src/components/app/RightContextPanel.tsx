import { useTranslation } from "react-i18next";
import { GraphInspector } from "../../features/graph/GraphInspector";
import { ImportRightPanel } from "../../features/import/ImportRightPanel";
import { AgentRightPanel } from "../../features/agent/AgentRightPanel";
import { RelatedPagesPanel } from "../../features/wiki/RelatedPagesPanel";
import { useWikiStore } from "../../features/wiki/wikiStore";
import { latestAssistantMessage, useChatStore } from "../../stores/chatStore";
import { useNavigationStore } from "../../stores/navigationStore";
import { useGraphStore } from "../../stores/graphStore";
import { useProjectStore } from "../../stores/projectStore";
import { useTaskStore } from "../../stores/taskStore";
import { useProjectStatus } from "../../hooks/useProjectStatus";
import type { GraphState, IndexState } from "../../types/project";
import { RightPanelHeader } from "./RightPanelHeader";

export function RightContextPanel() {
  const { t } = useTranslation();
  const activeView = useNavigationStore((state) => state.activeView);
  const setActiveView = useNavigationStore((state) => state.setActiveView);
  const currentProject = useProjectStore((state) => state.currentProject);
  const pendingAction = useProjectStore((state) => state.pendingAction);
  const tasks = useTaskStore((state) => state.tasks);
  const wikiPage = useWikiStore((state) => state.page?.meta ?? null);
  const wikiTree = useWikiStore((state) => state.tree);
  const openWikiPage = useWikiStore((state) => state.openPage);
  const requestWikiExport = useWikiStore((state) => state.requestExport);
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

  const status = useProjectStatus(currentProject.projectId, currentProject.rootPath);

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
    const latestAssistant = latestAssistantMessage(chatSession);
    const citations = latestAssistant?.citations ?? [];
    const route = latestAssistant?.route ?? null;
    const provider = latestAssistant?.provider ?? null;
    const wikiCount = currentProject.wikiPageCount;
    const saveStatus = latestAssistant
      ? (useChatStore.getState().saveStatus[latestAssistant.id] ?? "idle")
      : "idle";
    const saveAnswer = useChatStore.getState().saveAnswer;
    const { projectId, rootPath } = currentProject;

    const handleSave = () => {
      if (!chatSession?.id || !latestAssistant) return;
      void saveAnswer(projectId, rootPath, chatSession.id, latestAssistant.id);
    };

    const handleCopyMarkdown = () => {
      if (!latestAssistant) return;
      void navigator.clipboard.writeText(latestAssistant.content);
    };

    const routeLabel = route
      ? route === "agent"
        ? t("chat.composer.route.agent")
        : provider
          ? `BYOK · ${provider}`
          : t("chat.composer.route.byok")
      : null;

    return (
      <aside
        id="right-context-panel"
        aria-label={t("chat.citations.title")}
        className="right-panel"
      >
        <RightPanelHeader title={t("chat.citations.title")} />
        <div className="min-h-0 flex-1 overflow-y-auto">
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
                <p className="m-0 text-[11px] text-[var(--text-muted)]">{t("chat.citations.empty")}</p>
              ) : (
                <div className="flex flex-col gap-1.5">
                  {citations.map((citation, index) => (
                    <button
                      key={citation.pagePath}
                      type="button"
                      onClick={() => {
                        setActiveView("wiki");
                        void openWikiPage(projectId, rootPath, citation.pagePath);
                      }}
                      className="flex w-full items-center gap-2 rounded-[var(--radius-sm)] border border-[var(--border-subtle)] p-2 text-left hover:bg-[var(--surface-muted)]"
                    >
                      <span className="flex h-[18px] w-[18px] items-center justify-center rounded-full bg-[var(--accent-soft)] text-[10.5px] font-semibold text-[var(--accent-hover)] font-mono shrink-0">
                        {index + 1}
                      </span>
                      <div className="min-w-0 flex-1">
                        <div className="truncate text-[12px] font-medium text-[var(--text-primary)]">{citation.title}</div>
                        <div className="text-[10.5px] text-[var(--text-muted)] font-mono">{citation.pagePath}</div>
                      </div>
                    </button>
                  ))}
                </div>
              )}
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
                  disabled={!latestAssistant || saveStatus === "saving" || saveStatus === "saved"}
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
                  {t("chat.citations.copyMd")}
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
    return (
      <aside
        id="right-context-panel"
        aria-label={t("wiki.related.title")}
        className="right-panel"
      >
        <RightPanelHeader title={t("wiki.related.title")} />
        <div className="min-h-0 flex-1 overflow-y-auto">
          <RelatedPagesPanel
            page={wikiPage}
            pages={wikiTree?.pages ?? []}
            onOpenPage={openPage}
            onViewAllBacklinks={viewWikiPageInGraph}
            onGenerateHtml={() => requestWikiExport("beautiful_read")}
            onGenerateCard={() => requestWikiExport("knowledge_card")}
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
        <div className="min-h-0 flex-1 overflow-y-auto">
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
    return <ImportRightPanel />;
  }

  if (activeView === "agent") {
    const agentsFromStatus = status?.agents ?? [];
    return (
      <AgentRightPanel
        agents={agentsFromStatus}
        onRunIngest={() => {
          // Switch into the agent view so the run dialog is reachable there.
          setActiveView("agent");
        }}
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

      <div className="min-h-0 flex-1 overflow-y-auto px-4 py-3">
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
