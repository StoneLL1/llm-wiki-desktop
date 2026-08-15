import { useTranslation } from "react-i18next";

import { useProjectStatus } from "../../../hooks/useProjectStatus";
import { useProjectStore } from "../../../stores/projectStore";
import { useTaskStore } from "../../../stores/taskStore";
import type { GraphState, IndexState } from "../../../types/project";
import { RightPanelHeader } from "../RightPanelHeader";
import type { RightPanelHostProps } from "./types";

export function ProjectSummaryRightPanel({ currentProject }: RightPanelHostProps) {
  const { t } = useTranslation();
  const authority = useProjectStore((state) => state.authority);
  const pendingAction = useProjectStore((state) => state.pendingAction);
  const tasks = useTaskStore((state) => state.tasks);
  const status = useProjectStatus(currentProject.projectId, currentProject.rootPath, true);
  const health = currentProject.health;
  const pendingCount = pendingAction ? 1 : 0;
  const inventoryState = currentProject.inventoryState ?? "ready";
  const gitBranch = status?.git?.branch ?? null;
  const gitHead = status?.git?.head ?? null;
  const installedAgents = (status?.agents ?? []).filter((agent) => agent.state === "installed");
  const configuredProviders = (status?.providers ?? []).filter(
    (provider) => provider.config.enabled
      && (provider.hasSecret || provider.config.provider === "ollama"),
  );

  return (
    <aside id="right-context-panel" aria-label={t("shell.projectInfo")} className="right-panel">
      <RightPanelHeader title={t("shell.projectInfo")} />
      <div className="app-pane-scrollbar min-h-0 flex-1 overflow-y-auto px-4 py-3">
        {authority ? (
          <div className="border-b border-[var(--border-subtle)] py-3">
            <h4 className="mb-2 text-[11px] font-semibold uppercase tracking-[0.06em] text-[var(--text-muted)]">
              {t("projectAssessment.title")}
            </h4>
            <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-2 text-[12px]">
              <dt className="font-medium text-[var(--text-muted)]">{t("projectAssessment.dimension.format")}</dt>
              <dd className="m-0 text-right text-[var(--text-secondary)]">{t(`projectAssessment.format.${authority.format}`)}</dd>
              <dt className="font-medium text-[var(--text-muted)]">{t("projectAssessment.dimension.trust")}</dt>
              <dd className="m-0 text-right text-[var(--text-secondary)]">{t(`projectAssessment.trust.${authority.trust}`)}</dd>
              <dt className="font-medium text-[var(--text-muted)]">{t("projectAssessment.dimension.filesystem")}</dt>
              <dd className="m-0 text-right text-[var(--text-secondary)]">{t(`projectAssessment.filesystem.${authority.filesystemAccess}`)}</dd>
              <dt className="font-medium text-[var(--text-muted)]">{t("projectAssessment.dimension.health")}</dt>
              <dd className="m-0 text-right text-[var(--text-secondary)]">{t(`projectAssessment.health.${authority.health}`)}</dd>
              <dt className="font-medium text-[var(--text-muted)]">{t("projectAssessment.dimension.git")}</dt>
              <dd className="m-0 text-right text-[var(--text-secondary)]">{t(authority.git.isRepository ? "projectAssessment.git.repository" : "projectAssessment.git.none")}</dd>
            </dl>
            {authority.layoutWarnings.filter((warning) => warning.code === "UNSAFE_ENTRY_SKIPPED").map((warning) => (
              <div
                key={warning.code + ":" + (warning.path ?? warning.message)}
                className="mt-2 rounded-[var(--radius-sm)] border border-[var(--warning)] bg-[var(--warning-soft)] px-2.5 py-2 text-[11px] leading-4 text-[var(--text-secondary)]"
                role="status"
              >
                {t("projectAssessment.layoutUnsafeEntrySkipped", {
                  path: warning.path ?? t("projectAssessment.layoutUnsafeEntryUnknown"),
                })}
              </div>
            ))}
          </div>
        ) : null}

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

        <div className="border-b border-[var(--border-subtle)] py-3">
          <h4 className="mb-2 text-[11px] font-semibold uppercase tracking-[0.06em] text-[var(--text-muted)]">{t("rightpanel.section.indexState")}</h4>
          <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-2 text-[12px]">
            <dt className="font-medium text-[var(--text-muted)]">{t("rightpanel.index.pages")}</dt>
            <dd className="m-0 font-mono text-[11.5px] text-[var(--text-primary)]">
              {inventoryState === "ready" ? currentProject.wikiPageCount : t(`status.inventory.${inventoryState}`)}
            </dd>
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
                    {agent.isDefault ? <span className="inline-flex h-[18px] items-center rounded-[var(--radius-sm)] bg-[var(--accent-soft)] px-[7px] text-[10.5px] font-medium text-[var(--accent-hover)]">{t("rightpanel.route.default")}</span> : null}
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

        <div className="border-b border-[var(--border-subtle)] py-3">
          <h4 className="mb-2 text-[11px] font-semibold uppercase tracking-[0.06em] text-[var(--text-muted)]">{t("rightpanel.section.tasks")}</h4>
          <div className="flex flex-col gap-2 text-[12px]">
            {tasks.filter((task) => task.status === "running").length === 0 ? (
              tasks.filter((task) => task.status === "queued").length === 0 ? (
                <p className="m-0 text-[11px] text-[var(--text-muted)]">{t("rightpanel.tasks.none")}</p>
              ) : tasks.filter((task) => task.status === "queued").map((task) => (
                <div key={task.id} className="flex items-center gap-2">
                  <span className="dotstatus dotstatus--ok" aria-hidden="true" />
                  <span className="min-w-0 flex-1 truncate">{task.title}</span>
                  <span className="shrink-0 text-[var(--text-muted)]">{t(`task.status.${task.status}`)}</span>
                </div>
              ))
            ) : tasks.filter((task) => task.status === "running" || task.status === "queued").slice(0, 5).map((task) => (
              <div key={task.id} className="flex items-center gap-2">
                <span className={`dotstatus ${task.status === "running" ? "dotstatus--busy" : "dotstatus--ok"}`} aria-hidden="true" />
                <span className="min-w-0 flex-1 truncate">{task.title}</span>
                {task.progress != null ? (
                  <span className="shrink-0 font-mono text-[var(--text-muted)]">
                    {task.progress.total != null && task.progress.total > 0
                      ? `${Math.round((task.progress.current / task.progress.total) * 100)}%`
                      : (task.progress.label ?? task.progress.current)}
                  </span>
                ) : <span className="shrink-0 text-[var(--text-muted)]">{t(`task.status.${task.status}`)}</span>}
              </div>
            ))}
            {tasks.filter((task) => task.status === "succeeded" || task.status === "failed").length > 0
              && tasks.filter((task) => task.status === "running" || task.status === "queued").length === 0
              ? <p className="m-0 text-[11px] text-[var(--text-muted)]">{t("rightpanel.tasks.noOthers")}</p>
              : null}
          </div>
        </div>
      </div>
    </aside>
  );
}
