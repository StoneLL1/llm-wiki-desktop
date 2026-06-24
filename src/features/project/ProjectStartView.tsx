import { invoke } from "@tauri-apps/api/core";
import {
  FolderOpen,
  FolderPlus,
  PanelRightClose,
  PanelRightOpen,
  Plus,
  Search,
  Upload,
  X,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import { i18next, LANGUAGE_STORAGE_KEY } from "../../i18n";
import { useProjectStore } from "../../stores/projectStore";
import type { AgentInfo } from "../../types/agent";
import type { ProviderStatus } from "../../types/llm";
import type { ProjectTemplate } from "../../types/project";
import { ConfirmationDialog } from "../../components/app/ConfirmationDialog";
import { useModalDialog } from "../../hooks/useModalDialog";

const TEMPLATES: Array<{ key: ProjectTemplate; titleKey: string; descKey: string }> = [
  { key: "general", titleKey: "launch.template.general", descKey: "launch.template.generalDesc" },
  { key: "research", titleKey: "launch.template.research", descKey: "launch.template.researchDesc" },
  { key: "reading", titleKey: "launch.template.reading", descKey: "launch.template.readingDesc" },
  { key: "personal-growth", titleKey: "launch.template.personal", descKey: "launch.template.personalDesc" },
  { key: "business", titleKey: "launch.template.business", descKey: "launch.template.businessDesc" },
];

const CATEGORIES = ["all", "research", "reading", "personal-growth", "business", "general"] as const;
type Category = (typeof CATEGORIES)[number];

function errorMessage(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error) {
    const message = (error as { message: unknown }).message;
    if (typeof message === "string") return message;
  }
  return String(error);
}

function hasTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function templateOf(entry: { template: ProjectTemplate }): Category {
  return CATEGORIES.includes(entry.template as Category) ? (entry.template as Category) : "general";
}

