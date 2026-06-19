import { Bell } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useTaskStore } from "../../stores/taskStore";

export function TaskActivityButton() {
  const { t } = useTranslation();
  const runningCount = useTaskStore((state) => state.runningCount);

  return (
    <button aria-label={t("shell.tasks")} className="icon-button relative" title={t("shell.tasks")} type="button">
      <Bell aria-hidden="true" size={16} />
      {runningCount > 0 ? (
        <span className="absolute right-1 top-1 h-1.5 w-1.5 rounded-full bg-[var(--accent)]" aria-hidden="true" />
      ) : null}
    </button>
  );
}
