import { invoke } from "@tauri-apps/api/core";
import { FolderOpen, Search, Settings } from "lucide-react";
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
  const persistPatch = useSettingsStore((state) => state.persistPatch);
  const openPage = useWikiStore((state) => state.openPage);
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<SearchResult[]>([]);
  const [searching, setSearching] = useState(false);
  const [searchOpen, setSearchOpen] = useState(false);
  const [searchError, setSearchError] = useState(false);
  const searchInput = useRef<HTMLInputElement>(null);
  const requestSequence = useRef(0);
  const activeLanguage = i18n.resolvedLanguage ?? i18n.language;

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

      <button
        aria-label={t("shell.switchProject")}
        className="flex h-[30px] min-w-0 max-w-[360px] items-center gap-2 rounded-[var(--radius-md)] px-2 text-left hover:bg-[var(--surface-muted)]"
        onClick={clearCurrentProject}
        title={t("shell.switchProject")}
        type="button"
      >
        <FolderOpen aria-hidden="true" className="text-[var(--text-muted)]" size={16} />
        <span className="truncate text-[13px] font-medium">{currentProject.name}</span>
        <span className="truncate font-mono text-[11px] text-[var(--text-muted)]">{currentProject.rootPath}</span>
      </button>

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
            {t("shell.searchShortcut")}
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
            aria-pressed={activeLanguage === "en"}
            className={`rounded-[var(--radius-sm)] px-2 py-0.5 ${
              activeLanguage === "en" ? "bg-[var(--background)] font-medium" : "text-[var(--text-muted)]"
            }`}
            onClick={() => setLanguage("en")}
            type="button"
          >
            {t("language.en")}
          </button>
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