export function ProjectStartView() {
  const { t, i18n } = useTranslation();
  const recentProjects = useProjectStore((state) => state.recentProjects);
  const initializing = useProjectStore((state) => state.initializing);
  const storeError = useProjectStore((state) => state.error);
  const openProject = useProjectStore((state) => state.openProject);
  const createProject = useProjectStore((state) => state.createProject);
  const pendingAction = useProjectStore((state) => state.pendingAction);
  const confirmPendingAction = useProjectStore((state) => state.confirmPendingAction);
  const cancelPendingAction = useProjectStore((state) => state.cancelPendingAction);

  const [query, setQuery] = useState("");
  const [category, setCategory] = useState<Category>("all");
  const [openPath, setOpenPath] = useState("");
  const [busy, setBusy] = useState(false);
  const [localError, setLocalError] = useState<string | null>(null);
  const [newDialogOpen, setNewDialogOpen] = useState(false);
  const [sideOpen, setSideOpen] = useState(() =>
    typeof window === "undefined" ||
    typeof window.matchMedia !== "function" ||
    !window.matchMedia("(max-width: 1180px)").matches,
  );
  const [agents, setAgents] = useState<AgentInfo[]>([]);
  const [providers, setProviders] = useState<ProviderStatus[]>([]);
  const templatesRequestedRef = useRef(false);

  const activeLanguage = i18n.resolvedLanguage ?? i18n.language;

  const setLanguage = (language: "en" | "zh-CN") => {
    void i18next.changeLanguage(language);
    try {
      window.localStorage.setItem(LANGUAGE_STORAGE_KEY, language);
    } catch {
      /* localStorage may be unavailable in some embed contexts */
    }
  };

  useEffect(() => {
    if (!sideOpen || !templatesRequestedRef.current) return;
    templatesRequestedRef.current = false;
    document.getElementById("launch-templates")?.scrollIntoView({
      behavior: "smooth",
      block: "start",
    });
  }, [sideOpen]);

  useEffect(() => {
    if (!sideOpen) return;
    const closeOnEscape = (event: KeyboardEvent) => {
      if (
        event.key === "Escape" &&
        !event.defaultPrevented &&
        !document.querySelector('[aria-modal="true"]')
      ) {
        setSideOpen(false);
      }
    };
    document.addEventListener("keydown", closeOnEscape);
    return () => document.removeEventListener("keydown", closeOnEscape);
  }, [sideOpen]);

  // Agent / BYOK detection. detect_agents requires a project context, so reuse
  // the most recent project's id/path when available (agent CLI availability is
  // system-wide, so any valid context works). Without a recent project we leave
  // the side panel empty and prompt the user to open a project.
  useEffect(() => {
    if (!hasTauri()) return;
    const ctx = recentProjects[0];
    if (!ctx) return;
    let active = true;
    const request = { projectId: ctx.projectId, projectRootPath: ctx.rootPath };
    Promise.all([
      invoke<AgentInfo[]>("detect_agents", { request }).catch(() => [] as AgentInfo[]),
      invoke<ProviderStatus[]>("list_llm_providers", { request }).catch(() => [] as ProviderStatus[]),
    ]).then(([detected, statuses]) => {
      if (!active) return;
      setAgents(detected);
      setProviders(statuses);
    });
    return () => {
      active = false;
    };
  }, [recentProjects]);

  const filtered = useMemo(() => {
    const keyword = query.trim().toLowerCase();
    return recentProjects.filter((entry) => {
      if (category !== "all" && templateOf(entry) !== category) return false;
      if (!keyword) return true;
      return (
        entry.name.toLowerCase().includes(keyword) ||
        entry.rootPath.toLowerCase().includes(keyword) ||
        entry.template.toLowerCase().includes(keyword)
      );
    });
  }, [recentProjects, query, category]);

  const run = async (operation: () => Promise<unknown>) => {
    setBusy(true);
    setLocalError(null);
    try {
      await operation();
    } catch (error) {
      setLocalError(errorMessage(error));
    } finally {
      setBusy(false);
    }
  };

  const availableAgents = agents.filter((a) => a.state === "installed").length;

  return (
    <div className={`launch ${sideOpen ? "launch--side-open" : ""}`}>
      <header className="launch__top">
        <div className="launch__brand">
          <div className="launch__mark">LW</div>
          <div>
            <div className="launch__brand-name">{t("app.title")}</div>
            <div className="launch__brand-sub">v0.1.0 · {activeLanguage === "zh-CN" ? "zh-CN" : "en"}</div>
          </div>
        </div>
        <nav className="launch__nav" aria-label={t("launch.nav.label")}>
          <button type="button" className="is-active">{t("launch.nav.recent")}</button>
          <button type="button" onClick={() => setNewDialogOpen(true)}>{t("launch.nav.new")}</button>
          <button type="button" onClick={() => {
            const el = document.getElementById("launch-open-input");
            el?.focus();
          }}>{t("launch.nav.open")}</button>
          <button type="button" onClick={() => {
            if (sideOpen) {
              document.getElementById("launch-templates")?.scrollIntoView({ behavior: "smooth", block: "start" });
            } else {
              templatesRequestedRef.current = true;
              setSideOpen(true);
            }
          }}>{t("launch.nav.templates")}</button>
        </nav>
        <div className="launch__top-actions">
          <div className="launch__langswitch" role="group" aria-label={t("settings.language.title")}>
            <button type="button" className={activeLanguage === "zh-CN" ? "is-active" : ""} onClick={() => setLanguage("zh-CN")}>中</button>
            <button type="button" className={activeLanguage === "en" ? "is-active" : ""} onClick={() => setLanguage("en")}>EN</button>
          </div>
          <button
            type="button"
            className="btn btn--ghost btn--icon btn--sm"
            title={t(sideOpen ? "launch.side.collapse" : "launch.side.open")}
            aria-label={t(sideOpen ? "launch.side.collapse" : "launch.side.open")}
            aria-controls="launch-info-panel"
            aria-expanded={sideOpen}
            onClick={() => setSideOpen((value) => !value)}
          >
            {sideOpen ? (
              <PanelRightClose size={16} aria-hidden="true" />
            ) : (
              <PanelRightOpen size={16} aria-hidden="true" />
            )}
          </button>
        </div>
      </header>

      <div className="launch__body">
        <main className="launch__main">
          <div className="launch__hero">
            <h1 className="launch__hero-title">{t("launch.hero.title")}</h1>
            <p className="launch__hero-sub">{t("launch.hero.sub")}</p>
          </div>

          <div className="launch__filterbar">
            <label className="launch__search">
              <Search size={16} aria-hidden="true" style={{ color: "var(--text-muted)" }} />
              <input
                type="search"
                placeholder={t("launch.search.placeholder")}
                aria-label={t("launch.search.label")}
                value={query}
                onChange={(event) => setQuery(event.target.value)}
              />
              <span className="kbd">⌘K</span>
            </label>
            <div className="launch__filter-pills">
              {CATEGORIES.map((cat) => (
                <button
                  key={cat}
                  type="button"
                  className={`pill pill--hover ${category === cat ? "pill--active" : ""}`}
                  onClick={() => setCategory(cat)}
                >
                  {t(`launch.filter.${cat}`)}
                </button>
              ))}
            </div>
          </div>

          <div className="projgrid">
            {/* Quick actions */}
            <button type="button" className="quickaction" onClick={() => setNewDialogOpen(true)}>
              <span className="quickaction__icon"><FolderPlus size={20} aria-hidden="true" /></span>
              <h3 className="quickaction__title">{t("launch.quick.new")}</h3>
              <p className="quickaction__desc">{t("launch.quick.newDesc")}</p>
            </button>
            <label className="quickaction" htmlFor="launch-open-input">
              <span className="quickaction__icon"><FolderOpen size={20} aria-hidden="true" /></span>
              <h3 className="quickaction__title">{t("launch.quick.open")}</h3>
              <p className="quickaction__desc">{t("launch.quick.openDesc")}</p>
            </label>
            <div className="quickaction" role="note" aria-label={t("launch.quick.import")}>
              <span className="quickaction__icon"><Upload size={20} aria-hidden="true" /></span>
              <h3 className="quickaction__title">{t("launch.quick.import")}</h3>
              <p className="quickaction__desc">{t("launch.quick.importDesc")}</p>
            </div>

            {/* Open-folder inline input (drives the "open folder as project" quick action) */}
            <form
              className="quickaction"
              style={{ display: "flex", flexDirection: "column", gap: "var(--sp-2)", cursor: "default" }}
              onSubmit={(event) => {
                event.preventDefault();
                if (openPath.trim()) void run(() => openProject(openPath.trim()));
              }}
            >
              <label className="quickaction__title" htmlFor="launch-open-input" style={{ fontSize: 13, fontWeight: 600 }}>
                {t("launch.open.title")}
              </label>
              <input
                id="launch-open-input"
                className="input input--mono"
                value={openPath}
                onChange={(event) => setOpenPath(event.target.value)}
                placeholder={t("launch.open.placeholder")}
                disabled={initializing || busy}
              />
              <button type="submit" className="btn btn--sm btn--primary" disabled={initializing || busy || !openPath.trim()}>
                {t("launch.open.action")}
              </button>
            </form>

            {/* Recent project cards */}
            {filtered.length === 0 ? (
              <div className="empty" style={{ gridColumn: "1 / -1" }}>
                <div className="empty__title">{t("launch.empty.title")}</div>
                <div className="empty__desc">{initializing ? t("launch.empty.loading") : t("launch.empty.desc")}</div>
              </div>
            ) : (
              filtered.map((entry) => (
                <button
                  key={`${entry.projectId}:${entry.rootPath}`}
                  type="button"
                  className="projcard"
                  disabled={initializing || busy}
                  onClick={() => void run(() => openProject(entry.rootPath))}
                >
                  <div className="projcard__head">
                    <div className="projcard__mark">{entry.name.slice(0, 1).toUpperCase()}</div>
                    <div style={{ flex: 1, minWidth: 0 }}>
                      <h3 className="projcard__title">{entry.name}</h3>
                      <div className="projcard__path">{entry.rootPath}</div>
                    </div>
                  </div>
                  <div className="projcard__meta">
                    <span className="pill pill--active">{t(`launch.filter.${templateOf(entry)}`)}</span>
                    <span>·</span>
                    <span>{relativeTime(entry.openedAt, t)}</span>
                  </div>
                </button>
              ))
            )}
          </div>

          {localError || storeError ? (
            <p role="alert" className="mt-4 text-[12px] text-[var(--danger)]">{localError ?? storeError}</p>
          ) : null}
        </main>

        {sideOpen ? (
          <button
            aria-hidden="true"
            className="launch__side-backdrop"
            onClick={() => setSideOpen(false)}
            tabIndex={-1}
            type="button"
          />
        ) : null}
        {sideOpen ? <aside id="launch-info-panel" className="launch__side" aria-label={t("launch.side.label")}>
          <h3>{t("launch.side.agents")}</h3>
          {agents.length === 0 ? (
            <p className="m-0 mb-2 text-[11.5px] leading-5 text-[var(--text-muted)]">{t("launch.side.agentsHint")}</p>
          ) : (
            agents.map((agent) => (
              <AgentMini
                key={agent.kind}
                name={agent.command}
                state={agent.state}
                version={agent.version}
                path={agent.executablePath}
                isDefault={agent.isDefault}
                guidance={agent.installGuidance}
              />
            ))
          )}

          <h3 style={{ marginTop: 24 }}>{t("launch.side.byok")}</h3>
          {providers.length === 0 ? (
            <p className="m-0 mb-2 text-[11.5px] leading-5 text-[var(--text-muted)]">{t("launch.side.byokHint")}</p>
          ) : (
            providers
              .filter((p) => p.config.enabled)
              .map((provider) => (
                <AgentMini
                  key={provider.config.provider}
                  name={labelForProvider(provider.config.provider)}
                  state={provider.hasSecret || provider.config.provider === "ollama" ? "installed" : "missing"}
                  version={provider.config.model || t("launch.side.unconfigured")}
                  hint={provider.hasSecret ? t("launch.side.keyConfigured") : t("launch.side.byokHint")}
                />
              ))
          )}

          <h3 id="launch-templates" style={{ marginTop: 24 }}>{t("launch.side.templates")}</h3>
          {TEMPLATES.map((template) => (
            <button
              key={template.key}
              type="button"
              className="templateside"
              style={{ width: "100%", textAlign: "left" }}
              onClick={() => {
                setNewDialogOpen(true);
              }}
            >
              <div className="templateside__title">{t(template.titleKey)}</div>
              <div className="templateside__desc">{t(template.descKey)}</div>
            </button>
          ))}

          <div style={{ marginTop: 24, padding: 12, background: "var(--surface-muted)", borderRadius: "var(--radius-md)", fontSize: 11.5, color: "var(--text-muted)", lineHeight: 1.6 }}>
            <strong style={{ color: "var(--text-secondary)" }}>{t("launch.tip.label")}</strong> · {t("launch.tip.body")}
          </div>
        </aside> : null}
      </div>

      <footer className="launch__bottom">
        <span className="dotstatus dotstatus--ok" aria-hidden="true" />
        <span>{t("launch.bottom.ready")}</span>
        <span>·</span>
        <span>{t("launch.bottom.agents", { count: availableAgents })}</span>
        {providers.some((p) => p.config.enabled && (p.hasSecret || p.config.provider === "ollama")) ? (
          <>
            <span>·</span>
            <span>{t("launch.bottom.byokReady")}</span>
          </>
        ) : null}
      </footer>

      {newDialogOpen ? (
        <NewProjectDialog
          onClose={() => setNewDialogOpen(false)}
          onCreate={(payload) => {
            void run(async () => {
              await createProject({ rootPath: payload.rootPath, name: payload.name, template: payload.template });
              setNewDialogOpen(false);
            });
          }}
          busy={busy}
        />
      ) : null}

      {pendingAction ? (
        <ConfirmationDialog
          action={pendingAction}
          checkpointExists={false}
          onCancel={() => void run(cancelPendingAction)}
          onConfirm={() => void run(confirmPendingAction)}
        />
      ) : null}
    </div>
  );
}

