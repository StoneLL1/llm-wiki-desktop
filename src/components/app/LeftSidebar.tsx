import { FileText } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useNavigationStore } from "../../stores/navigationStore";
import { useProjectStore } from "../../stores/projectStore";
import type { NavigationItem } from "./shellNavigation";
import { mainViews, workflowViews } from "./shellNavigation";

const recentPages = ["Agent Memory Model", "Tool Selection Heuristics", "ReAct Pattern", "Plan-and-Execute"];

export function LeftSidebar() {
  const { t } = useTranslation();
  const activeView = useNavigationStore((state) => state.activeView);
  const setActiveView = useNavigationStore((state) => state.setActiveView);
  const currentProject = useProjectStore((state) => state.currentProject);

  const renderNavGroup = (items: NavigationItem[]) =>
    items.map((item) => {
      const Icon = item.icon;
      const active = item.view === activeView;
      return (
        <button
          key={item.view}
          aria-current={active ? "page" : undefined}
          className={`flex h-[30px] w-full items-center gap-2 rounded-[var(--radius-md)] px-2 text-left text-[13px] font-medium ${
            active
              ? "bg-[var(--accent-soft)] text-[var(--accent-hover)]"
              : "text-[var(--text-secondary)] hover:bg-[var(--surface-muted)]"
          }`}
          onClick={() => setActiveView(item.view)}
          type="button"
        >
          <Icon aria-hidden="true" size={16} />
          <span className="truncate">{t(item.labelKey)}</span>
          {item.view === "wiki" ? (
            <span className={`ml-auto font-mono text-[10.5px] ${active ? "text-[var(--accent-hover)]" : "text-[var(--text-muted)]"}`}>{currentProject.wikiPageCount}</span>
          ) : null}
        </button>
      );
    });

  return (
    <aside
      aria-label={t("shell.primaryNavigation")}
      role="navigation"
      className="flex w-[var(--sidebar-w)] flex-col overflow-hidden border-r border-[var(--border)] bg-[var(--surface)]"
    >
      <div className="py-3">
        <div className="px-2 pb-1 text-[10.5px] font-medium uppercase tracking-[0.08em] text-[var(--text-muted)]">
          {t("shell.mainViews")}
        </div>
        <div className="flex flex-col gap-[1px] px-2">{renderNavGroup(mainViews)}</div>
      </div>

      <div className="py-3">
        <div className="px-2 pb-1 text-[10.5px] font-medium uppercase tracking-[0.08em] text-[var(--text-muted)]">
          {t("shell.workflow")}
        </div>
        <div className="flex flex-col gap-[1px] px-2">{renderNavGroup(workflowViews)}</div>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto py-3">
        <div className="flex items-center justify-between px-2 pb-1 text-[10.5px] font-medium uppercase tracking-[0.08em] text-[var(--text-muted)]">
          <span>{t("shell.recentPages")}</span>
        </div>
        <div className="flex flex-col gap-[1px] px-2">
          {recentPages.map((page, index) => (
            <button
              key={page}
              className="flex h-[26px] w-full items-center gap-2 rounded-[var(--radius-sm)] px-2 text-left text-[12.5px] text-[var(--text-secondary)] hover:bg-[var(--surface-muted)]"
              type="button"
            >
              <FileText aria-hidden="true" className={index === 0 ? "text-[var(--accent)]" : "text-[var(--text-muted)]"} size={14} />
              <span className="truncate">{page}</span>
            </button>
          ))}
        </div>
      </div>

      <div className="flex items-center gap-2 border-t border-[var(--border-subtle)] px-2 py-2 text-[11px] text-[var(--text-muted)]">
        <span className="flex min-w-0 flex-1 items-center gap-2">
          <span className="h-[7px] w-[7px] shrink-0 rounded-full bg-[var(--accent)] shadow-[0_0_0_2px_var(--accent-soft)]" aria-hidden="true" />
          <span className="truncate font-mono text-[var(--text-secondary)]">{t("status.agentDetected")}</span>
        </span>
      </div>
    </aside>
  );
}
