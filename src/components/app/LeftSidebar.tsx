import { ChevronRight, FileText } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useProjectStatus } from "../../hooks/useProjectStatus";
import { useLintStore } from "../../stores/lintStore";
import { useNavigationStore } from "../../stores/navigationStore";
import { useProjectStore } from "../../stores/projectStore";
import { useToastStore } from "../../stores/toastStore";
import { useWikiStore } from "../../features/wiki/wikiStore";
import type { NavigationItem } from "./shellNavigation";
import { mainViews, workflowViews } from "./shellNavigation";

export function LeftSidebar() {
  const { t } = useTranslation();
  const activeView = useNavigationStore((state) => state.activeView);
  const setActiveView = useNavigationStore((state) => state.setActiveView);
  const currentProject = useProjectStore((state) => state.currentProject);
  const recentPages = useWikiStore((state) => state.recentPages);
  const openPage = useWikiStore((state) => state.openPage);
  const pushToast = useToastStore((state) => state.pushToast);
  const localReport = useLintStore((state) => state.localReport);
  const lintIssueCount = localReport?.summary.totalIssues ?? 0;
  const status = useProjectStatus(currentProject.projectId, currentProject.rootPath);

  const defaultAgent = status?.agents?.find((a) => a.isDefault) ?? status?.agents?.find((a) => a.state === "installed") ?? null;
  const agentLabel = defaultAgent
    ? `${defaultAgent.kind} · ${defaultAgent.version ?? "—"}`
    : t("shell.agentUnconfigured");
  const agentDot = defaultAgent ? "bg-[var(--accent)] shadow-[0_0_0_2px_var(--accent-soft)]" : "bg-[var(--text-disabled)]";

  const openRecentPage = (path: string) => {
    if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) {
      pushToast("warning", t("shell.browserUnavailable"));
      return;
    }
    setActiveView("wiki");
    void openPage(currentProject.projectId, currentProject.rootPath, path);
  };

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
          {item.view === "lint" && lintIssueCount > 0 ? (
            <span className="ml-auto inline-flex h-[16px] min-w-[16px] items-center justify-center rounded-[var(--radius-pill)] bg-[#e0550a] px-[5px] text-[9.5px] font-semibold text-white" aria-label={`${lintIssueCount} lint issues`}>{lintIssueCount}</span>
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
          {recentPages.length === 0 ? (
            <p className="m-0 px-2 text-[11.5px] leading-5 text-[var(--text-muted)]">
              {t("shell.recentPages.empty")}
            </p>
          ) : (
            recentPages.map((page, index) => (
              <button
                key={page.path}
                onClick={() => openRecentPage(page.path)}
                className="flex h-[26px] w-full items-center gap-2 rounded-[var(--radius-sm)] px-2 text-left text-[12.5px] text-[var(--text-secondary)] hover:bg-[var(--surface-muted)]"
                title={page.path}
                type="button"
              >
                <FileText aria-hidden="true" className={index === 0 ? "text-[var(--accent)]" : "text-[var(--text-muted)]"} size={14} />
                <span className="truncate">{page.title}</span>
              </button>
            ))
          )}
        </div>
      </div>

      <div className="flex items-center gap-2 border-t border-[var(--border-subtle)] px-2 py-2 text-[11px] text-[var(--text-muted)]">
        <span className="flex min-w-0 flex-1 items-center gap-2">
          <span className={`inline-block h-[7px] w-[7px] shrink-0 rounded-full ${agentDot}`} aria-hidden="true" />
          <span className="truncate font-mono text-[var(--text-secondary)]">{agentLabel}</span>
        </span>
        <button
          aria-label={t("shell.agentTooltip")}
          className="btn btn--ghost btn--icon btn--sm shrink-0"
          onClick={() => setActiveView("agent")}
          title={t("shell.agentTooltip")}
          type="button"
        >
          <ChevronRight aria-hidden="true" size={14} />
        </button>
      </div>
    </aside>
  );
}
