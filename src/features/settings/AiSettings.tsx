import { useEffect, useMemo, useRef, useState } from "react";
import { CheckCircle2, KeyRound, RefreshCw, Settings2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { ActionableErrorNotice } from "../../components/app/ActionableErrorNotice";
import { normalizeBackendError, type NormalizedBackendError } from "../../lib/backendError";
import type { AgentInfo, AgentKind } from "../../types/agent";
import type { LlmProviderConfig, LlmProviderKind, ProviderStatus, ProviderTestResult } from "../../types/llm";
import type { WorkflowRouteSelection } from "../../types/workflow";
import { BrandMark } from "./BrandMark";
import { providerDraftDirty, providerOrder, useProviderDrafts } from "./useProviderDrafts";

interface AiSettingsProps {
  agents: AgentInfo[];
  providers: ProviderStatus[];
  agentDefault: AgentKind | null;
  contextWindow: number;
  aiDefaultRoute?: WorkflowRouteSelection | null;
  onChangeAiDefaultRoute?: (route: WorkflowRouteSelection | null) => Promise<unknown> | unknown;
  onDirtyChange?: (dirty: boolean) => void;
  onRefreshAgents: () => void;
  onChangeDefault: (agent: AgentKind | null) => void;
  onSaveProvider: (config: LlmProviderConfig) => Promise<unknown> | unknown;
  onSaveSecret: (provider: LlmProviderKind, secret: string) => Promise<unknown> | unknown;
  onDeleteSecret?: (provider: LlmProviderKind) => Promise<unknown> | unknown;
  onTestProvider?: (config: LlmProviderConfig) => Promise<ProviderTestResult>;
}

type AiTab = "cli" | "byok";
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

export function AiSettings(props: AiSettingsProps) {
  const { agents, providers, agentDefault, aiDefaultRoute, onChangeAiDefaultRoute, onRefreshAgents, onChangeDefault, onDeleteSecret, onTestProvider } = props;
  const { t, i18n } = useTranslation();
  const preferredAgent = useMemo(() => getPreferredAgent(agents, agentDefault), [agentDefault, agents]);
  const [activeTab, setActiveTab] = useState<AiTab>(aiDefaultRoute?.kind === "byok" ? "byok" : "cli");
  const [activeAgent, setActiveAgent] = useState<AgentKind | null>(() => preferredAgent);
  const [agentTouched, setAgentTouched] = useState(false);
  const [activeProvider, setActiveProvider] = useState<LlmProviderKind>(aiDefaultRoute?.kind === "byok" ? aiDefaultRoute.provider : "anthropic");
  const [routePending, setRoutePending] = useState(false);
  const [routeError, setRouteError] = useState<NormalizedBackendError | null>(null);
  const routeBusy = useRef(false);
  const mounted = useRef(true);
  const secretInputRef = useRef<HTMLInputElement>(null);
  const { drafts, edit, run, dismissApproval } = useProviderDrafts(props);
  const draft = drafts[activeProvider];
  const formDirty = providerDraftDirty(draft);
  const activeProviderStatus = providers.find((item) => item.config.provider === activeProvider);
  const activeAgentExists = activeAgent ? agents.some((agent) => agent.kind === activeAgent) : false;
  const route = aiDefaultRoute === undefined ? (agentDefault ? { kind: "agent" as const, agent: agentDefault } : null) : aiDefaultRoute;
  const routeValue = route ? `${route.kind}:${route.kind === "agent" ? route.agent : route.provider}` : "";
  const compatibleProviders = providers.filter((provider) => provider.config.enabled && provider.config.model.trim());
  const routeHint = route
    ? t("settings.ai.explicitRouteHint")
    : agentDefault
      ? t("settings.ai.automaticAgentHint", { agent: agentMeta[agentDefault].name })
      : compatibleProviders.length === 1
        ? t("settings.ai.automaticProviderHint", { provider: t(`provider.name.${compatibleProviders[0].config.provider}`) })
        : t("settings.ai.automaticChoiceHint");
  const contextValue = Number(draft.fields.contextWindow);
  const validConfig = Boolean(draft.fields.model.trim() && draft.fields.baseUrl.trim()) && Number.isInteger(contextValue) && contextValue > 0 && contextValue <= 4_294_967_295;

  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; };
  }, []);
  useEffect(() => {
    if (agentTouched && activeAgentExists) return;
    setActiveAgent(preferredAgent);
    if (!activeAgentExists) setAgentTouched(false);
  }, [activeAgentExists, agentTouched, preferredAgent]);

  const changeRoute = async (next: WorkflowRouteSelection | null) => {
    if (routeBusy.current) return;
    routeBusy.current = true;
    setRoutePending(true);
    setRouteError(null);
    try {
      if (onChangeAiDefaultRoute) await onChangeAiDefaultRoute(next);
      else onChangeDefault(next?.kind === "agent" ? next.agent : null);
    } catch (error) {
      if (mounted.current) setRouteError(normalizeBackendError(error));
    } finally {
      routeBusy.current = false;
      if (mounted.current) setRoutePending(false);
    }
  };
  const routeOptions = [
    ...agents.filter((agent) => agent.state === "installed" || route?.kind === "agent" && route.agent === agent.kind).map((agent) => ({ value: `agent:${agent.kind}`, name: `${agentMeta[agent.kind].name} · ${t(`settings.agent.state.${agent.state}`)}` })),
    ...providers.filter((provider) => provider.config.enabled || route?.kind === "byok" && route.provider === provider.config.provider).map((provider) => ({ value: `byok:${provider.config.provider}`, name: `${t(`provider.name.${provider.config.provider}`)} · ${provider.config.model || t("provider.noModel")}` })),
  ];
  if (routeValue && !routeOptions.some((option) => option.value === routeValue)) {
    routeOptions.push({ value: routeValue, name: route?.kind === "agent" ? agentMeta[route.agent].name : t(`provider.name.${route!.kind === "byok" ? route!.provider : "custom"}`) });
  }
  const connectionState = (provider: LlmProviderKind) => {
    const status = providers.find((item) => item.config.provider === provider);
    if (drafts[provider].test) return t("settings.ai.connection.tested");
    if (status?.hasSecret) return t("settings.ai.connection.credentialSaved");
    return provider === "ollama" ? t("provider.localService") : t("provider.notConfigured");
  };
  const operationLabel = draft.pending?.kind === "test" ? t("provider.testing") : draft.pending ? t("settings.ai.connection.saving") : formDirty ? t("settings.ai.connection.unsaved") : draft.saved === "provider" ? t("provider.settingsSaved") : draft.saved === "secret" ? t("provider.keySaved") : draft.saved === "deleted" ? t("settings.ai.connection.keyDeleted") : null;

  return (
    <section className="settings-section-panel">
      <div className="settings-section-heading">
        <div>
          <h2 className="settings-view__section-title">{t("settings.ai.title")}</h2>
          <p className="settings-view__section-desc">{t("settings.ai.description")}</p>
        </div>
      </div>
      {onChangeAiDefaultRoute ? (
        <div className="settings-ai-route">
          <label htmlFor="settings-ai-default-route">
            <span className="settings-row-card__title">{t("settings.ai.defaultRoute")}</span>
            <span className="settings-ai-field-hint">{routeHint}</span>
          </label>
          <select id="settings-ai-default-route" className="settings-input" value={routeValue} disabled={routePending} onChange={(event) => {
            const [kind, value] = event.target.value.split(":");
            void changeRoute(kind === "agent" ? { kind, agent: value as AgentKind } : kind === "byok" ? { kind, provider: value as LlmProviderKind } : null);
          }}>
            <option value="">{t("settings.ai.noDefaultRoute")}</option>
            {routeOptions.map((option) => <option key={option.value} value={option.value}>{option.name}</option>)}
          </select>
          {routeError ? <ActionableErrorNotice error={routeError} /> : null}
        </div>
      ) : null}
      <div className="settings-ai-tabs" role="group" aria-label={t("settings.ai.title")}>
        <button type="button" aria-pressed={activeTab === "cli"} onClick={() => setActiveTab("cli")}>{t("settings.ai.tab.cli")}</button>
        <button type="button" aria-pressed={activeTab === "byok"} onClick={() => setActiveTab("byok")}>{t("settings.ai.tab.byok")}</button>
      </div>
      {activeTab === "cli" ? (
        <div className="settings-ai-stack">
          <div className="settings-ai-toolbar">
            <span>{t("settings.agent.description")}</span>
            <button type="button" className="settings-button settings-button--secondary" onClick={onRefreshAgents}><RefreshCw aria-hidden="true" size={13} />{t("settings.agent.refresh")}</button>
          </div>
          {agents.map((agent) => {
            const meta = agentMeta[agent.kind];
            const selected = route?.kind === "agent" && route.agent === agent.kind;
            const active = activeAgent === agent.kind;
            return (
              <article key={agent.kind} className={`settings-ai-card ${active ? "is-active" : ""}`}>
                <button type="button" className="settings-ai-card__main" onClick={() => { setAgentTouched(true); setActiveAgent(agent.kind); }} aria-pressed={active}>
                  <BrandMark kind={agent.kind} type="agent" />
                  <span className="settings-ai-card__copy">
                    <span className="settings-ai-card__title">{meta.name}<span className="settings-ai-card__vendor">{t(meta.vendorKey)}</span></span>
                    <span className="settings-ai-card__meta">{agent.version ?? agent.executablePath ?? agent.error ?? agent.installGuidance ?? t(meta.descriptionKey)}</span>
                  </span>
                </button>
                <span className={`badge ${agent.state === "installed" ? "badge--success" : agent.state === "failed" ? "badge--danger" : "badge--outline"}`}>{t(`settings.agent.state.${agent.state}`)}</span>
                <div className="settings-ai-card__actions">
                  <button type="button" className="settings-button settings-button--secondary" disabled={agent.state !== "installed" || routePending || selected} aria-label={t("settings.ai.agent.setDefaultFor", { agent: agent.kind })} onClick={() => { setAgentTouched(false); setActiveAgent(agent.kind); void changeRoute({ kind: "agent", agent: agent.kind }); }}>{selected ? t("settings.agent.selected") : t("settings.agent.makeDefault")}</button>
                </div>
              </article>
            );
          })}
          {activeAgent ? (
            <div className="settings-ai-detail">
              <div className="settings-ai-detail__head"><Settings2 aria-hidden="true" size={14} /><span>{t("settings.ai.agent.detailTitle")}</span></div>
              <dl className="settings-ai-detail__grid"><dt>{t("settings.ai.agent.modelLabel")}</dt><dd>{t("settings.ai.agent.modelDefault")}</dd><dt>{t("settings.agent.selected")}</dt><dd>{activeAgent}</dd></dl>
              <p className="settings-ai-field-hint">{t("settings.ai.agent.modelSource")}</p>
            </div>
          ) : null}
        </div>
      ) : null}
      {activeTab === "byok" ? (
        <div className="settings-ai-stack">
          <div className="settings-ai-connections">
            <div className="settings-ai-provider-list" role="group" aria-label={t("settings.ai.provider.detailTitle")}>
              {providerOrder.map((provider) => (
                <button key={provider} type="button" className={`settings-ai-connection-item ${activeProvider === provider ? "is-active" : ""}`} aria-pressed={activeProvider === provider} onClick={() => setActiveProvider(provider)}>
                  <BrandMark kind={provider} type="provider" />
                  <span className="settings-ai-card__copy">
                    <span className="settings-ai-card__title">{t(`provider.name.${provider}`)}</span>
                    <span className="settings-ai-connection-item__state">{connectionState(provider)}</span>
                    <span className="settings-ai-card__meta" title={drafts[provider].fields.model}>{drafts[provider].fields.model || t("provider.noModel")}</span>
                  </span>
                  {providerDraftDirty(drafts[provider]) || drafts[provider].secret ? <span className="settings-ai-draft-dot" aria-label={t("settings.ai.connection.unsaved")} /> : null}
                </button>
              ))}
            </div>
            <div className="settings-ai-editor" aria-label={t("settings.ai.provider.editorFor", { provider: t(`provider.name.${activeProvider}`) })}>
              <div className="settings-ai-editor__head">
                <h3>{t(`provider.name.${activeProvider}`)}</h3>
                <span className={`badge ${draft.test ? "badge--success" : "badge--outline"}`}>{draft.test ? <CheckCircle2 aria-hidden="true" size={11} /> : null}{draft.test ? t("settings.ai.connection.tested") : t("settings.ai.connection.untested")}</span>
              </div>
              <div className="settings-form-compact settings-provider-fields">
                <label><span>{t("provider.model")}</span><input className="settings-input" autoCapitalize="none" autoCorrect="off" spellCheck={false} value={draft.fields.model} onChange={(event) => edit(activeProvider, { model: event.target.value })} /></label>
                <label><span>{t("settings.ai.contextWindow")}</span><input type="number" min={1} max={4_294_967_295} step={1} className="settings-input" value={draft.fields.contextWindow} onChange={(event) => edit(activeProvider, { contextWindow: event.target.value })} aria-describedby="provider-context-hint" /></label>
                <label className="settings-provider-fields__full"><span>{t("provider.baseUrl")}</span><input className="settings-input" autoCapitalize="none" autoCorrect="off" spellCheck={false} value={draft.fields.baseUrl} onChange={(event) => edit(activeProvider, { baseUrl: event.target.value })} readOnly={activeProvider !== "custom" && activeProvider !== "ollama"} /></label>
                <p id="provider-context-hint" className="settings-ai-field-hint">{activeProviderStatus ? t("settings.ai.contextWindowEffective", { count: activeProviderStatus.config.contextWindow.toLocaleString(i18n.language) }) : t("settings.ai.contextWindowHint")}</p>
              </div>
              <div className="settings-ai-detail__actions">
                <button type="button" className="settings-button" disabled={Boolean(draft.pending) || !validConfig} onClick={() => void run(activeProvider, "save_provider")}>{t("provider.saveProvider")}</button>
                <button type="button" className="settings-button settings-button--secondary" onClick={() => void run(activeProvider, "test")} disabled={Boolean(draft.pending) || formDirty || !activeProviderStatus?.credentialBinding || !onTestProvider}>{draft.pending?.kind === "test" ? t("provider.testing") : t("provider.test")}</button>
              </div>
              {activeProvider !== "ollama" ? (
                <div className="settings-ai-secret-row">
                  <label><span>{t("provider.apiKey")}</span><input ref={secretInputRef} type="password" autoComplete="off" spellCheck={false} className="settings-input" value={draft.secret} placeholder={activeProviderStatus?.secretMask ?? ""} onChange={(event) => edit(activeProvider, { secret: event.target.value })} /></label>
                  <div className="settings-ai-detail__actions">
                    <button type="button" className="settings-button settings-button--secondary" disabled={Boolean(draft.pending) || !draft.secret} onClick={() => void run(activeProvider, "save_secret")}>{t("provider.saveKey")}</button>
                    {activeProviderStatus?.hasSecret && onDeleteSecret ? <button type="button" className="settings-button settings-button--danger" disabled={Boolean(draft.pending)} onClick={() => void run(activeProvider, "delete_secret")}>{t("provider.deleteKey")}</button> : null}
                  </div>
                  <span className="settings-ai-field-hint">{activeProviderStatus?.hasSecret ? t("settings.ai.connection.credentialSaved") : t("provider.notConfigured")}</span>
                </div>
              ) : null}
              {draft.approval && activeProviderStatus?.credentialBinding ? (
                <div role="alertdialog" aria-labelledby="provider-origin-approval-title" aria-describedby="provider-origin-approval-description" className="settings-ai-origin-approval">
                  <strong id="provider-origin-approval-title">{t("provider.binding.reviewTitle")}</strong>
                  <span id="provider-origin-approval-description">{t("provider.binding.reviewDescription")}</span>
                  <code>{activeProviderStatus.credentialBinding.canonicalOrigin}</code>
                  <div className="settings-ai-detail__actions"><button type="button" className="settings-button" disabled={Boolean(draft.pending)} onClick={() => void run(activeProvider, "save_secret", true)}>{t("provider.binding.authorize")}</button><button type="button" className="settings-button settings-button--secondary" onClick={() => dismissApproval(activeProvider)}>{t("common.cancel")}</button></div>
                </div>
              ) : null}
              {operationLabel || draft.test ? <div role="status" className="settings-ai-status-row">{operationLabel ? <span>{operationLabel}</span> : null}{draft.test ? <span>{t("settings.ai.connection.testResult", { provider: t(`provider.name.${activeProvider}`), time: new Date(draft.test.at).toLocaleTimeString(i18n.language, { hour: "2-digit", minute: "2-digit" }), message: draft.test.message })}</span> : null}</div> : null}
              {draft.error ? <ActionableErrorNotice error={draft.error} onAction={draft.error.actionKind === "retry" && draft.failedOperation ? () => run(activeProvider, draft.failedOperation!) : draft.error.actionKind === "reauthorize" && activeProvider !== "ollama" ? () => secretInputRef.current?.focus() : undefined} role="status" /> : null}
            </div>
          </div>
          <div className="settings-safety-note"><KeyRound aria-hidden="true" size={14} /><span>{t("provider.secretSafety")}</span></div>
        </div>
      ) : null}
    </section>
  );
}
