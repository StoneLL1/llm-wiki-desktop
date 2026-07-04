import { useEffect, useMemo, useRef } from "react";
import { useTranslation } from "react-i18next";
import {
  ArrowRight,
  MessageSquare,
  CircleCheck,
  CircleAlert,
  GitBranch,
  PenLine,
  Shield,
  Upload,
  FileOutput,
  type LucideIcon,
} from "lucide-react";

import { useNavigationStore } from "../../stores/navigationStore";
import { useGraphStore } from "../../stores/graphStore";
import { useProjectStore } from "../../stores/projectStore";
import { useTaskStore } from "../../stores/taskStore";
import { useWikiStore } from "../wiki/wikiStore";
import { PAGE_TYPE_COLORS } from "../../types/graph";
import type { TaskType } from "../../types/task";
import type { WikiPageType } from "../../types/wiki";
import { buildDashboardGraphPreview, latestCompileTask } from "./dashboardGraphPreview";

const TYPE_LABEL_KEYS: Record<WikiPageType, string> = {
  entity: "wiki.type.entity",
  concept: "wiki.type.concept",
  source: "wiki.type.source",
  synthesis: "wiki.type.synthesis",
  comparison: "wiki.type.comparison",
  query: "wiki.type.query",
  index: "wiki.type.index",
  overview: "wiki.type.overview",
  log: "wiki.type.log",
  other: "wiki.type.other",
};

const ACTIVITY_ICON: Partial<Record<TaskType, LucideIcon>> = {
  wiki_compile: CircleCheck,
  import: Upload,
  deep_lint: Shield,
  auto_fix: Shield,
  export: FileOutput,
  graph_build: GitBranch,
};

