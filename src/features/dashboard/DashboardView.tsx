import { useTranslation } from "react-i18next";
import { useProjectStore } from "../../stores/projectStore";
import { useTaskStore } from "../../stores/taskStore";
import type { AgentRoute, GraphState, IndexState } from "../../types/project";

export function DashboardView() {
  const { t } = useTranslation();
  const project = useProjectStore((state) => state.currentProject);
  const recentProjects = useProjectStore((state) => state.recentProjects);
  const tasks = useTaskStore((state) => state.tasks);

  const routeLabel = t(`status.routeLabel.${project.agentRoute}`, {
    defaultValue: project.agentRoute,
  });

  return (
    <div className="grid h-full grid-flow-row auto-rows-min gap-3 overflow-auto p-4">
      <section className="panel" aria-label={t("dashboard.health.title")}>
        <div className="panel-header">{t("dashboard.health.title")}</div>
        <dl className="m-0 mt-3 grid grid-cols-3 gap-x-4 gap-y-2 text-xs">
          <Metric label={t("dashboard.health.wikiPages")} value={String(project.wikiPageCount)} />
          <Metric label={t("dashboard.health.sources")} value={String(project.sourceCount)} />
          <Metric label={t("dashboard.health.tasks")} value={String(project.taskCount)} />
          <Metric
            label={t("dashboard.health.indexState")}
            value={t(`status.indexState.${project.indexState as IndexState}`)}
          />
          <Metric
            label={t("dashboard.health.graphState")}
            value={t(`dashboard.graph.state.${project.graphState as GraphState}`)}
          />
          <Metric label={t("dashboard.health.route")} value={routeLabel} />
        </dl>
        <ProjectHealth project={project} />
      </section>

      <div className="grid grid-cols-2 gap-3">
        <section className="panel" aria-label={t("dashboard.import.title")}>
          <div className="panel-header">{t("dashboard.import.title")}</div>
          <p className="m-0 mt-2 text-xs leading-5 text-[var(--text-muted)]">
            {t("dashboard.import.body", { count: project.sourceCount })}
          </p>
        </section>
        <section className="panel" aria-label={t("dashboard.route.title")}>
          <div className="panel-header">{t("dashboard.route.title")}</div>
          <p className="m-0 mt-2 text-xs leading-5 text-[var(--text-muted)]">
            {t(`dashboard.route.body.${project.agentRoute as AgentRoute}`)}
          </p>
        </section>
      </div>

      <section className="panel" aria-label={t("dashboard.recentProjects.title")}>
        <div className="panel-header">{t("dashboard.recentProjects.title")}</div>
        {recentProjects.length === 0 ? (
          <p className="m-0 mt-2 text-xs text-[var(--text-muted)]">{t("dashboard.recentProjects.empty")}</p>
        ) : (
          <ul className="m-0 mt-2 grid list-none gap-1 p-0 text-xs">
            {recentProjects.slice(0, 5).map((entry) => (
              <li key={entry.projectId} className="flex items-center justify-between gap-3">
                <span className="truncate font-medium">{entry.name}</span>
                <span className="truncate font-mono text-[11px] text-[var(--text-muted)]">{entry.rootPath}</span>
              </li>
            ))}
          </ul>
        )}
      </section>

      <section className="panel" aria-label={t("dashboard.recentTasks.title")}>
        <div className="panel-header">{t("dashboard.recentTasks.title")}</div>
        {tasks.length === 0 ? (
          <p className="m-0 mt-2 text-xs text-[var(--text-muted)]">{t("dashboard.recentTasks.empty")}</p>
        ) : (
          <ul className="m-0 mt-2 grid list-none gap-1 p-0 text-xs">
            {tasks.slice(0, 6).map((task) => (
              <li key={task.id} className="flex items-center justify-between gap-3">
                <span className="truncate">{task.title}</span>
                <span className="text-[var(--text-muted)]">{t(`task.status.${task.status}`)}</span>
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex flex-col gap-1">
      <dt className="text-[11px] uppercase tracking-wide text-[var(--text-muted)]">{label}</dt>
      <dd className="m-0 font-mono text-sm text-[var(--text-primary)]">{value}</dd>
    </div>
  );
}

function ProjectHealth({ project }: { project: ReturnType<typeof useProjectStore.getState>["currentProject"] }) {
  const { t } = useTranslation();
  const items: Array<{ key: string; ok: boolean; label: string }> = [
    { key: "purpose", ok: project.health.hasPurpose, label: t("dashboard.health.flag.purpose") },
    { key: "schema", ok: project.health.hasSchema, label: t("dashboard.health.flag.schema") },
    { key: "app", ok: project.health.hasAppState, label: t("dashboard.health.flag.appState") },
    {
      key: "wiki",
      ok: project.wikiPageCount > 0,
      label: t("dashboard.health.flag.wiki"),
    },
  ];
  if (project.health.hasObsidian) {
    items.push({ key: "obsidian", ok: true, label: t("dashboard.health.flag.obsidian") });
  }

  return (
    <ul className="m-0 mt-3 flex list-none flex-wrap gap-2 p-0">
      {items.map((item) => (
        <li
          key={item.key}
          className={`inline-flex items-center gap-1 rounded-[var(--radius-pill)] border px-2 py-0.5 text-[11px] ${
            item.ok
              ? "border-[var(--accent-border)] bg-[var(--accent-soft)] text-[var(--accent-hover)]"
              : "border-[var(--border)] bg-[var(--surface-muted)] text-[var(--text-muted)]"
          }`}
        >
          <span aria-hidden>{item.ok ? "✓" : "—"}</span>
          {item.label}
        </li>
      ))}
    </ul>
  );
}
