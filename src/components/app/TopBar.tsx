import { invoke } from "@tauri-apps/api/core";
import { ChevronDown, FolderOpen, LayoutDashboard, Search, Settings } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigationStore } from "../../stores/navigationStore";
import { useProjectStore } from "../../stores/projectStore";
import { useSettingsStore } from "../../stores/settingsStore";
import { useWikiStore } from "../../features/wiki/wikiStore";
import type { SearchResult, SearchResponse } from "../../types/wiki";
import { TaskActivityButton } from "./TaskActivityButton";

export function TopBar() {
  const { i18n, t } = useTranslation();
  const setActiveView = useNavigationStore((state) => state.setActiveView);
  const clearCurrentProject = useProjectStore((state) => state.clearCurrentProject);
  const currentProject = useProjectStore((state) => state.currentProject);
  const openProject = useProjectStore((state) => state.openProject);
  const recentProjects = useProjectStore((state) => state.recentProjects);
  const persistPatch = useSettingsStore((state) => state.persistPatch);
  const openPage = useWikiStore((state) => state.openPage);
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<SearchResult[]>([]);
  const [searching, setSearching] = useState(false);
  const [searchOpen, setSearchOpen] = useState(false);
  const [searchError, setSearchError] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const searchInput = useRef<HTMLInputElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const requestSequence = useRef(0);
  const activeLanguage = i18n.resolvedLanguage ?? i18n.language;

  const isMac = typeof navigator !== "undefined" && navigator.platform?.toLowerCase().includes("mac");
  const kbdLabel = isMac ? "⌘K" : "Ctrl K";

  const setLanguage = (language: "en" | "zh-CN") => {
    void persistPatch(currentProject.projectId, currentProject.rootPath, { language });
  };

  useEffect(() => {
    const focusSearch = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        searchInput.current?.focus();
      }
    };
    window.addEventListener("keydown", focusSearch);
    return () => window.removeEventListener("keydown", focusSearch);
  }, []);

  useEffect(() => {
    if (!menuOpen) return;
    const handleClick = (event: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(event.target as Node)) {
        setMenuOpen(false);
      }
    };
    window.addEventListener("mousedown", handleClick);
    return () => window.removeEventListener("mousedown", handleClick);
  }, [menuOpen]);

  const runSearch = async () => {
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
    <header className="flex h-12 items-center gap-3 border-b border-[var(--border)] bg-[var(--background)] px-4 text-[13px]">
      <div className="flex h-full items-center gap-2 border-r border-[var(--border-subtle)] pr-3">
        <div className="grid h-[22px] w-[22px] place-items-center rounded-[var(--radius-sm)] bg-[var(--foreground)] font-mono text-[11px] font-semibold text-[var(--text-inverse)]">
          LW
        </div>
        <strong className="text-[13px] font-semibold tracking-[-0.01em] text-[var(--text-primary)]">{t("app.title")}</strong>
      </div>

      <div className="relative" ref={menuRef}>
        <button
          aria-label={t("shell.switchProject")}
          aria-expanded={menuOpen}
          className="flex h-[30px] min-w-0 max-w-[360px] items-center gap-2 rounded-[var(--radius-md)] px-2 text-left hover:bg-[var(--surface-muted)]"
          onClick={() => setMenuOpen((v) => !v)}
          title={t("shell.switchProject")}
          type="button"
        >
          <FolderOpen aria-hidden="true" className="text-[var(--text-muted)]" size={16} />
          <span className="truncate text-[13px] font-medium">{currentProject.name}</span>
          <span className="truncate font-mono text-[11px] text-[var(--text-muted)]">{currentProject.rootPath}</span>
          <ChevronDown aria-hidden="true" className="shrink-0 text-[var(--text-muted)]" size={12} />
        </button>
        {menuOpen ? (
          <div className="absolute left-0 top-full z-50 mt-1 w-[360px] rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--background)] p-1 shadow-lg" role="menu">
            <div className="px-3 py-1.5 text-[10.5px] font-medium uppercase tracking-[0.08em] text-[var(--text-muted)]">
              {t("shell.projectMenu.recent")}
            </div>
            {recentProjects.map((rp) => (
              <button
                key={rp.projectId}
                role="menuitem"
                className="flex w-full items-center gap-2 rounded-[var(--radius-sm)] px-3 py-1.5 text-left text-[13px] hover:bg-[var(--surface-muted)]"
                onClick={() => { setMenuOpen(false); void openProject(rp.rootPath); }}
                type="button"
              >
                <FolderOpen aria-hidden="true" className="text-[var(--text-muted)]" size={14} />
                <span className="truncate font-medium">{rp.name}</span>
                <span className="truncate font-mono text-[11px] text-[var(--text-muted)]">{rp.rootPath}</span>
                {rp.projectId === currentProject.projectId ? (
                  <span className="shrink-0 rounded-[var(--radius-sm)] bg-[var(--accent-soft)] px-1.5 py-px text-[10px] font-medium text-[var(--accent-hover)]">{t("shell.current")}</span>
                ) : null}
              </button>
            ))}
            <div className="my-1 border-t border-[var(--border-subtle)]" />
            <button
              role="menuitem"
              className="flex w-full items-center gap-2 rounded-[var(--radius-sm)] px-3 py-1.5 text-left text-[13px] text-[var(--text-secondary)] hover:bg-[var(--surface-muted)]"
              onClick={() => { setMenuOpen(false); clearCurrentProject(); }}
              type="button"
            >
              <LayoutDashboard aria-hidden="true" className="text-[var(--text-muted)]" size={14} />
              {t("shell.backToLaunch")}
            </button>
          </div>
        ) : null}
      </div>

      <div className="relative flex max-w-[520px] flex-1">
        <label className="flex h-[30px] w-full items-center gap-2 rounded-[var(--radius-md)] bg-[var(--surface-muted)] px-3 text-[13px] text-[var(--text-muted)] focus-within:bg-[var(--background)] focus-within:shadow-[0_0_0_3px_rgba(16,163,127,0.12)]">
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

      <div className="ml-auto flex items-center gap-2">
        <TaskActivityButton />
        <div className="flex h-[26px] items-center rounded-[var(--radius-md)] bg-[var(--surface-muted)] p-0.5 text-[11px]">
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
          className="icon-button"
          onClick={() => setActiveView("settings")}
          title={t("nav.settings")}
          type="button"
        >
          <Settings aria-hidden="true" size={16} />
        </button>
        <button
          aria-label={t("shell.backToLaunch")}
          className="icon-button"
          onClick={clearCurrentProject}
          title={t("shell.backToLaunch")}
          type="button"
        >
          <LayoutDashboard aria-hidden="true" size={16} />
        </button>
      </div>
    </header>
  );
}
