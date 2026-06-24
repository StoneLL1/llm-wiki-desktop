import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Cpu, Link2, RefreshCw } from "lucide-react";
import type { LlmProviderConfig, LlmProviderKind, ProviderStatus, ProviderTestResult } from "../../types/llm";
import { useTranslation } from "react-i18next";

interface Props {
  providers: ProviderStatus[];
  onSaveProvider: (config: LlmProviderConfig) => Promise<unknown> | unknown;
  onSaveSecret: (provider: LlmProviderKind, secret: string) => Promise<unknown> | unknown;
  onDeleteSecret?: (provider: LlmProviderKind) => Promise<unknown> | unknown;
  onTestProvider?: (config: LlmProviderConfig) => Promise<ProviderTestResult>;
}

const providerOrder: LlmProviderKind[] = ["anthropic", "open_ai", "google", "ollama", "custom"];
const defaultBaseUrls: Record<LlmProviderKind, string> = {
  open_ai: "https://api.openai.com",
  anthropic: "https://api.anthropic.com",
  google: "https://generativelanguage.googleapis.com",
  ollama: "http://localhost:11434",
  custom: "",
};

const hasTauri = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

interface OllamaReachability {
  reachable: boolean;
  baseUrl: string;
  modelCount: number;
}

function providerInitial(kind: LlmProviderKind): string {
  switch (kind) {
    case "anthropic":
      return "A";
    case "open_ai":
      return "O";
    case "google":
      return "G";
    default:
      return "";
  }
}

