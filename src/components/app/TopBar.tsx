import { FolderOpen, Search, Settings } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useNavigationStore } from "../../stores/navigationStore";
import { useProjectStore } from "../../stores/projectStore";
import { TaskActivityButton } from "./TaskActivityButton";

export function TopBar() {
  const { i18n, t } = useTranslation();
  const setActiveView = useNavigationStore((state) => state.setActiveView);
  const currentProject = useProjectStore((state) => state.currentProject);
  const activeLanguage = i18n.resolvedLanguage ?? i18n.language;

  return (
    <header className="flex h-12 items-center gap-3 border-b border-[var(--border)] bg-[var(--background)] px-4 text-[13px]">
      <div className="flex h-full items-center gap-2 border-r border-[var(--border-subtle)] pr-3">
        <div className="grid h-[22px] w-[22px] place-items-center rounded-[var(--radius-sm)] bg-[var(--foreground)] font-mono text-[11px] font-semibold text-[var(--text-inverse)]">
          LW
        </div>
        <strong className="text-[13px] font-semibold tracking-[-0.01em] text-[var(--text-primary)]">{t("app.title")}</strong>
      </div>

      <button
        aria-label={t("shell.switchProject")}
        className="flex h-[30px] min-w-0 max-w-[360px] items-center gap-2 rounded-[var(--radius-md)] px-2 text-left hover:bg-[var(--surface-muted)]"
        type="button"
      >
        <FolderOpen aria-hidden="true" className="text-[var(--text-muted)]" size={16} />
        <span className="truncate text-[13px] font-medium">{currentProject.name}</span>
        <span className="truncate font-mono text-[11px] text-[var(--text-muted)]">{currentProject.rootPath}</span>
      </button>

      <label className="flex h-[30px] max-w-[520px] flex-1 items-center gap-2 rounded-[var(--radius-md)] bg-[var(--surface-muted)] px-3 text-[13px] text-[var(--text-muted)] focus-within:bg-[var(--background)] focus-within:shadow-[0_0_0_3px_rgba(16,163,127,0.12)]">
        <Search aria-hidden="true" size={14} />
        <input
          aria-label={t("shell.search")}
          className="min-w-0 flex-1 border-0 bg-transparent p-0 text-[13px] outline-none placeholder:text-[var(--text-muted)]"
          placeholder={t("shell.searchPlaceholder")}
          type="search"
        />
        <kbd className="rounded border border-[var(--border)] bg-[var(--background)] px-1.5 py-0.5 font-mono text-[10px] text-[var(--text-muted)]">
          {t("shell.searchShortcut")}
        </kbd>
      </label>

      <div className="ml-auto flex items-center gap-2">
        <TaskActivityButton />
        <div className="flex h-[26px] items-center rounded-[var(--radius-md)] bg-[var(--surface-muted)] p-0.5 text-[11px]">
          <button
            aria-pressed={activeLanguage === "en"}
            className={`rounded-[var(--radius-sm)] px-2 py-0.5 ${
              activeLanguage === "en" ? "bg-[var(--background)] font-medium" : "text-[var(--text-muted)]"
            }`}
            onClick={() => void i18n.changeLanguage("en")}
            type="button"
          >
            {t("language.en")}
          </button>
          <button
            aria-pressed={activeLanguage === "zh-CN"}
            className={`rounded-[var(--radius-sm)] px-2 py-0.5 ${
              activeLanguage === "zh-CN" ? "bg-[var(--background)] font-medium" : "text-[var(--text-muted)]"
            }`}
            onClick={() => void i18n.changeLanguage("zh-CN")}
            type="button"
          >
            {t("language.zhCN")}
          </button>
        </div>
        <button
          aria-label={t("nav.settings")}
          className="icon-button"
          onClick={() => setActiveView("settings")}
          title={t("nav.settings")}
          type="button"
        >
          <Settings aria-hidden="true" size={16} />
        </button>
      </div>
    </header>
  );
}