export function DashboardView() {
  const { t } = useTranslation();
  const project = useProjectStore((state) => state.currentProject);
  const tasks = useTaskStore((state) => state.tasks);
  const graphData = useGraphStore((state) => state.data);
  const graphStatus = useGraphStore((state) => state.status);
  const setActiveView = useNavigationStore((state) => state.setActiveView);
  const tree = useWikiStore((state) => state.tree);
  const loadingTree = useWikiStore((state) => state.loadingTree);
  const scanWiki = useWikiStore((state) => state.scan);
  const scannedKey = useRef<string>("");

  useEffect(() => {
    const key = `${project.projectId}@${project.rootPath}`;
    const hasTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
    if (!hasTauri || !project.projectId || !project.rootPath) return;
    if (tree || loadingTree || scannedKey.current === key) return;
    scannedKey.current = key;
    void scanWiki(project.projectId, project.rootPath);
  }, [project.projectId, project.rootPath, tree, loadingTree, scanWiki]);

  const pages = tree?.pages ?? [];

  const stats = useMemo(() => {
    const byType = new Map<WikiPageType, number>();
    let wikilinks = 0;
    for (const page of pages) {
      byType.set(page.pageType, (byType.get(page.pageType) ?? 0) + 1);
      wikilinks += page.wikilinks?.length ?? 0;
    }
    return {
      wikiPages: pages.length || project.wikiPageCount,
      entities: byType.get("entity") ?? 0,
      concepts: byType.get("concept") ?? 0,
      sources: byType.get("source") ?? 0,
      synthesis: (byType.get("synthesis") ?? 0) + (byType.get("comparison") ?? 0),
      wikilinks,
      byType,
    };
  }, [pages, project.wikiPageCount]);

  const distribution = useMemo(() => {
    const order: WikiPageType[] = ["concept", "entity", "source", "synthesis", "comparison", "query"];
    const rows = order
      .map((type) => ({ type, count: stats.byType.get(type) ?? 0 }))
      .filter((row) => row.count > 0);
    const max = rows.reduce((m, r) => Math.max(m, r.count), 0) || 1;
    return { rows, max };
  }, [stats.byType]);

  const activity = useMemo(() => {
    return [...tasks]
      .sort((a, b) => (b.updatedAt ?? "").localeCompare(a.updatedAt ?? ""))
      .slice(0, 8)
      .map((task) => ({
        id: task.id,
        title: task.title,
        taskType: task.taskType,
        status: task.status,
        sub: task.result?.summary ?? task.progress?.label ?? "",
        time: task.updatedAt ?? task.startedAt,
        icon: ACTIVITY_ICON[task.taskType] ?? PenLine,
      }));
  }, [tasks]);

  const graphPreview = useMemo(
    () => buildDashboardGraphPreview(project, graphData, graphStatus, tasks, tree),
    [project, graphData, graphStatus, tasks, tree],
  );

  const lastCompileTask = latestCompileTask(tasks);
  const lastCompileTime = lastCompileTask?.completedAt ?? lastCompileTask?.updatedAt ?? null;

  const pendingLint = tasks.filter(
    (task) => (task.taskType === "deep_lint" || task.taskType === "auto_fix") && task.status === "waiting_for_confirmation",
  ).length;

  return (
    <div className="min-h-0 flex-1 overflow-auto">
      <div className="main__body--padded-lg" style={{ padding: "var(--sp-5)" }}>
        {/* Project health */}
        <div className="dash-section">
          <HealthRow
            project={project}
            lastCompileTime={lastCompileTime}
            pendingLint={pendingLint}
            onViewLint={() => setActiveView("lint")}
            onOpenAgent={() => setActiveView("agent")}
          />
        </div>

        {/* Stats summary */}
        <div className="dash-section">
          <h2 className="dash-section__title">
            {t("dashboard.stats.title")}
            <span className="count">· {t("dashboard.stats.asOf")}</span>
          </h2>
          <div className="summarygrid">
            <SumCard label={t("dashboard.stats.wikiPages")} value={stats.wikiPages} hint={t("dashboard.stats.wikiPagesHint")} />
            <SumCard label={t("dashboard.stats.entities")} value={stats.entities} hint={t("dashboard.stats.entitiesHint")} />
            <SumCard label={t("dashboard.stats.concepts")} value={stats.concepts} hint={t("dashboard.stats.conceptsHint")} />
            <SumCard label={t("dashboard.stats.sources")} value={stats.sources} hint={t("dashboard.stats.sourcesHint")} />
            <SumCard label={t("dashboard.stats.synthesis")} value={stats.synthesis} hint={t("dashboard.stats.synthesisHint")} />
            <SumCard label={t("dashboard.stats.wikilinks")} value={stats.wikilinks} hint={t("dashboard.stats.wikilinksHint")} />
          </div>
        </div>

        {/* Graph overview */}
        <div className="dash-section">
          <section className="panel--rich dashboard-graph" aria-label={t("dashboard.graphOverview.title")}>
            <div className="panel__head">
              <span className="panel__title">{t("dashboard.graphOverview.title")}</span>
              <span className="badge">{t(`dashboard.graph.state.${graphPreview.graphState}`)}</span>
            </div>
            <div className="panel__body dashboard-graph__body">
              <div className="dashboard-graph__metrics">
                <div>
                  <span className="dashboard-graph__metric-value">{graphPreview.nodeCount.toLocaleString()}</span>
                  {" "}
                  <span className="dashboard-graph__metric-label">{t("graph.nodesLabel")}</span>
                </div>
                <div>
                  <span className="dashboard-graph__metric-value">{graphPreview.edgeCount.toLocaleString()}</span>
                  {" "}
                  <span className="dashboard-graph__metric-label">{t("graph.edgesLabel")}</span>
                </div>
                {graphPreview.activeTaskLabel ? (
                  <div className="dashboard-graph__task">{graphPreview.activeTaskLabel}</div>
                ) : null}
                <div className="dashboard-graph__status">{graphStatusLabel(graphPreview.status, t)}</div>
                {graphPreview.topTypes.length > 0 ? (
                  <div className="dashboard-graph__types">
                    {graphPreview.topTypes.map((row) => (
                      <span key={row.type}>
                        {t(TYPE_LABEL_KEYS[row.type as WikiPageType] ?? "wiki.type.other")} {row.count}
                      </span>
                    ))}
                  </div>
                ) : null}
                <button type="button" className="btn btn--sm" onClick={() => setActiveView("graph")}>
                  {t("dashboard.graphOverview.open")}
                </button>
              </div>
              <svg className="dashboard-graph__preview" viewBox="0 0 120 72" aria-hidden="true">
                {graphPreview.previewEdges.map((edge) => {
                  const source = graphPreview.previewNodes.find((node) => node.id === edge.source);
                  const target = graphPreview.previewNodes.find((node) => node.id === edge.target);
                  if (!source || !target) return null;
                  return (
                    <line
                      key={`${edge.source}-${edge.target}`}
                      className="dashboard-graph__edge"
                      x1={source.x}
                      y1={source.y}
                      x2={target.x}
                      y2={target.y}
                    />
                  );
                })}
                {graphPreview.previewNodes.map((node) => (
                  <circle
                    key={node.id}
                    className="dashboard-graph__node"
                    cx={node.x}
                    cy={node.y}
                    r="3"
                    style={{ fill: PAGE_TYPE_COLORS[node.type as WikiPageType] ?? PAGE_TYPE_COLORS.other }}
                  />
                ))}
              </svg>
            </div>
          </section>
        </div>

        {/* Type distribution + recent activity */}
        <div className="dash-section">
          <div className="dash-grid" style={{ gridTemplateColumns: "1fr 1.4fr" }}>
            <section className="panel--rich" aria-label={t("dashboard.distribution.title")}>
              <div className="panel__head">
                <span className="panel__title">{t("dashboard.distribution.title")}</span>
                <span className="panel__sub">{t("dashboard.distribution.sub")}</span>
              </div>
              <div className="panel__body">
                {distribution.rows.length === 0 ? (
                  <p className="m-0 text-[12px] text-[var(--text-muted)]">{t("dashboard.distribution.empty")}</p>
                ) : (
                  distribution.rows.map((row) => (
                    <div className="type-row" key={row.type}>
                      <span className="type-row__swatch" style={{ background: PAGE_TYPE_COLORS[row.type] }} />
                      <span className="type-row__label">{t(TYPE_LABEL_KEYS[row.type])}</span>
                      <span className="type-row__bar">
                        <span
                          className="type-row__bar-fill"
                          style={{ width: `${Math.round((row.count / distribution.max) * 100)}%`, background: PAGE_TYPE_COLORS[row.type] }}
                        />
                      </span>
                      <span className="type-row__count">{row.count}</span>
                    </div>
                  ))
                )}
              </div>
            </section>

            <section className="panel--rich" aria-label={t("dashboard.activity.title")}>
              <div className="panel__head">
                <span className="panel__title">{t("dashboard.activity.title")}</span>
                <span className="panel__sub">{t("dashboard.activity.sub")}</span>
              </div>
              <div className="panel__body">
                {activity.length === 0 ? (
                  <p className="m-0 text-[12px] text-[var(--text-muted)]">{t("dashboard.activity.empty")}</p>
                ) : (
                  activity.map((row) => {
                    const Icon = row.icon;
                    const isError = row.status === "failed";
                    return (
                      <div className="activity-row" key={row.id}>
                        <span className="activity-row__icon">
                          <Icon size={16} color={isError ? "var(--danger)" : "var(--accent-hover)"} aria-hidden="true" />
                        </span>
                        <div>
                          <div className="activity-row__title">{row.title}</div>
                          {row.sub ? <div className="activity-row__sub">{row.sub}</div> : null}
                        </div>
                        <span className="activity-row__time">{relativeTime(row.time, t)}</span>
                      </div>
                    );
                  })
                )}
              </div>
            </section>
          </div>
        </div>

        {/* Quick actions */}
        <div className="dash-section">
          <h2 className="dash-section__title">{t("dashboard.quickActions.title")}</h2>
          <div className="dash-grid">
            <QuickAction
              icon={Upload}
              title={t("dashboard.quickActions.import")}
              desc={t("dashboard.quickActions.importDesc")}
              onClick={() => setActiveView("import")}
            />
            <QuickAction
              icon={Shield}
              title={t("dashboard.quickActions.lint")}
              desc={t("dashboard.quickActions.lintDesc")}
              tone="warn"
              onClick={() => setActiveView("lint")}
            />
            <QuickAction
              icon={MessageSquare}
              title={t("dashboard.quickActions.chat")}
              desc={t("dashboard.quickActions.chatDesc")}
              onClick={() => setActiveView("chat")}
            />
            <QuickAction
              icon={FileOutput}
              title={t("dashboard.quickActions.export")}
              desc={t("dashboard.quickActions.exportDesc")}
              onClick={() => setActiveView("exports")}
            />
          </div>
        </div>
      </div>
    </div>
  );
}