function AgentMini({
  name,
  state,
  version,
  path,
  isDefault,
  guidance,
  hint,
}: {
  name: string;
  state: "installed" | "missing" | "failed";
  version?: string | null;
  path?: string | null;
  isDefault?: boolean;
  guidance?: string;
  hint?: string;
}) {
  const { t } = useTranslation();
  const ok = state === "installed";
  return (
    <div className="agentmini">
      <div className="agentmini__row">
        <span className={`dotstatus ${ok ? "dotstatus--ok" : "dotstatus--err"}`} aria-hidden="true" />
        <span className="agentmini__name">{name}</span>
        <span className="agentmini__ver">{ok ? (version ? `v${version}` : "") : t("launch.side.notDetected")}</span>
        {isDefault ? (
          <span className="badge badge--accent" style={{ marginLeft: "auto" }}>{t("launch.side.default")}</span>
        ) : null}
      </div>
      <div className="agentmini__hint">{ok ? (path ?? hint ?? "") : (guidance ?? hint ?? "")}</div>
    </div>
  );
}

function labelForProvider(provider: ProviderStatus["config"]["provider"]): string {
  switch (provider) {
    case "open_ai":
      return "OpenAI";
    case "anthropic":
      return "Anthropic";
    case "google":
      return "Google";
    case "ollama":
      return "Ollama";
    case "custom":
      return "Custom";
  }
}

