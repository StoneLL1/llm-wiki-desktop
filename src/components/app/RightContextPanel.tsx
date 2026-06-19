import { FileText, GitBranch, ListChecks } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { AppView } from "../../stores/navigationStore";
import { useProjectStore } from "../../stores/projectStore";
import { useTaskStore } from "../../stores/taskStore";

interface RightContextPanelProps {
  activeView: AppView;
}

export function RightContextPanel({ activeView }: RightContextPanelProps) {
  const { t } = useTranslation();
  const currentProject = useProjectStore((state) => state.currentProject);
  const tasks = useTaskStore((state) => state.tasks);

  return (
    <aside
      aria-label={t("shell.context")}
      className="flex w-[var(--rightpanel-w)] flex-col border-l border-[var(--border)] bg-[var(--surface)]"
    >
      <div className="border-b border-[var(--border-subtle)] px-3 py-2">
        <h2 className="m-0 text-sm font-semibold">{t("shell.context")}</h2>
        <p className="m-0 mt-1 text-xs text-[var(--text-muted)]">{t(`context.${activeView}`)}</p>
      </div>

      <div className="min-h-0 flex-1 space-y-3 overflow-auto p-3">
        <section className="panel">
          <div className="panel-header">
            <FileText aria-hidden="true" size={14} />
            <span>{t("context.project")}</span>
          </div>
          <dl className="mt-2 space-y-2 text-xs">
            <div>
              <dt className="text-[var(--text-muted)]">{t("status.projectPath")}</dt>
              <dd className="m-0 truncate font-mono text-[var(--text-secondary)]">{currentProject.path}</dd>
            </div>
            <div className="flex items-center justify-between">
              <dt className="text-[var(--text-muted)]">{t("status.index")}</dt>
              <dd className="m-0 rounded-[var(--radius-pill)] bg-[var(--accent-soft)] px-2 py-0.5 text-[var(--accent-hover)]">
                {t(`status.indexState.${currentProject.indexState}`)}
              </dd>
            </div>
          </dl>
        </section>

        <section className="panel">
          <div className="panel-header">
            <ListChecks aria-hidden="true" size={14} />
            <span>{t("context.tasks")}</span>
          </div>
          <div className="mt-2 space-y-2">
            {tasks.map((task) => (
              <div key={task.id} className="flex items-center gap-2 text-xs">
                <span className="h-2 w-2 rounded-full bg-[var(--accent)]" aria-hidden="true" />
                <span className="min-w-0 flex-1 truncate">{task.title}</span>
                <span className="text-[var(--text-muted)]">{t(`task.status.${task.status}`)}</span>
              </div>
            ))}
          </div>
        </section>

        <section className="panel">
          <div className="panel-header">
            <GitBranch aria-hidden="true" size={14} />
            <span>{t("context.safety")}</span>
          </div>
          <p className="m-0 mt-2 text-xs leading-5 text-[var(--text-muted)]">{t("context.safetyCopy")}</p>
        </section>
      </div>
    </aside>
  );
}