export function LlmProviderSettings({
  providers,
  onSaveProvider,
  onSaveSecret,
  onDeleteSecret,
  onTestProvider,
}: Props) {
  const { t } = useTranslation();
  const [active, setActive] = useState<LlmProviderKind>("anthropic");
  const [model, setModel] = useState("");
  const [baseUrl, setBaseUrl] = useState(defaultBaseUrls.anthropic);
  const [secret, setSecret] = useState("");
  const [saved, setSaved] = useState(false);
  const [testStatus, setTestStatus] = useState<string | null>(null);
  const [ollama, setOllama] = useState<OllamaReachability | null>(null);

  const statusFor = (kind: LlmProviderKind): ProviderStatus | undefined =>
    providers.find((item) => item.config.provider === kind);

  useEffect(() => {
    if (!hasTauri()) return;
    let cancelled = false;
    void invoke<OllamaReachability>("check_ollama_reachable", {
      request: { projectId: "", projectRootPath: "" },
    })
      .then((result) => {
        if (!cancelled) setOllama(result);
      })
      .catch(() => {
        if (!cancelled) setOllama(null);
      });
    return () => {
      cancelled = true;
    };
  }, [providers]);

  const selectProvider = (kind: LlmProviderKind) => {
    const saved = statusFor(kind)?.config;
    setActive(kind);
    setModel(saved?.model ?? "");
    setBaseUrl(saved?.baseUrl ?? defaultBaseUrls[kind]);
    setSecret("");
    setSaved(false);
    setTestStatus(null);
  };

  const saveProvider = async () => {
    await onSaveProvider({ provider: active, model, baseUrl, contextWindow: 32_000, enabled: true });
    setSaved(true);
  };

  const saveKey = async () => {
    await onSaveSecret(active, secret);
    setSecret("");
    setSaved(true);
  };

  const testProvider = async () => {
    setTestStatus(t("provider.testing"));
    try {
      const result = await onTestProvider?.({
        provider: active,
        model,
        baseUrl,
        contextWindow: 32_000,
        enabled: true,
      });
      setTestStatus(result?.message ?? t("provider.testUnavailable"));
    } catch (error) {
      setTestStatus(error instanceof Error ? error.message : t("provider.testFailed"));
    }
  };

  const clearKey = async () => {
    await onDeleteSecret?.(active);
  };

  const ollamaUp = ollama?.reachable === true;

  return (
    <section className="grid gap-4">
      <div>
        <h2 className="m-0 text-[16px] font-semibold">{t("provider.title")}</h2>
        <p className="mt-1 text-[12px] text-[var(--text-muted)]">{t("provider.byokHint")}</p>
      </div>

      <div className="grid">
        {providerOrder.map((kind) => {
          const status = statusFor(kind);
          const hasSecret = Boolean(status?.hasSecret);
          const isOllama = kind === "ollama";
          const isActive = active === kind;
          const hint = isOllama
            ? ollamaUp
              ? t("provider.ollamaUp", {
                  base: ollama?.baseUrl ?? defaultBaseUrls.ollama,
                  count: ollama?.modelCount ?? 0,
                })
              : t("provider.ollamaDown")
            : status?.secretMask
              ? t("provider.maskedHint", {
                  model: status.config.model || t("provider.noModel"),
                  mask: status.secretMask,
                })
              : hasSecret
                ? t("provider.configuredHint")
                : t("provider.unconfiguredHint");
          return (
            <button
              key={kind}
              type="button"
              aria-pressed={isActive}
              className={`apikey-row text-left ${isActive ? "rounded-[var(--radius-md)] bg-[var(--surface-raised)] px-3 shadow-[0_0_0_1px_var(--border)]" : "px-3"}`}
              onClick={() => selectProvider(kind)}
            >
              <span
                className="apikey-row__icon"
                style={kind === "anthropic" ? { background: "#0d0d0d", color: "#fff" } : undefined}
                aria-hidden="true"
              >
                {isOllama ? (
                  <Cpu size={16} />
                ) : kind === "custom" ? (
                  <Link2 size={16} />
                ) : (
                  providerInitial(kind)
                )}
              </span>
              <span className="min-w-0">
                <span className="apikey-row__name block">{t(`provider.name.${kind}`)}</span>
                <span className="apikey-row__hint block">
                  {isOllama ? hint : `${status?.config.model || t("provider.noModel")} · ${hint}`}
                </span>
              </span>
              {hasSecret ? (
                <span className="badge badge--success">
                  <span className="dot" />
                  {t("provider.configured")}
                </span>
              ) : isOllama ? (
                <span className="badge badge--warn">{t("provider.serviceDown")}</span>
              ) : (
                <span className="badge badge--outline">{t("provider.notConfigured")}</span>
              )}
              <span className="text-[12px] text-[var(--text-muted)]">
                {status?.config.model || t("provider.noModel")}
              </span>
            </button>
          );
        })}
      </div>

      <div className="grid gap-3 rounded-[var(--radius-md)] border border-[var(--border)] p-4">
        <div className="text-[13px] font-medium">{t("provider.editing", { provider: t(`provider.name.${active}`) })}</div>
        <label className="grid gap-1 text-[12px]">
          {t("provider.model")}
          <input
            className="settings-input"
            value={model}
            aria-label={t("provider.model")}
            onChange={(event) => { setSaved(false); setModel(event.target.value); }}
          />
        </label>
        {active !== "ollama" || baseUrl ? (
          <label className="grid gap-1 text-[12px]">
            {t("provider.baseUrl")}
            <input
              className="settings-input"
              value={baseUrl}
              aria-label={t("provider.baseUrl")}
              onChange={(event) => { setSaved(false); setBaseUrl(event.target.value); }}
            />
          </label>
        ) : null}
        <div className="flex flex-wrap gap-2">
          <button type="button" className="settings-button" onClick={() => void saveProvider()}>
            {t("provider.saveProvider")}
          </button>
          <button
            type="button"
            className="settings-button settings-button--secondary"
            onClick={() => void testProvider()}
            disabled={active === "ollama" && !ollamaUp}
          >
            {active === "ollama" ? <RefreshCw size={12} /> : null}
            {t("provider.test")}
          </button>
          {statusFor(active)?.hasSecret ? (
            <button type="button" className="settings-button settings-button--danger" onClick={() => void clearKey()}>
              {t("provider.deleteKey")}
            </button>
          ) : null}
        </div>
        {active !== "ollama" ? (
          <label className="grid gap-1 text-[12px]">
            {t("provider.apiKey")}
            <input
              aria-label={t("provider.apiKey")}
              type="password"
              autoComplete="off"
              className="settings-input"
              value={secret}
              onChange={(event) => { setSaved(false); setSecret(event.target.value); }}
            />
          </label>
        ) : null}
        {active !== "ollama" && secret ? (
          <button type="button" className="settings-button" onClick={() => void saveKey()}>
            {t("provider.saveKey")}
          </button>
        ) : null}
        {saved ? <span className="text-[12px] text-[var(--accent)]">{t("provider.saved")}</span> : null}
        {testStatus ? (
          <span role="status" className="text-[12px] text-[var(--text-muted)]">
            {testStatus}
          </span>
        ) : null}
      </div>

      <div className="rounded-[var(--radius-md)] border border-[var(--accent-border)] bg-[var(--accent-soft)] px-3 py-2 text-[12px] text-[var(--accent-hover)]">
        {t("provider.secretSafety")}
      </div>
    </section>
  );
}
