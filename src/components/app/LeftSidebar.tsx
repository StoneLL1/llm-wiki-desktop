import { useEffect, useMemo } from "react";
import { ChevronRight, FileOutput, FileText, Star } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useProjectStatus } from "../../hooks/useProjectStatus";
import { useLintStore } from "../../stores/lintStore";
import { useNavigationStore } from "../../stores/navigationStore";
import { useProjectStore } from "../../stores/projectStore";
import { useToastStore } from "../../stores/toastStore";
import { useExportStore } from "../../stores/exportStore";
import { selectFavoriteSidebarItems } from "../../features/bookmarks/bookmarkSelectors";
import { useWikiStore } from "../../features/wiki/wikiStore";
import type { FavoriteSidebarItem } from "../../types/bookmark";
import type { NavigationItem } from "./shellNavigation";
import { mainViews, workflowViews } from "./shellNavigation";
import { preloadWorkspaceView } from "./workspaceViewLoaders";

export function LeftSidebar() {
  const { t } = useTranslation();
  const activeView = useNavigationStore((state) => state.activeView);
  const setActiveView = useNavigationStore((state) => state.setActiveView);
  const openSettings = useNavigationStore((state) => state.openSettings);
  const currentProject = useProjectStore((state) => state.currentProject);
  const recentPages = useWikiStore((state) => state.recentPages);
  const wikiTree = useWikiStore((state) => state.tree);
  const wikiPages = wikiTree?.pages ?? [];
  const openPage = useWikiStore((state) => state.openPage);
  const exportRecords = useExportStore((state) => state.records);
  const loadExports = useExportStore((state) => state.loadExports);
  const loadPreview = useExportStore((state) => state.loadPreview);
  const pushToast = useToastStore((state) => state.pushToast);
  const localReport = useLintStore((state) => state.localReport);
  const lintIssueCount = localReport?.issues.length ?? 0;
  const status = useProjectStatus(
    currentProject.projectId,
    currentProject.rootPath,
    true,
    ["agents"],
  );
  const hasProject = Boolean(currentProject.projectId && currentProject.rootPath);

  const defaultAgent = status?.agents?.find((a) => a.isDefault) ?? null;
  const agentReady = defaultAgent?.state === "installed";
  const agentLabel = defaultAgent
    ? `${defaultAgent.kind} · ${defaultAgent.version ?? "—"}`
    : t("shell.agentUnconfigured");
  const agentDot = agentReady ? "bg-[var(--accent)] shadow-[0_0_0_2px_var(--accent-soft)]" : "bg-[var(--text-disabled)]";
  const favorites = useMemo(
    () => selectFavoriteSidebarItems(wikiPages, exportRecords),
    [wikiPages, exportRecords],
  );

  useEffect(() => {
    if (
      exportRecords.length > 0 ||
      !currentProject.projectId ||
      !currentProject.rootPath ||
      typeof window === "undefined" ||
      !("__TAURI_INTERNALS__" in window)
    ) {
      return;
    }
    void loadExports(currentProject.projectId, currentProject.rootPath);
  }, [currentProject.projectId, currentProject.rootPath, exportRecords.length, loadExports]);

  const openRecentPage = (path: string) => {
    if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) {
      pushToast("warning", t("shell.browserUnavailable"));
      return;
    }
    setActiveView("wiki");
    void openPage(currentProject.projectId, currentProject.rootPath, path);
  };

  const openFavorite = (item: FavoriteSidebarItem) => {
    if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) {
      pushToast("warning", t("shell.browserUnavailable"));
      return;
    }
    if (item.kind === "wiki_page") {
      setActiveView("wiki");
      void openPage(currentProject.projectId, currentProject.rootPath, item.path);
      return;
    }
    const projectId = currentProject.projectId;
    const rootPath = currentProject.rootPath;
    setActiveView("exports");
    void loadExports(projectId, rootPath).then(() => {
      const latestProject = useProjectStore.getState().currentProject;
      if (latestProject.projectId !== projectId || latestProject.rootPath !== rootPath) {
        return;
      }
      loadPreview(
        {
          projectId,
          projectRootPath: rootPath,
          outputPath: item.path,
        },
        item.exportRecordId ?? item.id,
      );
    });
  };

  const renderNavGroup = (items: NavigationItem[]) =>
    items.map((item) => {
      const Icon = item.icon;
      const active = item.view === activeView;
      return (
        <button
          key={item.view}
          data-app-view={item.view}
          aria-label={t(item.labelKey)}
          aria-current={active ? "page" : undefined}
          className={`flex h-[30px] w-full items-center gap-2 rounded-[var(--radius-md)] px-2 text-left text-[13px] font-medium ${
            active
              ? "bg-[var(--accent-soft)] text-[var(--accent-hover)]"
              : "text-[var(--text-secondary)] hover:bg-[var(--surface-muted)]"
          }`}
          onClick={() => setActiveView(item.view)}
          onFocus={() => void preloadWorkspaceView(item.view)}
          onPointerEnter={() => void preloadWorkspaceView(item.view)}
          title={t(item.labelKey)}
          type="button"
        >
          <Icon aria-hidden="true" className="shrink-0" size={16} />
          <span className="app-sidebar__nav-label truncate">{t(item.labelKey)}</span>
          {item.view === "wiki" ? (
            <span className={`app-sidebar__count ml-auto font-mono text-[10.5px] ${active ? "text-[var(--accent-hover)]" : "text-[var(--text-muted)]"}`}>{currentProject.wikiPageCount}</span>
          ) : null}
          {item.view === "lint" && lintIssueCount > 0 ? (
            <span className="app-sidebar__count ml-auto inline-flex h-[16px] min-w-[16px] items-center justify-center rounded-[var(--radius-pill)] bg-[var(--danger)] px-[5px] text-[9.5px] font-semibold text-[var(--text-inverse)]" aria-label={`${lintIssueCount} lint issues`}>{lintIssueCount}</span>
          ) : null}
        </button>
      );
    });

  return (
    <aside
      aria-label={t("shell.primaryNavigation")}
      role="navigation"
      className="app-sidebar"
    >
      <div className="app-sidebar__scroll-region app-pane-scrollbar">
        <div className="py-3">
          <div className="app-sidebar__section-label">
            {t("shell.mainViews")}
          </div>
          <div className="flex flex-col gap-[1px] px-2">{renderNavGroup(mainViews)}</div>
        </div>

        <div className="py-3">
          <div className="app-sidebar__section-label">
            {t("shell.knowledgeProcessing")}
          </div>
          <div className="flex flex-col gap-[1px] px-2">{renderNavGroup(workflowViews)}</div>
        </div>

        <div className="app-sidebar__favorites py-3">
          <div className="app-sidebar__section-label flex items-center justify-between">
            <span>{t("shell.favorites")}</span>
            <Star aria-hidden="true" className="shrink-0" size={11} />
          </div>
          <div className="flex flex-col gap-[1px] px-2">
            {favorites.length === 0 ? (
              <p className="m-0 px-2 text-[11.5px] leading-5 text-[var(--text-muted)]">
                {t("shell.favorites.empty")}
              </p>
            ) : (
              favorites.map((item) => {
                const Icon = item.kind === "wiki_page" ? FileText : FileOutput;
                return (
                  <button
                    key={item.id}
                    aria-label={t(item.kind === "wiki_page" ? "shell.favorite.openWiki" : "shell.favorite.openExport", { title: item.title })}
                    onClick={() => openFavorite(item)}
                    className="flex h-[26px] w-full items-center gap-2 rounded-[var(--radius-sm)] px-2 text-left text-[12.5px] text-[var(--text-secondary)] hover:bg-[var(--surface-muted)]"
                    title={item.missing ? t("shell.favorite.missingExport") : item.path}
                    type="button"
                  >
                    <Icon aria-hidden="true" className="shrink-0 text-[var(--accent)]" size={14} />
                    <span className="truncate">{item.title}</span>
                  </button>
                );
              })
            )}
          </div>
        </div>

        <div className="app-sidebar__recent py-3">
          <div className="app-sidebar__section-label flex items-center justify-between">
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
                  aria-label={page.title}
                  onClick={() => openRecentPage(page.path)}
                  className="flex h-[26px] w-full items-center gap-2 rounded-[var(--radius-sm)] px-2 text-left text-[12.5px] text-[var(--text-secondary)] hover:bg-[var(--surface-muted)]"
                  title={page.path}
                  type="button"
                >
                  <FileText aria-hidden="true" className={`shrink-0 ${index === 0 ? "text-[var(--accent)]" : "text-[var(--text-secondary)]"}`} size={14} />
                  <span className="truncate">{page.title}</span>
                </button>
              ))
            )}
          </div>
        </div>
      </div>

      {hasProject ? (
        <div className="flex shrink-0 items-center gap-2 border-t border-[var(--border-subtle)] px-2 py-2 text-[11px] text-[var(--text-muted)]">
          <span className="flex min-w-0 flex-1 items-center gap-2">
            <span className={`inline-block h-[7px] w-[7px] shrink-0 rounded-full ${agentDot}`} aria-hidden="true" />
            <span className="app-sidebar__agent-name truncate font-mono text-[var(--text-secondary)]">{agentLabel}</span>
          </span>
          <button
            aria-label={t("shell.agentTooltip")}
            className="btn btn--ghost btn--icon btn--sm shrink-0"
            onClick={() => openSettings("ai")}
            title={t("shell.agentTooltip")}
            type="button"
          >
            <ChevronRight aria-hidden="true" size={14} />
          </button>
        </div>
      ) : (
        <div className="app-sidebar__no-project-foot border-t border-[var(--border-subtle)] px-3 py-2 text-[11px] text-[var(--text-muted)]">
          {t("noProject.switcher")}
        </div>
      )}
    </aside>
  );
}
