import { FileText } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useNavigationStore } from "../../stores/navigationStore";
import { useProjectStore } from "../../stores/projectStore";
import { primaryNavigation } from "./shellNavigation";

const recentPages = ["Agent Memory Model", "Tool Selection Heuristics", "ReAct Pattern", "Plan-and-Execute"];

export function LeftSidebar() {
  const { t } = useTranslation();
  const activeView = useNavigationStore((state) => state.activeView);
  const setActiveView = useNavigationStore((state) => state.setActiveView);
  const currentProject = useProjectStore((state) => state.currentProject);

  return (
    <aside className="flex w-[var(--sidebar-w)] flex-col border-r border-[var(--border)] bg-[var(--surface)] p-3">
      <nav aria-label={t("shell.primaryNavigation")} className="space-y-1">
        <div className="px-2 pb-2 text-[11px] font-medium uppercase tracking-[0.02em] text-[var(--text-muted)]">
          {t("shell.mainViews")}
        </div>
        {primaryNavigation.map((item) => {
          const Icon = item.icon;
          const active = item.view === activeView;
          const label = t(item.labelKey);

          return (
            <button
              key={item.view}
              aria-current={active ? "page" : undefined}
              className={`flex h-8 w-full items-center gap-2 rounded-[var(--radius-md)] px-2 text-left text-sm ${
                active
                  ? "bg-[var(--accent-soft)] font-medium text-[var(--accent-hover)]"
                  : "text-[var(--text-secondary)] hover:bg-[var(--surface-muted)]"
              }`}
              onClick={() => setActiveView(item.view)}
              type="button"
            >
              <Icon aria-hidden="true" size={18} />
              <span className="truncate">{label}</span>
              {item.view === "wiki" ? (
                <span className="ml-auto font-mono text-[11px] text-[var(--text-muted)]">{currentProject.wikiPageCount}</span>
              ) : null}
            </button>
          );
        })}
      </nav>

      <section className="mt-5 min-h-0 flex-1" aria-label={t("shell.recentPages")}>
        <div className="px-2 pb-2 text-[11px] font-medium uppercase tracking-[0.02em] text-[var(--text-muted)]">
          {t("shell.recentPages")}
        </div>
        <div className="space-y-0.5">
          {recentPages.map((page, index) => (
            <button
              key={page}
              className="flex h-[26px] w-full items-center gap-2 rounded-[var(--radius-sm)] px-2 text-left text-xs text-[var(--text-secondary)] hover:bg-[var(--surface-muted)]"
              type="button"
            >
              <FileText aria-hidden="true" className={index === 0 ? "text-[var(--accent)]" : "text-[var(--text-muted)]"} size={14} />
              <span className="truncate">{page}</span>
            </button>
          ))}
        </div>
      </section>

      <div className="mt-3 flex items-center justify-between border-t border-[var(--border-subtle)] pt-3 text-xs text-[var(--text-muted)]">
        <span className="flex min-w-0 items-center gap-2">
          <span className="h-2 w-2 rounded-full bg-[var(--accent)]" aria-hidden="true" />
          <span className="truncate">{t("status.agentDetected")}</span>
        </span>
      </div>
    </aside>
  );
}