function HealthRow({
  project,
  lastCompileTime,
  pendingLint,
  onViewLint,
  onOpenAgent,
}: {
  project: ReturnType<typeof useProjectStore.getState>["currentProject"];
  lastCompileTime: string | null;
  pendingLint: number;
  onViewLint: () => void;
  onOpenAgent: () => void;
}) {
  const { t } = useTranslation();
  const healthy = project.health.hasPurpose && project.health.hasSchema && project.wikiPageCount > 0;
  const desc = t("dashboard.health.summary", {
    pages: project.wikiPageCount,
    pending: pendingLint,
    route: t(`status.routeLabel.${project.agentRoute}`),
  });
  const title = healthy
    ? t("dashboard.health.healthy", { time: lastCompileTime ? relativeTime(lastCompileTime, t) : t("dashboard.health.never") })
    : t("dashboard.health.needsSetup");

  return (
    <div className="health-row">
      <div className="health-row__icon">
        {healthy ? <CircleCheck size={22} aria-hidden="true" /> : <CircleAlert size={22} aria-hidden="true" />}
      </div>
      <div className="health-row__body">
        <div className="health-row__title">{title}</div>
        <div className="health-row__desc">{desc}</div>
      </div>
      <button type="button" className="btn btn--sm" onClick={onViewLint}>
        {t("dashboard.health.viewLint")}
      </button>
      <button type="button" className="btn btn--sm" onClick={onOpenAgent}>
        {t("dashboard.health.openAgent")}
      </button>
    </div>
  );
}

