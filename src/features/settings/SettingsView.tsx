import { invoke } from "@tauri-apps/api/core";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";

import type { AgentInfo } from "../../types/agent";
import type { LlmProviderConfig, LlmProviderKind, ProviderStatus, ProviderTestResult } from "../../types/llm";
import type { ProjectSummary } from "../../types/project";
import { useSettingsStore } from "../../stores/settingsStore";
import { AgentSettings } from "./AgentSettings";
import { AppearanceSettings } from "./AppearanceSettings";
import { BackgroundTaskSettings } from "./BackgroundTaskSettings";
import { LanguageSettings } from "./LanguageSettings";
import { LlmProviderSettings } from "./LlmProviderSettings";
import { SecuritySettings, type ProviderSecretRow } from "./SecuritySettings";
import { UpdateSettings } from "./UpdateSettings";

interface SettingsViewProps {
  project: ProjectSummary;
  providers: ProviderStatus[];
  agents: AgentInfo[];
  onRefreshCapabilities: () => Promise<void> | void;
  onSaveProvider: (config: LlmProviderConfig) => Promise<unknown> | unknown;
  onSaveSecret: (provider: LlmProviderKind, secret: string) => Promise<unknown> | unknown;
  onDeleteSecret: (provider: LlmProviderKind) => Promise<unknown> | unknown;
  onTestProvider: (config: LlmProviderConfig) => Promise<ProviderTestResult>;
}

type SettingsSectionKey =
  | "general"
  | "appearance"
  | "language"
  | "agent"
  | "providers"
  | "security"
  | "background"
  | "updates";

const hasTauri = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const providerKinds: readonly LlmProviderKind[] = ["open_ai", "anthropic", "google", "ollama", "custom"];

export function SettingsView({
  project,
  providers,
  agents,
  onRefreshCapabilities,
  onSaveProvider,
  onSaveSecret,
  onDeleteSecret,
  onTestProvider,
}: SettingsViewProps) {
  const { t } = useTranslation();
  const [activeSection, setActiveSection] = useState<SettingsSectionKey>("general");
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
  const loadSettings = useSettingsStore((state) => state.loadSettings);
  const persistPatch = useSettingsStore((state) => state.persistPatch);

  useEffect(() => {
    void loadSettings(project.projectId, project.rootPath);
  }, [project.projectId, project.rootPath, loadSettings]);

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

  const sections = useMemo(
    () =>
      ([
        "general",
        "appearance",
        "language",
        "agent",
        "providers",
        "security",
        "background",
        "updates",
      ] as const).map((key) => ({
        key,
        label: t(`settings.nav.${key}`),
      })),
    [t],
  );

  const savePatch = async (patch: Partial<typeof settings>, refreshCapabilities = false) => {
    await persistPatch(project.projectId, project.rootPath, patch);
    if (refreshCapabilities) {
      await onRefreshCapabilities();
    }
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
      <aside className="settings-view__nav border-r border-[var(--border)] bg-[var(--surface)] p-3">
        <div className="mb-3 px-2">
          <div className="text-[10.5px] uppercase tracking-[0.08em] text-[var(--text-muted)]">{t("settings.nav.label")}</div>
          <div className="mt-1 text-[13px] font-medium">{project.name}</div>
        </div>
        <nav className="settings-view__tabs grid gap-1" aria-label={t("nav.settings")}>
          {sections.map((section) => (
            <button
              key={section.key}
              type="button"
              className={`flex h-[30px] items-center rounded-[var(--radius-md)] px-2 text-[13px] ${
                activeSection === section.key
                  ? "bg-[var(--surface-raised)] font-medium text-[var(--text-primary)] shadow-[0_0_0_1px_var(--border)]"
                  : "text-[var(--text-muted)] hover:bg-[var(--surface-raised)] hover:text-[var(--text-primary)]"
              }`}
              onClick={() => setActiveSection(section.key)}
            >
              {section.label}
            </button>
          ))}
        </nav>
      </aside>

      <div className="min-h-0 overflow-auto p-5">
        <div className="mx-auto grid max-w-[920px] gap-5">
          {loading || saving ? (
            <div className="flex justify-end gap-2 text-[11px] text-[var(--text-muted)]" role="status">
              {loading ? <span>{t("settings.state.loading")}</span> : null}
              {saving ? <span>{t("settings.state.saving")}</span> : null}
            </div>
          ) : null}

          {error ? <div className="rounded-[var(--radius-md)] border border-[var(--danger)] bg-[var(--danger-soft)] px-3 py-2 text-[12px] text-[var(--danger)]">{error}</div> : null}

          {activeSection === "general" ? (
            <section className="grid gap-4">
              <div>
                <h2 className="m-0 text-[16px] font-semibold">{t("settings.general.title")}</h2>
                <p className="mt-1 text-[12px] text-[var(--text-muted)]">{t("settings.general.description")}</p>
              </div>
              <div className="grid gap-3 md:grid-cols-2">
                <div className="rounded-[var(--radius-md)] border border-[var(--border)] p-3">
                  <div className="text-[12px] text-[var(--text-muted)]">{t("settings.general.projectRoot")}</div>
                  <div className="mt-1 font-mono text-[11px] text-[var(--text-secondary)]">{project.rootPath}</div>
                </div>
                <div className="rounded-[var(--radius-md)] border border-[var(--border)] p-3">
                  <div className="text-[12px] text-[var(--text-muted)]">{t("settings.general.scope")}</div>
                  <div className="mt-1 text-[13px]">{t("settings.general.scopeCopy")}</div>
                </div>
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

          {activeSection === "agent" ? (
            <AgentSettings
              agents={agents}
              agentDefault={settings.agentDefault}
              onRefresh={() => { void onRefreshCapabilities(); }}
              onChangeDefault={(agentDefault) => { void savePatch({ agentDefault }, true); }}
            />
          ) : null}

          {activeSection === "providers" ? (
            <LlmProviderSettings
              providers={providerStatuses}
              onSaveProvider={(config) => onSaveProvider({ ...config, contextWindow: settings.contextWindow })}
              onSaveSecret={async (provider, secret) => {
                await onSaveSecret(provider, secret);
                await onRefreshCapabilities();
              }}
              onDeleteSecret={async (provider) => {
                await onDeleteSecret(provider);
                await onRefreshCapabilities();
              }}
              onTestProvider={(config) => onTestProvider({ ...config, contextWindow: settings.contextWindow })}
            />
          ) : null}

          {activeSection === "security" ? (
            <SecuritySettings
              providers={securityRows}
              onDeleteSecret={(provider) => {
                void (async () => {
                  await onDeleteSecret(provider);
                  await onRefreshCapabilities();
                })();
              }}
            />
          ) : null}

          {activeSection === "background" ? (
            <BackgroundTaskSettings
              closeBehavior={settings.closeBehavior}
              contextWindow={settings.contextWindow}
              onChangeCloseBehavior={(closeBehavior) => { void savePatch({ closeBehavior }); }}
              onChangeContextWindow={(contextWindow) => { void savePatch({ contextWindow }); }}
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
