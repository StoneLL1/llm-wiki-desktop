import { useTranslation } from "react-i18next";
import { RelatedPagesPanel } from "../../features/wiki/RelatedPagesPanel";
import { useWikiStore } from "../../features/wiki/wikiStore";
import { useNavigationStore } from "../../stores/navigationStore";
import { useProjectStore } from "../../stores/projectStore";
import { useTaskStore } from "../../stores/taskStore";
import type { GraphState, IndexState } from "../../types/project";

export function RightContextPanel() {
  const { t } = useTranslation();
  const activeView = useNavigationStore((state) => state.activeView);
  const currentProject = useProjectStore((state) => state.currentProject);
  const tasks = useTaskStore((state) => state.tasks);
  const wikiPage = useWikiStore((state) => state.page?.meta ?? null);
  const wikiTree = useWikiStore((state) => state.tree);
  const openWikiPage = useWikiStore((state) => state.openPage);

  const openPage = (path: string) => {
    void openWikiPage(currentProject.projectId, currentProject.rootPath, path);
  };

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
          />
        </div>
      </aside>
    );
  }

  const health = currentProject.health;
  const routeLabel = t(`status.routeLabel.${currentProject.agentRoute}`, { defaultValue: currentProject.agentRoute });

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
          </dl>
        </div>

        {/* 执行路径 */}
        <div className="border-b border-[var(--border-subtle)] py-3">
          <h4 className="mb-2 text-[11px] font-semibold uppercase tracking-[0.06em] text-[var(--text-muted)]">{t("rightpanel.section.route")}</h4>
          <div className="flex flex-col gap-2 text-xs">
            <div className="flex items-center gap-2">
              <span className={`inline-block h-2 w-2 rounded-full ${currentProject.agentRoute === "unconfigured" ? "bg-[var(--text-disabled)]" : "bg-[var(--accent)] shadow-[0_0_0_2px_var(--accent-soft)]"}`} aria-hidden="true" />
              <span className="font-mono">{routeLabel}</span>
              {currentProject.agentRoute !== "unconfigured" && (
                <span className="ml-auto inline-flex h-[18px] items-center rounded-[var(--radius-sm)] bg-[var(--accent-soft)] px-[7px] text-[10.5px] font-medium text-[var(--accent-hover)]">默认</span>
              )}
            </div>
          </div>
        </div>

        {/* 背景任务 */}
        <div className="py-3">
          <h4 className="mb-2 text-[11px] font-semibold uppercase tracking-[0.06em] text-[var(--text-muted)]">{t("rightpanel.section.tasks")}</h4>
          <div className="flex flex-col gap-2 text-xs">
            {tasks.length === 0 ? (
              <p className="m-0 text-[11px] text-[var(--text-muted)]">{t("rightpanel.tasks.none")}</p>
            ) : (
              tasks.map((task) => (
                <div key={task.id} className="flex items-center gap-2">
                  <span className={`h-2 w-2 shrink-0 rounded-full ${task.status === "running" ? "bg-[var(--warning)] shadow-[0_0_0_2px_var(--warning-soft)]" : "bg-[var(--accent)] shadow-[0_0_0_2px_var(--accent-soft)]"}`} aria-hidden="true" />
                  <span className="min-w-0 flex-1 truncate">{task.title}</span>
                  <span className="shrink-0 text-[var(--text-muted)]">{t(`task.status.${task.status}`)}</span>
                </div>
              ))
            )}
          </div>
        </div>
      </div>
    </aside>
  );
}