function SumCard({ label, value, hint }: { label: string; value: number; hint?: string }) {
  return (
    <div className="sumcard">
      <div className="sumcard__label">{label}</div>
      <div className="sumcard__value">{value.toLocaleString()}</div>
      {hint ? <div className="sumcard__hint">{hint}</div> : null}
    </div>
  );
}

function QuickAction({
  icon: Icon,
  title,
  desc,
  onClick,
  tone = "accent",
}: {
  icon: LucideIcon;
  title: string;
  desc: string;
  onClick: () => void;
  tone?: "accent" | "warn";
}) {
  return (
    <button type="button" className="panel--rich" style={{ textAlign: "left", cursor: "pointer", border: "1px solid var(--border)" }} onClick={onClick}>
      <div className="panel__body" style={{ display: "flex", alignItems: "center", gap: "var(--sp-4)" }}>
        <div
          className="health-row__icon"
          style={tone === "warn" ? { background: "var(--warning-soft)", color: "var(--warning-text)" } : undefined}
        >
          <Icon size={22} aria-hidden="true" />
        </div>
        <div style={{ flex: 1, minWidth: 0 }}>
          <div style={{ fontSize: 13, fontWeight: 600, color: "var(--text-primary)" }}>{title}</div>
          <div style={{ fontSize: 11.5, color: "var(--text-muted)", marginTop: 2 }}>{desc}</div>
        </div>
        <ArrowRight size={16} style={{ color: "var(--text-muted)" }} aria-hidden="true" />
      </div>
    </button>
  );
}

function graphStatusLabel(status: ReturnType<typeof useGraphStore.getState>["status"], t: (key: string) => string): string {
  if (status === "loading") return t("graph.loading");
  if (status === "rebuilding") return t("graph.status.rebuilding");
  if (status === "ready-empty") return t("graph.empty.noPages");
  if (status === "error") return t("graph.error");
  return t("graph.status.fresh");
}

function relativeTime(iso: string | null, t: (key: string, opts?: Record<string, unknown>) => string): string {
  if (!iso) return "";
  const then = new Date(iso).getTime();
  if (Number.isNaN(then)) return "";
  const diff = Date.now() - then;
  const min = Math.floor(diff / 60000);
  if (min < 1) return t("relative.justNow");
  if (min < 60) return t("relative.minutesAgo", { count: min });
  const hours = Math.floor(min / 60);
  if (hours < 24) return t("relative.hoursAgo", { count: hours });
  const days = Math.floor(hours / 24);
  if (days < 7) return t("relative.daysAgo", { count: days });
  return new Date(iso).toLocaleDateString();
}
