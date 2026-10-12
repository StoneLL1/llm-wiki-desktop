import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Check, Clock, Cpu, History, Monitor, RefreshCw, RotateCcw, Search, Settings as SettingsIcon, Shield, Sun, Wrench, X, type LucideIcon } from "lucide-react";

import type { AgentInfo } from "../../types/agent";
import type { LlmProviderConfig, LlmProviderKind, ProviderStatus, ProviderTestResult } from "../../types/llm";
import type { ProjectSummary } from "../../types/project";
import type { GlobalPreferencesPatch } from "../../types/settings";
import type { SettingsSectionKey } from "../../stores/navigationStore";
import { defaultSettings, useSettingsStore } from "../../stores/settingsStore";
import { observeProjectResources } from "../../stores/projectScope";
import { ActionableErrorNotice } from "../../components/app/ActionableErrorNotice";
import { AiSettings } from "./AiSettings";
import { VersionHistorySettings } from "./VersionHistorySettings";
import { AppearanceSettings } from "./AppearanceSettings";
import { BackgroundTaskSettings } from "./BackgroundTaskSettings";
import { LanguageSettings } from "./LanguageSettings";
import { SecuritySettings, type ProviderSecretRow } from "./SecuritySettings";
import { UpdateSettings } from "./UpdateSettings";
import { ImportCompatibilitySettings } from "./ImportCompatibilitySettings";
import { preferenceIsModified, settingsCatalog, type SettingsCatalogEntry } from "./settingsCatalog";
import type { ImportWorkflow } from "../import/importWorkflow";
import { invalidateNotificationPermissionEpoch, requestNotificationPermissionFromUser } from "../../services/notifications";

export interface SettingsViewProps {
  initialSection?: SettingsSectionKey;
  project: ProjectSummary | null;
  providers: ProviderStatus[];
  agents: AgentInfo[];
  onRefreshCapabilities: () => Promise<void> | void;
  onSaveProvider: (config: LlmProviderConfig) => Promise<unknown> | unknown;
  onSaveSecret: (provider: LlmProviderKind, secret: string) => Promise<unknown> | unknown;
  onDeleteSecret: (provider: LlmProviderKind) => Promise<unknown> | unknown;
  onTestProvider: (config: LlmProviderConfig) => Promise<ProviderTestResult>;
  importWorkflow?: ImportWorkflow;
  onManageProjectAuthority?: () => void;
  onDirtyChange?: (dirty: boolean) => void;
}

const NAV_GROUPS: Array<{ scope: "app" | "project"; items: Array<{ key: SettingsSectionKey; label: string; icon: LucideIcon }> }> = [
  { scope: "app", items: [
    { key: "general", label: "settings.nav.general", icon: SettingsIcon },
    { key: "appearance", label: "settings.nav.appearance", icon: Sun },
    { key: "background", label: "settings.nav.background", icon: Clock },
    { key: "updates", label: "settings.nav.updates", icon: RefreshCw },
  ] },
  { scope: "project", items: [
    { key: "ai", label: "settings.nav.ai", icon: Cpu },
    { key: "versions", label: "versions.title", icon: History },
    { key: "security", label: "settings.nav.security", icon: Shield },
    { key: "compatibility", label: "settings.nav.compatibility", icon: Wrench },
  ] },
];
const providerKinds: readonly LlmProviderKind[] = ["open_ai", "anthropic", "google", "ollama", "custom"];

