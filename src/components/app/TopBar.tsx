import { invoke } from "@tauri-apps/api/core";
import { ChevronDown, FolderOpen, FolderSearch, LayoutDashboard, RotateCcw, Search, Settings, Trash2 } from "lucide-react";
import { type KeyboardEvent as ReactKeyboardEvent, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { i18next, LANGUAGE_STORAGE_KEY } from "../../i18n";
import { compactPath } from "../../lib/pathDisplay";
import { useNavigationStore } from "../../stores/navigationStore";
import { useProjectStore } from "../../stores/projectStore";
import { useSettingsStore } from "../../stores/settingsStore";
import { useWikiStore } from "../../features/wiki/wikiStore";
import { pickDirectory } from "../../features/import/nativeFilePicker";
import type { ProjectOpenAssessment, RecentProject } from "../../types/project";
import type { SearchResult, SearchResponse } from "../../types/wiki";
import { TaskActivityButton } from "./TaskActivityButton";
import appLogoUrl from "../../assets/app-logo.png";

function formatOpenedAt(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return iso;
  return date.toLocaleDateString();
}

function canOpenAssessment(assessment: ProjectOpenAssessment): boolean {
  return (
    !["ambiguous_markdown", "ordinary_materials", "unknown"].includes(assessment.format)
    && assessment.health !== "unreadable"
  );
}

export function TopBar() {
  const { i18n, t } = useTranslation();
  const setActiveView = useNavigationStore((state) => state.setActiveView);
  const openSettings = useNavigationStore((state) => state.openSettings);
  const clearCurrentProject = useProjectStore((state) => state.clearCurrentProject);
  const showAssessedProjectSelection = useProjectStore((state) => state.showAssessedProjectSelection);
  const currentProject = useProjectStore((state) => state.currentProject);
  const assessProject = useProjectStore((state) => state.assessProject);
  const cancelProjectAssessment = useProjectStore((state) => state.cancelProjectAssessment);
  const openAssessedProject = useProjectStore((state) => state.openAssessedProject);
  const relocateRecentProject = useProjectStore((state) => state.relocateRecentProject);
  const resolveAmbiguousAssessedProject = useProjectStore(
    (state) => state.resolveAmbiguousAssessedProject,
  );
  const clearAmbiguousProjectIntent = useProjectStore((state) => state.clearAmbiguousProjectIntent);
  const recentProjects = useProjectStore((state) => state.recentProjects);
  const removeRecentProject = useProjectStore((state) => state.removeRecentProject);
  const persistPatch = useSettingsStore((state) => state.persistPatch);
  const openPage = useWikiStore((state) => state.openPage);
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<SearchResult[]>([]);
  const [searching, setSearching] = useState(false);
  const [searchOpen, setSearchOpen] = useState(false);
  const [searchError, setSearchError] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const [menuError, setMenuError] = useState<string | null>(null);
  const [relocatingProjectId, setRelocatingProjectId] = useState<string | null>(null);
  const searchInput = useRef<HTMLInputElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const projectButtonRef = useRef<HTMLButtonElement>(null);
  const menuItemRefs = useRef<(HTMLButtonElement | null)[]>([]);
  const pendingMenuFocus = useRef<"first" | "last" | null>(null);
  const requestSequence = useRef(0);
  const activeLanguage = i18n.resolvedLanguage ?? i18n.language;
  const hasProject = Boolean(currentProject.projectId && currentProject.rootPath);

  const isMac = typeof navigator !== "undefined" && navigator.platform?.toLowerCase().includes("mac");
  const kbdLabel = isMac ? "⌘K" : "Ctrl K";
  const settingsKbdHint = isMac ? "⌘," : "Ctrl+,";

  const setLanguage = (language: "en" | "zh-CN") => {
    if (hasProject) {
      void persistPatch(currentProject.projectId, currentProject.rootPath, { language });
      return;
    }
    void i18next.changeLanguage(language);
    try {
      window.localStorage.setItem(LANGUAGE_STORAGE_KEY, language);
    } catch {
      /* localStorage may be unavailable in an embedded preview. */
    }
  };

  useEffect(() => {
    const focusSearch = (event: KeyboardEvent) => {
      if (
        (event.ctrlKey || event.metaKey) &&
        event.key.toLowerCase() === "k" &&
        hasProject &&
        !document.querySelector('[aria-modal="true"]')
      ) {
        event.preventDefault();
        searchInput.current?.focus();
      }
    };
    window.addEventListener("keydown", focusSearch);
    return () => window.removeEventListener("keydown", focusSearch);
  }, [hasProject]);

  useEffect(() => {
    if (!menuOpen) return;
    if (pendingMenuFocus.current) {
      const enabled = menuItemRefs.current.filter(
        (item): item is HTMLButtonElement => Boolean(item && item.dataset.missing !== "true"),
      );
      const target = pendingMenuFocus.current === "last" ? enabled.at(-1) : enabled[0];
      pendingMenuFocus.current = null;
      window.setTimeout(() => target?.focus(), 0);
    }
    const handleClick = (event: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(event.target as Node)) {
        setMenuOpen(false);
      }
    };
    window.addEventListener("mousedown", handleClick);
    return () => window.removeEventListener("mousedown", handleClick);
  }, [menuOpen, recentProjects]);

  const getEnabledMenuItems = () =>
    menuItemRefs.current.filter(
      (item): item is HTMLButtonElement => Boolean(item && item.dataset.missing !== "true"),
    );

  const focusMenuBoundaryItem = (boundary: "first" | "last") => {
    const enabled = getEnabledMenuItems();
    const target = boundary === "last" ? enabled.at(-1) : enabled[0];
    target?.focus();
  };

  const focusRelativeMenuItem = (current: HTMLButtonElement, direction: 1 | -1) => {
    const enabled = getEnabledMenuItems();
    if (enabled.length === 0) return;
    const currentIndex = enabled.indexOf(current);
    const nextIndex =
      currentIndex === -1
        ? direction === 1 ? 0 : enabled.length - 1
        : (currentIndex + direction + enabled.length) % enabled.length;
    enabled[nextIndex]?.focus();
  };

  const closeProjectMenu = () => {
    setMenuError(null);
    projectButtonRef.current?.focus();
    setMenuOpen(false);
  };

  const openRecentProject = async (project: RecentProject) => {
    if (project.missing) return;
    setMenuOpen(false);
    try {
      const assessment = await assessProject(project.rootPath);
      if (canOpenAssessment(assessment)) {
        await openAssessedProject(assessment.assessmentId);
      } else if (
        assessment.format === "ambiguous_markdown"
        && assessment.rememberedOpenIntent === "open_as_markdown_vault"
      ) {
        await resolveAmbiguousAssessedProject(assessment.assessmentId);
      } else {
        showAssessedProjectSelection();
      }
    } catch {
      // Keep the current project open when assessment fails.
    }
  };

  const reassessRecentProject = async (project: RecentProject) => {
    if (project.missing) return;
    setMenuOpen(false);
    try {
      const assessment = await assessProject(project.rootPath);
      if (assessment.format === "ambiguous_markdown") {
        if (assessment.rememberedOpenIntent) {
          await clearAmbiguousProjectIntent(assessment.assessmentId);
        }
        showAssessedProjectSelection();
        return;
      }
      if (canOpenAssessment(assessment)) {
        await openAssessedProject(assessment.assessmentId);
      } else {
        showAssessedProjectSelection();
      }
    } catch {
      // Keep the current project open when assessment or decision clearing fails.
    }
  };

  const removeRecentProjectFromMenu = async (project: RecentProject) => {
    try {
      await removeRecentProject(project.projectId, project.rootPath);
      setMenuOpen(false);
      projectButtonRef.current?.focus();
    } catch {
      // The recent entry remains visible when the backend cannot update global state.
    }
  };

  const relocateMissingRecentProject = async (project: RecentProject) => {
    if (!project.missing || relocatingProjectId) return;
    setMenuError(null);
    try {
      const selected = await pickDirectory({ title: t("shell.projectMenu.relocatePicker") });
      if (!selected) return;
      setRelocatingProjectId(project.projectId);
      const assessment = await assessProject(selected);
      await relocateRecentProject(assessment.assessmentId, project.projectId, project.rootPath);
      setMenuOpen(false);
    } catch (error) {
      await cancelProjectAssessment();
      const errorText = error instanceof Error ? error.message : String(error);
      if (errorText.includes("PROJECT_RELOCATION_SOURCE_AVAILABLE")) {
        setMenuError(t("shell.projectMenu.relocateSourceAvailable", { name: project.name }));
      } else if (
        errorText.includes("PROJECT_RECENT_RELOCATION_NOT_FOUND") ||
        errorText.includes("PROJECT_RECENT_RELOCATION_TARGET_CONFLICT")
      ) {
        setMenuError(t("shell.projectMenu.relocateRecentChanged"));
      } else if (errorText.includes("PROJECT_MUTATION_LOCKED")) {
        setMenuError(t("shell.projectMenu.relocateLocked"));
      } else {
        setMenuError(t("shell.projectMenu.relocateFailed", { name: project.name }));
      }
      setMenuOpen(true);
    } finally {
      setRelocatingProjectId(null);
    }
  };

  const handleProjectButtonKeyDown = (event: ReactKeyboardEvent<HTMLButtonElement>) => {
    if (event.key === "Escape" && menuOpen) {
      event.preventDefault();
      closeProjectMenu();
      return;
    }
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const targetBoundary = event.key === "ArrowUp" ? "last" : "first";
      if (menuOpen) {
        focusMenuBoundaryItem(targetBoundary);
      } else {
        pendingMenuFocus.current = targetBoundary;
        setMenuOpen(true);
      }
    }
  };

  const handleProjectMenuItemKeyDown = (
    event: ReactKeyboardEvent<HTMLButtonElement>,
    onActivate: () => void,
  ) => {
    if (event.key === "Escape") {
      event.preventDefault();
      closeProjectMenu();
      return;
    }
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      focusRelativeMenuItem(event.currentTarget, event.key === "ArrowDown" ? 1 : -1);
      return;
    }
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      onActivate();
    }
  };

  const runSearch = async () => {
    if (!hasProject) return;
    const keyword = query.trim();
    if (!keyword) {
      setResults([]);
      setSearchOpen(false);
      return;
    }
    const sequence = ++requestSequence.current;
    setSearching(true);
    setSearchError(false);
    setSearchOpen(true);
    try {
      const response = await invoke<SearchResponse>("search_wiki", {
        request: {
          projectId: currentProject.projectId,
          projectRootPath: currentProject.rootPath,
          query: keyword,
          pageTypes: [],
          tags: [],
          source: null,
          limit: 20,
        },
      });
      if (sequence === requestSequence.current) setResults(response.results);
    } catch {
      if (sequence === requestSequence.current) {
        setResults([]);
        setSearchError(true);
      }
    } finally {
      if (sequence === requestSequence.current) setSearching(false);
    }
  };

  const openSearchResult = async (result: SearchResult) => {
    setSearchOpen(false);
    setActiveView("wiki");
    await openPage(currentProject.projectId, currentProject.rootPath, result.path);
  };

  return (
    <header className="app-topbar">
      <div className="app-topbar__brand">
        <img alt="" aria-hidden="true" className="app-topbar__logo h-[24px] w-[24px] shrink-0 rounded-[var(--radius-sm)]" src={appLogoUrl} />
        <strong className="app-topbar__brand-name">{t("app.title")}</strong>
      </div>

      <div className="app-topbar__project" ref={menuRef}>
        <button
          ref={projectButtonRef}
          aria-label={t("shell.switchProject")}
          aria-expanded={menuOpen}
          className="app-topbar__project-button"
          onClick={() => {
            setMenuError(null);
            setMenuOpen((v) => !v);
          }}
          onKeyDown={handleProjectButtonKeyDown}
          title={t("shell.switchProject")}
          type="button"
        >
          <FolderOpen aria-hidden="true" className="text-[var(--text-muted)]" size={16} />
          <span className="app-topbar__project-text">
            <span className="app-topbar__project-name">{hasProject ? currentProject.name : t("noProject.switcher")}</span>
            {hasProject ? (
              <span className="app-topbar__project-path" title={currentProject.rootPath}>
                {compactPath(currentProject.rootPath)}
              </span>
            ) : null}
          </span>
          <ChevronDown aria-hidden="true" className="shrink-0 text-[var(--text-muted)]" size={12} />
        </button>
        {menuOpen ? (
          <div className="absolute left-0 top-full z-50 mt-1 w-[360px] rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--background)] p-1 shadow-lg">
            <div className="px-3 py-1.5 text-[10.5px] font-medium uppercase tracking-[0.08em] text-[var(--text-muted)]">
              {t("shell.projectMenu.recent")}
            </div>
            {menuError ? <p className="m-0 px-3 py-1.5 text-[12px] text-[var(--danger)]" role="alert">{menuError}</p> : null}
            {recentProjects.length === 0 ? (
              <p className="m-0 px-3 py-2 text-[12px] text-[var(--text-muted)]">{t("noProject.recent.empty")}</p>
            ) : recentProjects.map((rp, index) => (
              <div className="app-topbar__project-menu-entry" key={`${rp.projectId}:${rp.rootPath}`}>
                <button
                  ref={(node) => {
                    menuItemRefs.current[index * 4] = node;
                  }}
                  aria-disabled={rp.missing ? "true" : undefined}
                  data-missing={rp.missing ? "true" : undefined}
                  tabIndex={rp.missing ? -1 : 0}
                  className={`app-topbar__project-menu-row ${rp.missing ? "is-missing" : ""}`}
                  onClick={() => void openRecentProject(rp)}
                  onKeyDown={(event) => handleProjectMenuItemKeyDown(event, () => void openRecentProject(rp))}
                  type="button"
                >
                  <FolderOpen aria-hidden="true" className="text-[var(--text-muted)]" size={14} />
                  <span className="app-topbar__project-menu-copy">
                    <span className="app-topbar__project-menu-name">{rp.name}</span>
                    <span className="app-topbar__project-menu-path" title={rp.rootPath}>
                      {compactPath(rp.rootPath)}
                    </span>
                  </span>
                  {rp.projectId === currentProject.projectId && rp.rootPath === currentProject.rootPath ? (
                    <span className="shrink-0 rounded-[var(--radius-sm)] bg-[var(--accent-soft)] px-1.5 py-px text-[10px] font-medium text-[var(--accent-hover)]">{t("shell.current")}</span>
                  ) : (
                    <span className="app-topbar__project-menu-meta">
                      {rp.missing ? t("shell.projectMenu.missing") : formatOpenedAt(rp.openedAt)}
                    </span>
                  )}
                </button>
                <button
                  ref={(node) => {
                    menuItemRefs.current[index * 4 + 1] = node;
                  }}
                  aria-label={t("shell.projectMenu.reassess", { name: rp.name })}
                  data-missing={rp.missing ? "true" : undefined}
                  disabled={rp.missing}
                  title={t("shell.projectMenu.reassess", { name: rp.name })}
                  className="app-topbar__project-menu-reset"
                  onClick={() => void reassessRecentProject(rp)}
                  onKeyDown={(event) => handleProjectMenuItemKeyDown(event, () => void reassessRecentProject(rp))}
                  type="button"
                >
                  <RotateCcw aria-hidden="true" size={13} />
                </button>
                {rp.missing ? (
                  <button
                    ref={(node) => {
                      menuItemRefs.current[index * 4 + 2] = node;
                    }}
                    aria-label={t("shell.projectMenu.relocate", { name: rp.name })}
                    disabled={Boolean(relocatingProjectId)}
                    title={t("shell.projectMenu.relocate", { name: rp.name })}
                    className="app-topbar__project-menu-reset"
                    onClick={() => void relocateMissingRecentProject(rp)}
                    onKeyDown={(event) => handleProjectMenuItemKeyDown(event, () => void relocateMissingRecentProject(rp))}
                    type="button"
                  >
                    <FolderSearch aria-hidden="true" size={13} />
                  </button>
                ) : null}
                <button
                  ref={(node) => {
                    menuItemRefs.current[index * 4 + 3] = node;
                  }}
                  aria-label={t("shell.projectMenu.removeRecent", { name: rp.name })}
                  title={t("shell.projectMenu.removeRecent", { name: rp.name })}
                  className="app-topbar__project-menu-reset"
                  onClick={() => void removeRecentProjectFromMenu(rp)}
                  onKeyDown={(event) => handleProjectMenuItemKeyDown(event, () => void removeRecentProjectFromMenu(rp))}
                  type="button"
                >
                  <Trash2 aria-hidden="true" size={13} />
                </button>
              </div>
            ))}
            <div className="my-1 border-t border-[var(--border-subtle)]" />
            <button
              className="flex w-full items-center gap-2 rounded-[var(--radius-sm)] px-3 py-1.5 text-left text-[13px] text-[var(--text-secondary)] hover:bg-[var(--surface-muted)]"
              onClick={() => { setMenuOpen(false); clearCurrentProject(); }}
              type="button"
            >
              <LayoutDashboard aria-hidden="true" className="text-[var(--text-muted)]" size={14} />
              {t("shell.backToWorkspace")}
            </button>
          </div>
        ) : null}
      </div>

      <div className="app-topbar__search-wrap">
        <label className="app-topbar__search">
          <Search aria-hidden="true" size={14} />
          <input
            ref={searchInput}
            aria-label={t("shell.search")}
            className="min-w-0 flex-1 border-0 bg-transparent p-0 text-[13px] outline-none placeholder:text-[var(--text-muted)]"
            placeholder={t("shell.searchPlaceholder")}
            type="search"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") void runSearch();
              if (event.key === "Escape") setSearchOpen(false);
            }}
            disabled={!hasProject}
          />
          <kbd className="rounded border border-[var(--border)] bg-[var(--background)] px-1.5 py-0.5 font-mono text-[10px] text-[var(--text-muted)]">
            {kbdLabel}
          </kbd>
        </label>
        {searchOpen ? (
          <div className="absolute left-0 right-0 top-9 z-50 max-h-[360px] overflow-auto rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--background)] p-1 shadow-lg" role="listbox" aria-label={t("shell.searchResults")}>
            {searching ? <div className="px-3 py-2 text-[12px] text-[var(--text-muted)]">{t("shell.searching")}</div> : searchError ? <div role="alert" className="px-3 py-2 text-[12px] text-[var(--danger)]">{t("shell.searchError")}</div> : results.length === 0 ? <div className="px-3 py-2 text-[12px] text-[var(--text-muted)]">{t("shell.searchNoResults")}</div> : results.map((result) => (
              <button key={result.path} type="button" className="flex w-full items-start justify-between gap-3 rounded-[var(--radius-sm)] px-3 py-2 text-left hover:bg-[var(--surface-muted)]" onClick={() => void openSearchResult(result)}>
                <span><span className="block text-[13px] font-medium">{result.title}</span><span className="block font-mono text-[11px] text-[var(--text-muted)]">{result.path}</span></span>
                <span className="text-[10.5px] uppercase tracking-[0.08em] text-[var(--text-muted)]">{result.pageType}</span>
              </button>
            ))}
          </div>
        ) : null}
      </div>

      <div className="app-topbar__actions">
        <TaskActivityButton />
        <div className="app-topbar__language">
          <button
            aria-pressed={activeLanguage === "zh-CN"}
            className={`rounded-[var(--radius-sm)] px-2 py-0.5 ${
              activeLanguage === "zh-CN" ? "bg-[var(--background)] font-medium" : "text-[var(--text-muted)]"
            }`}
            onClick={() => setLanguage("zh-CN")}
            type="button"
          >
            {t("language.zhCN")}
          </button>
          <button
            aria-pressed={activeLanguage === "en"}
            className={`rounded-[var(--radius-sm)] px-2 py-0.5 ${
              activeLanguage === "en" ? "bg-[var(--background)] font-medium" : "text-[var(--text-muted)]"
            }`}
            onClick={() => setLanguage("en")}
            type="button"
          >
            {t("language.en")}
          </button>
        </div>
        <button
          aria-label={t("nav.settings")}
          aria-keyshortcuts={isMac ? "Meta+Comma" : "Control+Comma"}
          className="icon-button"
          onClick={() => openSettings()}
          title={`${t("nav.settings")} (${settingsKbdHint})`}
          type="button"
        >
          <Settings aria-hidden="true" size={16} />
        </button>
        {hasProject ? (
          <button
            aria-label={t("shell.backToWorkspace")}
            className="icon-button"
            onClick={clearCurrentProject}
            title={t("shell.backToWorkspace")}
            type="button"
          >
            <LayoutDashboard aria-hidden="true" size={16} />
          </button>
        ) : null}
      </div>
    </header>
  );
}