function NewProjectDialog({
  onClose,
  onCreate,
  busy,
}: {
  onClose: () => void;
  onCreate: (payload: { rootPath: string; name: string; template: ProjectTemplate; initGit: boolean }) => void;
  busy: boolean;
}) {
  const { t } = useTranslation();
  const [name, setName] = useState("");
  const [rootPath, setRootPath] = useState("");
  const [template, setTemplate] = useState<ProjectTemplate>("general");
  const [initGit, setInitGit] = useState(true);
  const nameRef = useRef<HTMLInputElement>(null);
  const dialogRef = useModalDialog({ onClose, initialFocusRef: nameRef });

  const submit = (event: React.FormEvent) => {
    event.preventDefault();
    if (!name.trim() || !rootPath.trim()) return;
    onCreate({ rootPath: rootPath.trim(), name: name.trim(), template, initGit });
  };

  return (
    <div
      ref={dialogRef}
      tabIndex={-1}
      className="fixed inset-0 z-[100] grid place-items-center bg-black/40 p-4"
      role="dialog"
      aria-modal="true"
      aria-labelledby="new-project-title"
      onClick={onClose}
    >
      <form
        onClick={(event) => event.stopPropagation()}
        onSubmit={submit}
        className="dialog--wide w-[640px] max-w-full rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--background)] shadow-xl"
        style={{ background: "var(--background)" }}
      >
        <div className="flex items-center justify-between border-b border-[var(--border-subtle)] px-5 py-3">
          <h2 id="new-project-title" className="m-0 text-[15px] font-semibold">{t("launch.dialog.title")}</h2>
          <button type="button" aria-label={t("launch.dialog.close")} className="btn btn--ghost btn--icon btn--sm" onClick={onClose}>
            <X size={16} aria-hidden="true" />
          </button>
        </div>

        <div className="px-5 py-4">
          <div className="formrow">
            <div>
              <div className="formrow__label">{t("launch.dialog.name")}</div>
              <div className="formrow__hint">{t("launch.dialog.nameHint")}</div>
            </div>
            <div className="formrow__control">
              <input ref={nameRef} className="input" value={name} onChange={(event) => setName(event.target.value)} />
            </div>
          </div>

          <div className="formrow">
            <div>
              <div className="formrow__label">{t("launch.dialog.location")}</div>
              <div className="formrow__hint">{t("launch.dialog.locationHint")}</div>
            </div>
            <div className="formrow__control">
              <div className="input-group">
                <span className="input-group__lead"><FolderOpen size={14} aria-hidden="true" /></span>
                <input className="input input--mono" value={rootPath} onChange={(event) => setRootPath(event.target.value)} placeholder={t("launch.dialog.locationPlaceholder")} />
              </div>
            </div>
          </div>

          <div className="formrow">
            <div>
              <div className="formrow__label">{t("launch.dialog.template")}</div>
              <div className="formrow__hint">{t("launch.dialog.templateHint")}</div>
            </div>
            <div className="formrow__control">
              <div className="seg" role="group" aria-label={t("launch.dialog.template")}>
                {TEMPLATES.map((entry) => (
                  <button
                    key={entry.key}
                    type="button"
                    className={template === entry.key ? "is-active" : ""}
                    onClick={() => setTemplate(entry.key)}
                  >
                    {t(entry.titleKey)}
                  </button>
                ))}
              </div>
            </div>
          </div>

          <div className="formrow">
            <div>
              <div className="formrow__label">{t("launch.dialog.git")}</div>
              <div className="formrow__hint">{t("launch.dialog.gitHint")}</div>
            </div>
            <div className="formrow__control">
              <label className="checkbox">
                <input type="checkbox" checked={initGit} onChange={(event) => setInitGit(event.target.checked)} />
                {t("launch.dialog.gitLabel")}
              </label>
            </div>
          </div>
        </div>

        <div className="flex items-center justify-end gap-2 border-t border-[var(--border-subtle)] px-5 py-3">
          <button type="button" className="btn" onClick={onClose}>{t("launch.dialog.cancel")}</button>
          <button type="submit" className="btn btn--primary" disabled={busy || !name.trim() || !rootPath.trim()}>
            <Plus size={14} aria-hidden="true" />
            {t("launch.dialog.create")}
          </button>
        </div>
      </form>
    </div>
  );
}

function relativeTime(iso: string | null, t: (key: string, opts?: Record<string, unknown>) => string): string {
  if (!iso) return "";
  const then = new Date(iso).getTime();
  if (Number.isNaN(then)) return "";
  const min = Math.floor((Date.now() - then) / 60000);
  if (min < 1) return t("relative.justNow");
  if (min < 60) return t("relative.minutesAgo", { count: min });
  const hours = Math.floor(min / 60);
  if (hours < 24) return t("relative.hoursAgo", { count: hours });
  const days = Math.floor(hours / 24);
  if (days < 7) return t("relative.daysAgo", { count: days });
  return new Date(iso).toLocaleDateString();
}