export function SettingsView({ initialSection = "general", project, providers, agents, onRefreshCapabilities, onSaveProvider, onSaveSecret, onDeleteSecret, onTestProvider, importWorkflow, onManageProjectAuthority, onDirtyChange }: SettingsViewProps) {
  const { t } = useTranslation();
  const [activeSection, setActiveSection] = useState<SettingsSectionKey>(initialSection === "language" ? "general" : initialSection);
  const [aiVisited, setAiVisited] = useState(initialSection === "ai");
  const [query, setQuery] = useState("");
  const [modifiedOnly, setModifiedOnly] = useState(false);
  const [focusTarget, setFocusTarget] = useState<string | null>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  const settings = useSettingsStore((state) => state.settings);
  const loading = useSettingsStore((state) => state.loading);
  const saving = useSettingsStore((state) => state.saving);
  const error = useSettingsStore((state) => state.error);
  const globalLoading = useSettingsStore((state) => state.globalLoading);
  const globalSaving = useSettingsStore((state) => state.globalSaving);
  const globalError = useSettingsStore((state) => state.globalError);
  const chatConvenienceAuthorization = useSettingsStore((state) => state.chatConvenienceAuthorization);
  const loadSettings = useSettingsStore((state) => state.loadSettings);
  const loadGlobalSettings = useSettingsStore((state) => state.loadGlobalSettings);
  const ensureChatConvenienceAuthorization = useSettingsStore((state) => state.ensureChatConvenienceAuthorization);
  const setChatConvenienceAuthorization = useSettingsStore((state) => state.setChatConvenienceAuthorization);
  const revokeAllChatConvenienceAuthorizations = useSettingsStore((state) => state.revokeAllChatConvenienceAuthorizations);
  const persistPatch = useSettingsStore((state) => state.persistPatch);
  const persistGlobalPatch = useSettingsStore((state) => state.persistGlobalPatch);
  const projectId = project?.projectId;
  const rootPath = project?.rootPath;
  const projectScope = NAV_GROUPS[1].items.some((item) => item.key === activeSection);
  const searching = Boolean(query.trim()) || modifiedOnly;

  useEffect(() => {
    void loadGlobalSettings();
    if (!projectId || !rootPath) return;
    const unobserve = observeProjectResources({ projectId, rootPath }, ["settings-chat-authorization"]);
    void loadSettings(projectId, rootPath);
    void ensureChatConvenienceAuthorization(projectId, rootPath);
    return unobserve;
  }, [projectId, rootPath, loadSettings, loadGlobalSettings, ensureChatConvenienceAuthorization]);

  useEffect(() => {
    if (!focusTarget || searching) return;
    const element = contentRef.current?.querySelector<HTMLElement>(`#${focusTarget}`);
    if (element) {
      if (focusTarget === `settings-${activeSection}`) {
        if (contentRef.current) contentRef.current.scrollTop = 0;
      } else element.scrollIntoView?.({ block: "nearest" });
      element.focus({ preventScroll: true });
    }
    setFocusTarget(null);
  }, [focusTarget, searching, activeSection]);

  const saveGlobal = (patch: GlobalPreferencesPatch) => persistGlobalPatch(patch).catch(() => undefined);
  const saveProject = async (patch: Parameters<typeof persistPatch>[2]) => {
    if (!projectId || !rootPath) return;
    await persistPatch(projectId, rootPath, patch);
  };
  const selectSection = (section: SettingsSectionKey, target?: string) => {
    if (section === "ai") setAiVisited(true);
    setActiveSection(section);
    setQuery("");
    setModifiedOnly(false);
    setFocusTarget(target ?? `settings-${section}`);
    if (contentRef.current) contentRef.current.scrollTop = 0;
  };
  const requestPermission = async () => {
    invalidateNotificationPermissionEpoch();
    return requestNotificationPermissionFromUser();
  };
  const resetPreference = (entry: SettingsCatalogEntry) => {
    if (!entry.preference) return;
    if (entry.preference === "aiDefaultRoute") {
      void saveProject({ aiDefaultRoute: null, agentDefault: null }).catch(() => undefined);
    } else {
      void saveGlobal({ [entry.preference]: defaultSettings[entry.preference] });
    }
  };
  const terms = query.trim().toLocaleLowerCase().split(/\s+/).filter(Boolean);
  const results = settingsCatalog.filter((entry) => {
    if (modifiedOnly && !preferenceIsModified(entry, settings, defaultSettings)) return false;
    const text = `${t(entry.label)} ${t(entry.description)} ${entry.keywords}`.toLocaleLowerCase();
    return terms.every((term) => text.includes(term));
  });
  const securityRows: ProviderSecretRow[] = providerKinds.filter((provider) => provider !== "ollama").map((provider) => ({
    provider, hasSecret: Boolean(providers.find((item) => item.config.provider === provider)?.hasSecret),
  }));

  return (
    <div className="settings-view-layout">
      <aside className="settings-view__nav" aria-label={t("settings.nav.label")}>
        <div className="settings-view__nav-head">
          <div className="settings-search-box">
            <Search size={14} aria-hidden="true" />
            <input type="search" aria-label={t("settings.search.label")} placeholder={t("settings.search.placeholder")} value={query} onChange={(event) => setQuery(event.target.value)} />
            {query ? <button type="button" className="icon-button" aria-label={t("settings.search.clear")} onClick={() => setQuery("")}><X size={12} aria-hidden="true" /></button> : null}
          </div>
          <button type="button" className="settings-modified-filter" aria-pressed={modifiedOnly} onClick={() => setModifiedOnly((value) => !value)}><Check size={12} aria-hidden="true" />{t("settings.search.modified")}</button>
        </div>
        <nav className="settings-view__nav-body">
          {NAV_GROUPS.map((group) => <div className="settings-view__nav-group" key={group.scope} role="group" aria-labelledby={`settings-group-${group.scope}`}>
            <div id={`settings-group-${group.scope}`} className="settings-view__nav-group-label">{t(`settings.scope.${group.scope}`)}</div>
            {group.scope === "project" ? <div className="settings-view__nav-project" title={project?.rootPath}>{project?.name ?? t("settings.scope.noProject")}</div> : null}
            {group.items.map(({ key, label, icon: Icon }) => <button key={key} type="button" aria-current={!searching && activeSection === key ? "page" : undefined} className={`settings-view__nav-item${!searching && activeSection === key ? " is-active" : ""}`} onClick={() => selectSection(key)}><Icon aria-hidden="true" size={15} /><span>{t(label)}</span></button>)}
          </div>)}
        </nav>
        <div className="settings-nav-foot"><Monitor size={13} aria-hidden="true" />{t("settings.scope.local")}</div>
      </aside>
      <div className="settings-content-column">
        <div className="settings-view__content" ref={contentRef}>
          <div className="settings-view__content-inner">
            <div className="settings-scope-label">{searching ? t("settings.search.results") : projectScope ? t("settings.scope.projectNamed", { name: project?.name ?? t("settings.scope.noProject") }) : t("settings.scope.allProjects")}</div>
            {globalError ? <ActionableErrorNotice error={globalError} onAction={async () => { await loadGlobalSettings(); }} /> : null}
            {projectScope && error ? <ActionableErrorNotice error={error} onAction={async () => { if (projectId && rootPath) await loadSettings(projectId, rootPath); }} /> : null}
            {searching ? <section className="settings-search-results" aria-label={t("settings.search.results")}>
              <h2 className="settings-view__section-title">{t("settings.search.resultCount", { count: results.length })}</h2>
              {results.length === 0 ? <p className="settings-empty-copy">{t("settings.search.empty")}</p> : results.map((entry) => <div className="settings-search-result" key={entry.id}>
                <button type="button" className="settings-search-result__open" onClick={() => selectSection(entry.section, entry.id)}><strong>{t(entry.label)}</strong><span>{t(entry.description)}</span><small>{t(`settings.scope.${entry.scope}`)}{preferenceIsModified(entry, settings, defaultSettings) ? ` · ${t("settings.search.changed")}` : ""}</small></button>
                {entry.preference && preferenceIsModified(entry, settings, defaultSettings) && (entry.scope === "app" || project) ? <button type="button" className="icon-button" aria-label={t("settings.reset.named", { name: t(entry.label) })} title={t("settings.reset.named", { name: t(entry.label) })} onClick={() => resetPreference(entry)}><RotateCcw size={14} aria-hidden="true" /></button> : null}
              </div>)}
            </section> : null}
            {!searching && projectScope && !project ? <div className="settings-project-empty"><Cpu size={24} aria-hidden="true" /><h2 className="settings-view__section-title">{t("settings.scope.openProjectTitle")}</h2><p>{t("settings.scope.openProjectHelp")}</p></div> : null}
            {!searching && activeSection === "general" ? <section id="settings-general" tabIndex={-1} className="settings-section-panel">
              <div><h2 className="settings-view__section-title">{t("settings.general.title")}</h2><p className="settings-view__section-desc">{t("settings.general.applicationDescription")}</p></div>
              <LanguageSettings language={settings.language} onChange={(language) => { void saveGlobal({ language }); }} />
              <div className="settings-info-row"><div><strong>{t("settings.general.startup")}</strong><p>{t("settings.general.startupHelp")}</p></div></div>
              {project ? <div className="settings-project-info"><strong>{project.name}</strong><code title={project.rootPath}>{project.rootPath}</code>{onManageProjectAuthority ? <button type="button" className="settings-button settings-button--secondary" onClick={onManageProjectAuthority}>{t("settings.general.manageProjectAuthority")}</button> : null}</div> : null}
            </section> : null}
            {!searching && activeSection === "appearance" ? <AppearanceSettings theme={settings.theme} colorThemePreset={settings.colorThemePreset} onChange={(theme) => { void saveGlobal({ theme }); }} onChangeColorThemePreset={(colorThemePreset) => { void saveGlobal({ colorThemePreset }); }} /> : null}
            {project && aiVisited ? <div id="settings-ai" tabIndex={-1} hidden={searching || activeSection !== "ai"}>
              <AiSettings key={`${projectId}\0${rootPath}`} agents={agents} providers={providers} agentDefault={settings.agentDefault} aiDefaultRoute={settings.aiDefaultRoute} contextWindow={settings.contextWindow} onDirtyChange={onDirtyChange}
                onChangeAiDefaultRoute={(aiDefaultRoute) => saveProject({ aiDefaultRoute, agentDefault: aiDefaultRoute?.kind === "agent" ? aiDefaultRoute.agent : null })}
                onRefreshAgents={() => { void onRefreshCapabilities(); }} onChangeDefault={(agentDefault) => { void saveProject({ agentDefault, aiDefaultRoute: agentDefault ? { kind: "agent", agent: agentDefault } : null }).catch(() => undefined); }}
                onSaveProvider={onSaveProvider} onSaveSecret={onSaveSecret} onDeleteSecret={onDeleteSecret} onTestProvider={onTestProvider} />
            </div> : null}
            {!searching && project && activeSection === "versions" ? <div id="settings-versions" tabIndex={-1}><VersionHistorySettings key={`${projectId}\0${rootPath}`} project={project} /></div> : null}
            {!searching && project && activeSection === "security" ? <div id="settings-security" tabIndex={-1}><SecuritySettings providers={securityRows} chatConvenienceAuthorization={chatConvenienceAuthorization} onDeleteSecret={onDeleteSecret} onRevokeChatConvenience={() => setChatConvenienceAuthorization(project.projectId, project.rootPath, false)} onRevokeAllChatConvenience={revokeAllChatConvenienceAuthorizations} /></div> : null}
            {!searching && project && activeSection === "compatibility" ? <div id="settings-compatibility" tabIndex={-1}><ImportCompatibilitySettings workflow={importWorkflow ?? null} /></div> : null}
            {!searching && activeSection === "background" ? <BackgroundTaskSettings closeBehavior={settings.closeBehavior} systemNotifications={settings.systemNotifications} onChangeCloseBehavior={(closeBehavior) => { void saveGlobal({ closeBehavior }); }}
              onChangeSystemNotification={(key, enabled) => persistGlobalPatch({ systemNotifications: { [key]: enabled } }).then(() => { if (enabled) return requestPermission(); })}
              onRequestNotificationPermission={requestPermission} /> : null}
            {!searching && activeSection === "updates" ? <div id="settings-updates" tabIndex={-1}><UpdateSettings /></div> : null}
          </div>
        </div>
        <div className="settings-save-footer" role="status"><span className={`settings-save-dot${globalError ? " is-error" : ""}`} />{globalLoading || loading ? t("settings.state.loading") : globalSaving || saving ? t("settings.state.saving") : globalError ? t("settings.state.saveFailed") : t("settings.state.autoSave")}</div>
      </div>
    </div>
  );
}
