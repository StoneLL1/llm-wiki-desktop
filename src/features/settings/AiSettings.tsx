import { useEffect, useMemo, useState } from "react";
import { CheckCircle2, KeyRound, RefreshCw, Settings2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { AgentInfo, AgentKind } from "../../types/agent";
import type { LlmProviderConfig, LlmProviderKind, ProviderStatus, ProviderTestResult } from "../../types/llm";
import { BrandMark } from "./BrandMark";

interface AiSettingsProps {
  agents: AgentInfo[];
  providers: ProviderStatus[];
  agentDefault: AgentKind | null;
  contextWindow: number;
  onRefreshAgents: () => void;
  onChangeDefault: (agent: AgentKind | null) => void;
  onSaveProvider: (config: LlmProviderConfig) => Promise<unknown> | unknown;
  onSaveSecret: (provider: LlmProviderKind, secret: string) => Promise<unknown> | unknown;
  onDeleteSecret?: (provider: LlmProviderKind) => Promise<unknown> | unknown;
  onTestProvider?: (config: LlmProviderConfig) => Promise<ProviderTestResult>;
}

type AiTab = "cli" | "byok";

const providerOrder: LlmProviderKind[] = ["anthropic", "open_ai", "google", "ollama", "custom"];

const defaultBaseUrls: Record<LlmProviderKind, string> = {
  open_ai: "https://api.openai.com",
  anthropic: "https://api.anthropic.com",
  google: "https://generativelanguage.googleapis.com",
  ollama: "http://localhost:11434",
  custom: "",
};

const defaultModels: Record<LlmProviderKind, string> = {
  open_ai: "gpt-5.4",
  anthropic: "claude-sonnet-4-6",
  google: "gemini-2.5-pro",
  ollama: "",
  custom: "",
};

const agentMeta: Record<AgentKind, { name: string; vendorKey: string; descriptionKey: string }> = {
  claude: {
    name: "Claude Code",
    vendorKey: "settings.ai.agent.vendor.claude",
    descriptionKey: "settings.ai.agent.description.claude",
  },
  codex: {
    name: "Codex CLI",
    vendorKey: "settings.ai.agent.vendor.codex",
    descriptionKey: "settings.ai.agent.description.codex",
  },
  openclaw: {
    name: "OpenClaw",
    vendorKey: "settings.ai.agent.vendor.openclaw",
    descriptionKey: "settings.ai.agent.description.openclaw",
  },
  hermes: {
    name: "Hermes",
    vendorKey: "settings.ai.agent.vendor.hermes",
    descriptionKey: "settings.ai.agent.description.hermes",
  },
};

const getPreferredAgent = (agents: AgentInfo[], agentDefault: AgentKind | null): AgentKind | null =>
  agentDefault ?? agents.find((agent) => agent.isDefault)?.kind ?? agents[0]?.kind ?? null;

export function AiSettings({
  agents,
  providers,
  agentDefault,
  contextWindow,
  onRefreshAgents,
  onChangeDefault,
  onSaveProvider,
  onSaveSecret,
  onDeleteSecret,
  onTestProvider,
}: AiSettingsProps) {
  const { t } = useTranslation();
  const preferredAgent = useMemo(() => getPreferredAgent(agents, agentDefault), [agentDefault, agents]);
  const [activeTab, setActiveTab] = useState<AiTab>("cli");
  const [activeAgent, setActiveAgent] = useState<AgentKind | null>(() => preferredAgent);
  const [agentTouched, setAgentTouched] = useState(false);
  const [activeProvider, setActiveProvider] = useState<LlmProviderKind>("anthropic");
  const activeProviderStatus = providers.find((item) => item.config.provider === activeProvider);
  const [model, setModel] = useState(activeProviderStatus?.config.model ?? defaultModels.anthropic);
  const [baseUrl, setBaseUrl] = useState(activeProviderStatus?.config.baseUrl ?? defaultBaseUrls.anthropic);
  const [secret, setSecret] = useState("");
  const [formDirty, setFormDirty] = useState(false);
  const [providerSaved, setProviderSaved] = useState(false);
  const [secretSaved, setSecretSaved] = useState(false);
  const [testStatus, setTestStatus] = useState<string | null>(null);
  const activeProviderModel = activeProviderStatus?.config.model;
  const activeProviderBaseUrl = activeProviderStatus?.config.baseUrl;
  const activeAgentExists = activeAgent ? agents.some((agent) => agent.kind === activeAgent) : false;

  useEffect(() => {
    if (agentTouched && activeAgentExists) return;
    setActiveAgent(preferredAgent);
    if (!activeAgentExists) {
      setAgentTouched(false);
    }
  }, [activeAgentExists, agentTouched, preferredAgent]);

  useEffect(() => {
    if (formDirty) return;
    setModel(activeProviderModel ?? defaultModels[activeProvider]);
    setBaseUrl(activeProviderBaseUrl ?? defaultBaseUrls[activeProvider]);
  }, [activeProvider, activeProviderBaseUrl, activeProviderModel, formDirty]);

  const providerStatuses = useMemo(
    () => providerOrder.map((provider) => providers.find((item) => item.config.provider === provider) ?? null),
    [providers],
  );

  const selectProvider = (provider: LlmProviderKind) => {
    const status = providers.find((item) => item.config.provider === provider);
    setActiveProvider(provider);
    setModel(status?.config.model ?? defaultModels[provider]);
    setBaseUrl(status?.config.baseUrl ?? defaultBaseUrls[provider]);
    setSecret("");
    setFormDirty(false);
    setProviderSaved(false);
    setSecretSaved(false);
    setTestStatus(null);
  };

  const saveProvider = async () => {
    await onSaveProvider({
      provider: activeProvider,
      model,
      baseUrl,
      contextWindow,
      enabled: true,
    });
    setFormDirty(false);
    setProviderSaved(true);
  };

  const saveKey = async () => {
    await onSaveSecret(activeProvider, secret);
    setSecret("");
    setSecretSaved(true);
  };

  const deleteKey = async () => {
    await onDeleteSecret?.(activeProvider);
    setSecretSaved(false);
  };

  const testProvider = async () => {
    setTestStatus(t("provider.testing"));
    try {
      const result = await onTestProvider?.({
        provider: activeProvider,
        model,
        baseUrl,
        contextWindow,
        enabled: true,
      });
      setTestStatus(result?.message ?? t("provider.testUnavailable"));
    } catch (error) {
      setTestStatus(error instanceof Error ? error.message : t("provider.testFailed"));
    }
  };

  return (
    <section className="settings-section-panel">
      <div className="settings-section-heading">
        <div>
          <h2 className="settings-view__section-title">{t("settings.ai.title")}</h2>
          <p className="settings-view__section-desc">{t("settings.ai.description")}</p>
        </div>
        <span className="badge badge--outline">{activeTab === "cli" ? t("settings.ai.agent.count", { count: agents.length }) : t("settings.ai.provider.count", { count: providerOrder.length })}</span>
      </div>

      <div className="settings-ai-tabs" role="group" aria-label={t("settings.ai.title")}>
        <button type="button" aria-pressed={activeTab === "cli"} onClick={() => setActiveTab("cli")}>
          {t("settings.ai.tab.cli")}
        </button>
        <button type="button" aria-pressed={activeTab === "byok"} onClick={() => setActiveTab("byok")}>
          {t("settings.ai.tab.byok")}
        </button>
      </div>

      {activeTab === "cli" ? (
        <div className="settings-ai-stack">
          <div className="settings-ai-toolbar">
            <span>{t("settings.agent.description")}</span>
            <button type="button" className="settings-button settings-button--secondary" onClick={onRefreshAgents}>
              <RefreshCw aria-hidden="true" size={13} />
              {t("settings.agent.refresh")}
            </button>
          </div>
          {agents.map((agent) => {
            const meta = agentMeta[agent.kind];
            const selected = agentDefault === agent.kind || agent.isDefault;
            const active = activeAgent === agent.kind;
            return (
              <article key={agent.kind} className={`settings-ai-card ${active ? "is-active" : ""}`}>
                <button
                  type="button"
                  className="settings-ai-card__main"
                  onClick={() => {
                    setAgentTouched(true);
                    setActiveAgent(agent.kind);
                  }}
                  aria-pressed={active}
                >
                  <BrandMark kind={agent.kind} type="agent" />
                  <span className="settings-ai-card__copy">
                    <span className="settings-ai-card__title">
                      {meta.name}
                      <span className="settings-ai-card__vendor">{t(meta.vendorKey)}</span>
                    </span>
                    <span className="settings-ai-card__meta">{agent.version ?? agent.executablePath ?? agent.error ?? agent.installGuidance ?? t(meta.descriptionKey)}</span>
                  </span>
                </button>
                <span className={`badge ${agent.state === "installed" ? "badge--success" : agent.state === "failed" ? "badge--danger" : "badge--outline"}`}>
                  {agent.state === "installed" ? <span className="dot" /> : null}
                  {t(`settings.agent.state.${agent.state}`)}
                </span>
                <div className="settings-ai-card__actions">
                  {selected ? <span className="badge badge--accent">{t("settings.ai.agent.defaultBadge")}</span> : null}
                  <button
                    type="button"
                    className="settings-button"
                    disabled={agent.state !== "installed"}
                    aria-label={t("settings.ai.agent.setDefaultFor", { agent: agent.kind })}
                    onClick={() => {
                      setAgentTouched(false);
                      setActiveAgent(agent.kind);
                      onChangeDefault(agent.kind);
                    }}
                  >
                    {selected ? t("settings.agent.selected") : t("settings.agent.makeDefault")}
                  </button>
                  {selected ? (
                    <button type="button" className="settings-button settings-button--secondary" onClick={() => onChangeDefault(null)}>
                      {t("settings.agent.clear")}
                    </button>
                  ) : null}
                </div>
              </article>
            );
          })}
          {activeAgent ? (
            <div className="settings-ai-detail">
              <div className="settings-ai-detail__head">
                <Settings2 aria-hidden="true" size={14} />
                <span>{t("settings.ai.agent.detailTitle")}</span>
              </div>
              <dl className="settings-ai-detail__grid">
                <dt>{t("settings.ai.agent.modelLabel")}</dt>
                <dd>{t("settings.ai.agent.modelDefault")}</dd>
                <dt>{t("settings.agent.selected")}</dt>
                <dd>{activeAgent}</dd>
              </dl>
            </div>
          ) : null}
        </div>
      ) : null}

      {activeTab === "byok" ? (
        <div className="settings-ai-stack">
          <div className="settings-ai-toolbar">
            <span>{t("provider.byokHint")}</span>
            <span className="badge badge--outline"><KeyRound aria-hidden="true" size={11} />{t("settings.security.title")}</span>
          </div>
          <div className="settings-ai-provider-list">
            {providerOrder.map((provider, index) => {
              const status = providerStatuses[index];
              const active = activeProvider === provider;
              const configured = Boolean(status?.hasSecret || status?.secretMask);
              const modelText = status?.config.model || defaultModels[provider] || t("provider.noModel");
              const hint = provider === "ollama"
                ? t("provider.ollamaLocal", { base: status?.config.baseUrl || defaultBaseUrls.ollama })
                : status?.secretMask
                  ? t("provider.maskedHint", { model: modelText, mask: status.secretMask })
                  : configured
                    ? t("provider.configuredHint")
                    : t("provider.unconfiguredHint");
              return (
                <button
                  key={provider}
                  type="button"
                  className={`settings-ai-card ${active ? "is-active" : ""}`}
                  aria-pressed={active}
                  onClick={() => selectProvider(provider)}
                >
                  <BrandMark kind={provider} type="provider" />
                  <span className="settings-ai-card__copy">
                    <span className="settings-ai-card__title">{t(`provider.name.${provider}`)}</span>
                    <span className="settings-ai-card__meta">
                      <span className="settings-ai-card__model">{modelText}</span>
                      <span> - {hint}</span>
                    </span>
                  </span>
                  <span className={`badge ${configured ? "badge--success" : "badge--outline"}`}>
                    {configured ? <span className="dot" /> : null}
                    {configured ? t("provider.configured") : provider === "ollama" ? t("provider.localService") : t("provider.notConfigured")}
                  </span>
                </button>
              );
            })}
          </div>

          <div className="settings-ai-detail">
            <div className="settings-ai-detail__head">
              <CheckCircle2 aria-hidden="true" size={14} />
              <span>{t("settings.ai.provider.detailTitle")}</span>
            </div>
            <div className="settings-form-compact">
              <label>
                <span>{t("provider.model")}</span>
                <input
                  className="settings-input"
                  value={model}
                  aria-label={t("provider.model")}
                  onChange={(event) => {
                    setFormDirty(true);
                    setProviderSaved(false);
                    setModel(event.target.value);
                  }}
                />
              </label>
              <label>
                <span>{t("provider.baseUrl")}</span>
                <input
                  className="settings-input"
                  value={baseUrl}
                  aria-label={t("provider.baseUrl")}
                  onChange={(event) => {
                    setFormDirty(true);
                    setProviderSaved(false);
                    setBaseUrl(event.target.value);
                  }}
                />
              </label>
              {activeProvider !== "ollama" ? (
                <label>
                  <span>{t("provider.apiKey")}</span>
                  <input
                    aria-label={t("provider.apiKey")}
                    type="password"
                    autoComplete="off"
                    className="settings-input"
                    value={secret}
                    onChange={(event) => {
                      setSecretSaved(false);
                      setSecret(event.target.value);
                    }}
                  />
                </label>
              ) : null}
            </div>
            <div className="settings-ai-detail__actions">
              <button type="button" className="settings-button" onClick={() => void saveProvider()}>
                {t("provider.saveProvider")}
              </button>
              <button type="button" className="settings-button settings-button--secondary" onClick={() => void testProvider()}>
                {t("provider.test")}
              </button>
              {activeProvider !== "ollama" && secret ? (
                <button type="button" className="settings-button" onClick={() => void saveKey()}>
                  {t("provider.saveKey")}
                </button>
              ) : null}
              {activeProviderStatus?.hasSecret ? (
                <button type="button" className="settings-button settings-button--danger" onClick={() => void deleteKey()}>
                  {t("provider.deleteKey")}
                </button>
              ) : null}
            </div>
            {providerSaved ? <span className="settings-inline-status text-[var(--accent)]">{t("provider.settingsSaved")}</span> : null}
            {secretSaved ? <span className="settings-inline-status text-[var(--accent)]">{t("provider.keySaved")}</span> : null}
            {testStatus ? <span role="status" className="settings-inline-status">{testStatus}</span> : null}
          </div>

          <div className="settings-safety-note">
            <KeyRound aria-hidden="true" size={14} />
            <span>{t("provider.secretSafety")}</span>
          </div>
        </div>
      ) : null}
    </section>
  );
}
