import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  Clock,
  Cpu,
  Globe,
  RefreshCw,
  Settings as SettingsIcon,
  Shield,
  Sun,
  Wrench,
  type LucideIcon,
} from "lucide-react";

import type { AgentInfo } from "../../types/agent";
import type { LlmProviderConfig, LlmProviderKind, ProviderStatus, ProviderTestResult } from "../../types/llm";
import type { ProjectSummary } from "../../types/project";
import type { SettingsSectionKey } from "../../stores/navigationStore";
import { useSettingsStore } from "../../stores/settingsStore";
import { observeProjectResources } from "../../stores/projectScope";
import { AiSettings } from "./AiSettings";
import { AppearanceSettings } from "./AppearanceSettings";
import { BackgroundTaskSettings } from "./BackgroundTaskSettings";
import { LanguageSettings } from "./LanguageSettings";
import { SecuritySettings, type ProviderSecretRow } from "./SecuritySettings";
import { UpdateSettings } from "./UpdateSettings";
import { ImportCompatibilitySettings } from "./ImportCompatibilitySettings";
import type { ImportWorkflow } from "../import/importWorkflow";
import {
  invalidateNotificationPermissionEpoch,
  requestNotificationPermissionFromUser,
} from "../../services/notifications";

export interface SettingsViewProps {
  initialSection?: SettingsSectionKey;
  project: ProjectSummary;
  providers: ProviderStatus[];
  agents: AgentInfo[];
  onRefreshCapabilities: () => Promise<void> | void;
  onSaveProvider: (config: LlmProviderConfig) => Promise<unknown> | unknown;
  onSaveSecret: (provider: LlmProviderKind, secret: string) => Promise<unknown> | unknown;
  onDeleteSecret: (provider: LlmProviderKind) => Promise<unknown> | unknown;
  onTestProvider: (config: LlmProviderConfig) => Promise<ProviderTestResult>;
  importWorkflow?: ImportWorkflow;
  onManageProjectAuthority?: () => void;
}

interface SettingsNavItem {
  key: SettingsSectionKey;
  labelKey: string;
  icon: LucideIcon;
}

interface SettingsNavGroup {
  labelKey: string;
  items: SettingsNavItem[];
}

// Settings IA mirrors UI-Frontend-design/settings.html: three labelled groups
// (Application / AI / System) instead of a flat 8-item list.
const NAV_GROUPS: SettingsNavGroup[] = [
  {
    labelKey: "settings.nav.group.app",
    items: [
      { key: "general", labelKey: "settings.nav.general", icon: SettingsIcon },
      { key: "appearance", labelKey: "settings.nav.appearance", icon: Sun },
      { key: "language", labelKey: "settings.nav.language", icon: Globe },
    ],
  },
  {
    labelKey: "settings.nav.group.ai",
    items: [
      { key: "ai", labelKey: "settings.nav.ai", icon: Cpu },
    ],
  },
  {
    labelKey: "settings.nav.group.system",
    items: [
      { key: "security", labelKey: "settings.nav.security", icon: Shield },
      { key: "compatibility", labelKey: "settings.nav.compatibility", icon: Wrench },
      { key: "background", labelKey: "settings.nav.background", icon: Clock },
      { key: "updates", labelKey: "settings.nav.updates", icon: RefreshCw },
    ],
  },
];

const navGroupId = (labelKey: string): string => `settings-nav-group-${labelKey.replace(/\./g, "-")}`;

const hasTauri = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const providerKinds: readonly LlmProviderKind[] = ["open_ai", "anthropic", "google", "ollama", "custom"];

