import { useTranslation } from "react-i18next";
import { GraphInspector } from "../../features/graph/GraphInspector";
import { ImportRightPanel } from "../../features/import/ImportRightPanel";
import { RelatedPagesPanel } from "../../features/wiki/RelatedPagesPanel";
import { CitationPanel } from "../../features/chat/CitationPanel";
import { useWikiStore } from "../../features/wiki/wikiStore";
import { latestAssistantMessage, useChatStore } from "../../stores/chatStore";
import { useNavigationStore } from "../../stores/navigationStore";
import { useGraphStore } from "../../stores/graphStore";
import { useProjectStore } from "../../stores/projectStore";
import { useTaskStore } from "../../stores/taskStore";
import { useProjectStatus } from "../../hooks/useProjectStatus";
import type { GraphState, IndexState } from "../../types/project";

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
  const graphSelectedId = useGraphStore((state) => state.selectedNodeId);
  const setGraphSelectedNode = useGraphStore((state) => state.setSelectedNode);
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
    return (
      <aside
        aria-label={t("chat.citations.title")}
        className="flex w-[var(--rightpanel-w)] flex-col border-l border-[var(--border)] bg-[var(--surface)]"
      >
        <div className="flex h-[52px] items-center border-b border-[var(--border-subtle)] bg-[var(--background)] px-4">
          <span className="text-xs font-semibold uppercase tracking-[0.08em] text-[var(--text-secondary)]">
            {t("chat.citations.title")}
          </span>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto">
          <CitationPanel
            citations={citations}
            onOpenPage={(path) => {
              setActiveView("wiki");
              void openWikiPage(currentProject.projectId, currentProject.rootPath, path);
            }}
          />
        </div>
      </aside>
    );
  }

  if (activeView === "wiki") {
    return (
      <aside
        aria-label={t("wiki.related.title")}
        className="flex w-[var(--rightpanel-w)] flex-col border-l border-[var(--border)] bg-[var(--surface)]"
      >
        <div className="flex h-[52px] items-center border-b border-[var(--border-subtle)] bg-[var(--background)] px-4">
          <span className="text-xs font-semibold uppercase tracking-[0.08em] text-[var(--text-secondary)]">
            {t("wiki.related.title")}
          </span>
        </div>
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
    const neighborCount = (() => {
      // Distinct neighbor count from cached topology edges (no sigma needed).
      // Edges are deduped to undirected pairs in the backend, but count via a Set
      // so the number stays correct if a future schema allows parallel edges.
      if (!graphData || !selectedNode) return 0;
      const id = selectedNode.id;
      const neighbors = new Set<string>();
      for (const edge of graphData.edges) {
        if (edge.source === id) neighbors.add(edge.target);
        else if (edge.target === id) neighbors.add(edge.source);
      }
      return neighbors.size;
    })();
    return (
      <aside
        aria-label={t("graph.inspector.title")}
        className="flex w-[var(--rightpanel-w)] flex-col border-l border-[var(--border)] bg-[var(--surface)]"
      >
        <div className="flex h-[52px] items-center border-b border-[var(--border-subtle)] bg-[var(--background)] px-4">
          <span className="text-xs font-semibold uppercase tracking-[0.08em] text-[var(--text-secondary)]">
            {t("graph.inspector.title")}
          </span>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto">
          <GraphInspector
            node={selectedNode}
            neighborCount={neighborCount}
            onOpenPage={() => {
              if (!selectedNode) return;
              setActiveView("wiki");
              void openWikiPage(
                currentProject.projectId,
                currentProject.rootPath,
                selectedNode.path,
              );
            }}
          />
        </div>
      </aside>
    );
  }

  if (activeView === "import") {
    return <ImportRightPanel />;
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
      aria-label={t("shell.projectInfo")}
      className="flex w-[var(--rightpanel-w)] flex-col border-l border-[var(--border)] bg-[var(--surface)]"
    >
      <div className="flex h-[52px] items-center border-b border-[var(--border-subtle)] bg-[var(--background)] px-4">
        <span className="text-xs font-semibold uppercase tracking-[0.08em] text-[var(--text-secondary)]">{t("shell.projectInfo")}</span>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-4 py-3">
        {/* 路径 */}
        <div className="border-b border-[var(--border-subtle)] py-3">
          <h4 className="mb-2 text-[11px] font-semibold uppercase tracking-[0.06em] text-[var(--text-muted)]">{t("rightpanel.section.paths")}</h4>
          <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-2 text-xs">
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
          <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-2 text-xs">
            <dt className="font-medium text-[var(--text-muted)]">{t("rightpanel.index.pages")}</dt>
            <dd className="m-0 font-mono text-[11.5px] text-[var(--text-primary)]">{currentProject.wikiPageCount}</dd>
            <dt className="font-medium text-[var(--text-muted)]">{t("rightpanel.index.index")}</dt>
            <dd className="m-0 rounded-[var(--radius-pill)] bg-[var(--accent-soft)] px-2 py-0.5 text-[var(--accent-hover)]" style={{ display: "inline-block" }}>
              {t(`status.indexState.${currentProject.indexState as IndexState}`)}
            </dd>
            <dt className="font-medium text-[var(--text-muted)]">{t("rightpanel.index.graphCache")}</dt>
            <dd className="m-0 font-mono text-[11.5px]">{t(`dashboard.graph.state.${currentProject.graphState as GraphState}`)}</dd>
            <dt className="font-medium text-[var(--text-muted)]">{t("rightpanel.index.lastCompile")}</dt>
            <dd className="m-0 font-mono text-[11.5px] text-[var(--text-muted)]">—</dd>
            <dt className="font-medium text-[var(--text-muted)]">{t("rightpanel.index.pending")}</dt>
            <dd className="m-0 font-mono text-[11.5px]" style={{ color: pendingCount > 0 ? "var(--warning)" : "var(--text-secondary)" }}>
              {pendingCount > 0 ? t("rightpanel.pending.count", { count: pendingCount }) : t("rightpanel.pending.none")}
            </dd>
          </dl>
        </div>

        {/* 执行路径 */}
        <div className="border-b border-[var(--border-subtle)] py-3">
          <h4 className="mb-2 text-[11px] font-semibold uppercase tracking-[0.06em] text-[var(--text-muted)]">{t("rightpanel.section.route")}</h4>
          <div className="flex flex-col gap-2 text-xs">
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
          <div className="flex flex-col gap-2 text-xs">
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

        {/* 磁盘占用 */}
        <div className="py-3">
          <h4 className="mb-2 text-[11px] font-semibold uppercase tracking-[0.06em] text-[var(--text-muted)]">{t("rightpanel.section.disk")}</h4>
          <p className="m-0 text-[11px] text-[var(--text-muted)]">{t("rightpanel.disk.unavailable")}</p>
        </div>
      </div>
    </aside>
  );
}
