import { useTranslation } from "react-i18next";
import { useProjectStore } from "../../stores/projectStore";
import { useTaskStore } from "../../stores/taskStore";

export function BottomStatusBar() {
  const { t } = useTranslation();
  const currentProject = useProjectStore((state) => state.currentProject);
  const runningCount = useTaskStore((state) => state.runningCount);

  return (
    <footer className="flex h-7 items-center justify-between border-t border-[var(--border)] bg-[var(--surface)] px-3 text-[11px] text-[var(--text-muted)]">
      <span className="truncate font-mono">{currentProject.path}</span>
      <div className="flex items-center gap-3">
        <span>{t("status.route", { route: currentProject.agentRoute })}</span>
        <span>{t("status.tasks", { count: runningCount })}</span>
        <span>{t("status.wikiPages", { count: currentProject.wikiPageCount })}</span>
      </div>
    </footer>
  );
}