export function SettingsView({
  initialSection = "general",
  project,
  providers,
  agents,
  onRefreshCapabilities,
  onSaveProvider,
  onSaveSecret,
  onDeleteSecret,
  onTestProvider,
  importWorkflow,
  onManageProjectAuthority,
}: SettingsViewProps) {
  const { t } = useTranslation();
  const [activeSection, setActiveSection] = useState<SettingsSectionKey>(initialSection);
  const [secretStatus, setSecretStatus] = useState<Record<LlmProviderKind, string | null>>({
    open_ai: null,
    anthropic: null,
    google: null,
    ollama: null,
    custom: null,
  });
  const settings = useSettingsStore((state) => state.settings);
  const loading = useSettingsStore((state) => state.loading);
  const saving = useSettingsStore((state) => state.saving);
  const error = useSettingsStore((state) => state.error);
  const chatConvenienceAuthorization = useSettingsStore((state) => state.chatConvenienceAuthorization);
  const loadSettings = useSettingsStore((state) => state.loadSettings);
  const ensureChatConvenienceAuthorization = useSettingsStore((state) => state.ensureChatConvenienceAuthorization);
  const setChatConvenienceAuthorization = useSettingsStore((state) => state.setChatConvenienceAuthorization);
  const revokeAllChatConvenienceAuthorizations = useSettingsStore(
    (state) => state.revokeAllChatConvenienceAuthorizations,
  );
  const persistPatch = useSettingsStore((state) => state.persistPatch);
  const notificationProjectKey = `${project.projectId}\0${project.rootPath}`;
  const notificationSaveTailRef = useRef<Promise<unknown>>(Promise.resolve());
  const notificationScopeRef = useRef({ key: notificationProjectKey, generation: 0 });
  if (notificationScopeRef.current.key !== notificationProjectKey) {
    notificationScopeRef.current = {
      key: notificationProjectKey,
      generation: notificationScopeRef.current.generation + 1,
    };
    notificationSaveTailRef.current = Promise.resolve();
  }

  useEffect(() => {
    const unobserve = observeProjectResources(
      { projectId: project.projectId, rootPath: project.rootPath },
      ["settings-chat-authorization"],
    );
    void loadSettings(project.projectId, project.rootPath);
    void ensureChatConvenienceAuthorization(project.projectId, project.rootPath);
    return unobserve;
  }, [project.projectId, project.rootPath, loadSettings, ensureChatConvenienceAuthorization]);

  useEffect(() => {
    if (!hasTauri()) return;
    void Promise.all(
      providerKinds.map(async (provider) => {
        const mask = await invoke<string | null>("get_provider_secret_status", {
          request: { provider },
        });
        return [provider, mask] as const;
      }),
    )
      .then((entries) => {
        setSecretStatus(Object.fromEntries(entries) as Record<LlmProviderKind, string | null>);
      })
      .catch(() => undefined);
  }, [providers]);

  const savePatch = async (patch: Partial<typeof settings>, refreshCapabilities = false) => {
    await persistPatch(project.projectId, project.rootPath, patch);
    if (refreshCapabilities) {
      await onRefreshCapabilities();
    }
  };

  const requestNotificationPermission = async () => {
    invalidateNotificationPermissionEpoch();
    await requestNotificationPermissionFromUser();
  };

  const providerStatuses = providers.map((provider) => ({
    ...provider,
    secretMask: secretStatus[provider.config.provider] ?? provider.secretMask,
  }));
  const securityRows: ProviderSecretRow[] = providerKinds.map((provider) => {
    const configured = providerStatuses.find((item) => item.config.provider === provider);
    const secretMask = secretStatus[provider] ?? configured?.secretMask ?? null;
    return {
      provider,
      hasSecret: Boolean(secretMask) || Boolean(configured?.hasSecret),
    };
  });

  return (
    <div className="settings-view-layout">
      <aside className="settings-view__nav" aria-label={t("settings.nav.label")}>
        <div className="settings-view__nav-head">
          <div className="settings-view__nav-label">{t("settings.nav.label")}</div>
          <div className="settings-view__nav-project" title={project.rootPath}>{project.name}</div>
        </div>
        <nav className="settings-view__nav-body">
          {NAV_GROUPS.map((group) => (
            <div className="settings-view__nav-group" key={group.labelKey} role="group" aria-labelledby={navGroupId(group.labelKey)}>
              <div id={navGroupId(group.labelKey)} className="settings-view__nav-group-label">{t(group.labelKey)}</div>
              {group.items.map((item) => {
                const Icon = item.icon;
                const active = activeSection === item.key;
                return (
                  <button
                    key={item.key}
                    type="button"
                    aria-current={active ? "true" : undefined}
                    className={`settings-view__nav-item${active ? " is-active" : ""}`}
                    onClick={() => setActiveSection(item.key)}
                  >
                    <Icon aria-hidden="true" className="settings-view__nav-icon" size={14} />
                    <span className="truncate">{t(item.labelKey)}</span>
                  </button>
                );
              })}
            </div>
          ))}
        </nav>
      </aside>

      <div className="settings-view__content">
        <div className="settings-view__content-inner">
          {loading || saving ? (
            <div className="settings-view__status" role="status">
              {loading ? <span>{t("settings.state.loading")}</span> : null}
              {saving ? <span>{t("settings.state.saving")}</span> : null}
            </div>
          ) : null}

          {error ? (
            <div className="settings-view__error flex items-center justify-between gap-3" role="alert">
              <span>{error}</span>
              <button
                type="button"
                className="btn btn--secondary btn--sm"
                onClick={() => {
                  void loadSettings(project.projectId, project.rootPath);
                  void ensureChatConvenienceAuthorization(project.projectId, project.rootPath);
                }}
              >
                {t("workflows.action.retry")}
              </button>
            </div>
          ) : null}

          {activeSection === "general" ? (
            <section className="settings-view__section">
              <div>
                <h2 className="settings-view__section-title">{t("settings.general.title")}</h2>
                <p className="settings-view__section-desc">{t("settings.general.description")}</p>
              </div>
              <div className="settings-view__cards">
                <div className="settings-view__card">
                  <div className="settings-view__card-label">{t("settings.general.projectRoot")}</div>
                  <div className="settings-view__card-value settings-view__card-value--mono">{project.rootPath}</div>
                </div>
                <div className="settings-view__card">
                  <div className="settings-view__card-label">{t("settings.general.scope")}</div>
                  <div className="settings-view__card-value">{t("settings.general.scopeCopy")}</div>
                </div>
                {onManageProjectAuthority ? (
                  <div className="settings-view__card">
                    <div className="settings-view__card-label">{t("settings.general.projectAuthority")}</div>
                    <div className="settings-view__card-value">{t("settings.general.projectAuthorityDescription")}</div>
                    <button className="btn btn--secondary mt-3" onClick={onManageProjectAuthority} type="button">
                      {t("settings.general.manageProjectAuthority")}
                    </button>
                  </div>
                ) : null}
              </div>
            </section>
          ) : null}

          {activeSection === "appearance" ? (
            <AppearanceSettings
              theme={settings.theme}
              colorThemePreset={settings.colorThemePreset}
              onChange={(theme) => void savePatch({ theme })}
              onChangeColorThemePreset={(colorThemePreset) => void savePatch({ colorThemePreset })}
            />
          ) : null}

          {activeSection === "language" ? (
            <LanguageSettings language={settings.language} onChange={(language) => void savePatch({ language })} />
          ) : null}

          {activeSection === "ai" ? (
            <AiSettings
              agents={agents}
              providers={providerStatuses}
              agentDefault={settings.agentDefault}
              contextWindow={settings.contextWindow}
              onRefreshAgents={() => { void onRefreshCapabilities(); }}
              onChangeDefault={(agentDefault) => { void savePatch({ agentDefault }, true); }}
              onSaveProvider={(config) => onSaveProvider({ ...config, contextWindow: settings.contextWindow })}
              onSaveSecret={async (provider, secret) => {
                await onSaveSecret(provider, secret);
              }}
              onDeleteSecret={async (provider) => {
                await onDeleteSecret(provider);
              }}
              onTestProvider={(config) => onTestProvider({ ...config, contextWindow: settings.contextWindow })}
            />
          ) : null}

          {activeSection === "security" ? (
            <SecuritySettings
              providers={securityRows}
              chatConvenienceAuthorization={chatConvenienceAuthorization}
              onDeleteSecret={(provider) => {
                void (async () => {
                  await onDeleteSecret(provider);
                })();
              }}
              onRevokeChatConvenience={() => {
                void setChatConvenienceAuthorization(project.projectId, project.rootPath, false);
              }}
              onRevokeAllChatConvenience={() => {
                void revokeAllChatConvenienceAuthorizations();
              }}
            />
          ) : null}

          {activeSection === "compatibility" ? (
            <ImportCompatibilitySettings workflow={importWorkflow ?? null} />
          ) : null}

          {activeSection === "background" ? (
            <BackgroundTaskSettings
              closeBehavior={settings.closeBehavior}
              contextWindow={settings.contextWindow}
              systemNotifications={settings.systemNotifications}
              onChangeCloseBehavior={(closeBehavior) => { void savePatch({ closeBehavior }); }}
              onChangeContextWindow={(contextWindow) => { void savePatch({ contextWindow }); }}
              onChangeSystemNotification={(key, enabled) => {
                const owner = { ...notificationScopeRef.current };
                const save = notificationSaveTailRef.current.then(async () => {
                  if (
                    notificationScopeRef.current.key !== owner.key
                    || notificationScopeRef.current.generation !== owner.generation
                  ) return false;
                  const latest = useSettingsStore.getState().settings.systemNotifications;
                  await savePatch({
                    systemNotifications: { ...latest, [key]: enabled },
                  });
                  return notificationScopeRef.current.key === owner.key
                    && notificationScopeRef.current.generation === owner.generation;
                });
                notificationSaveTailRef.current = save.catch(() => undefined);
                if (enabled) void save.then((saved) => {
                  if (saved) return requestNotificationPermission();
                  return undefined;
                }, () => undefined);
              }}
              onRequestNotificationPermission={() => { void requestNotificationPermission(); }}
            />
          ) : null}

          {activeSection === "updates" ? (
            <UpdateSettings checkUpdates={settings.checkUpdates} onToggle={(checkUpdates) => { void savePatch({ checkUpdates }); }} />
          ) : null}
        </div>
      </div>
    </div>
  );
}
