import { useState } from "react";
import type { LlmProviderConfig, LlmProviderKind, ProviderStatus, ProviderTestResult } from "../../types/llm";
import { useTranslation } from "react-i18next";

interface Props {
  providers: ProviderStatus[];
  onSaveProvider: (config: LlmProviderConfig) => Promise<unknown> | unknown;
  onSaveSecret: (provider: LlmProviderKind, secret: string) => Promise<unknown> | unknown;
  onDeleteSecret?: (provider: LlmProviderKind) => Promise<unknown> | unknown;
  onTestProvider?: (config: LlmProviderConfig) => Promise<ProviderTestResult>;
}

const providerKinds: LlmProviderKind[] = ["open_ai", "anthropic", "google", "ollama", "custom"];
const defaultBaseUrls: Record<LlmProviderKind, string> = {
  open_ai: "https://api.openai.com",
  anthropic: "https://api.anthropic.com",
  google: "https://generativelanguage.googleapis.com",
  ollama: "http://localhost:11434",
  custom: "",
};

export function LlmProviderSettings({ providers, onSaveProvider, onSaveSecret, onDeleteSecret, onTestProvider }: Props) {
  const { t } = useTranslation();
  const [provider, setProvider] = useState<LlmProviderKind>("open_ai");
  const [model, setModel] = useState("");
  const [baseUrl, setBaseUrl] = useState("https://api.openai.com");
  const [secret, setSecret] = useState("");
  const [saved, setSaved] = useState(false);
  const [testStatus, setTestStatus] = useState<string | null>(null);

  const saveKey = async () => {
    await onSaveSecret(provider, secret);
    setSecret("");
    setSaved(true);
  };

  const testProvider = async () => {
    setTestStatus(null);
    try {
      const result = await onTestProvider?.({ provider, model, baseUrl, contextWindow: 32_000, enabled: true });
      setTestStatus(result?.message ?? t("provider.testUnavailable"));
    } catch (error) {
      setTestStatus(error instanceof Error ? error.message : t("provider.testFailed"));
    }
  };

  const selectProvider = (kind: LlmProviderKind) => {
    const saved = providers.find((item) => item.config.provider === kind)?.config;
    setProvider(kind);
    setModel(saved?.model ?? "");
    setBaseUrl(saved?.baseUrl ?? defaultBaseUrls[kind]);
    setSecret("");
    setSaved(false);
    setTestStatus(null);
  };

  return (
    <div className="grid gap-3">
      <section className="panel">
        <div className="panel-header">{t("provider.title")}</div>
        <div className="mt-3 flex flex-col gap-2">
          {providerKinds.map((kind) => {
            const status = providers.find((item) => item.config.provider === kind);
            return <button key={kind} type="button" onClick={() => selectProvider(kind)} className="grid min-h-[44px] grid-cols-[120px_1fr_auto] items-center gap-3 rounded-[var(--radius-sm)] border border-[var(--border-subtle)] px-3 text-left text-[12px]">
              <span className="font-semibold">{kind}</span><span className="font-mono text-[11px] text-[var(--text-muted)]">{status?.config.model || t("provider.notConfigured")}</span><span>{status?.hasSecret ? t("provider.configured") : t("provider.noKey")}</span>
            </button>;
          })}
        </div>
      </section>
      <section className="panel grid gap-3">
        <div className="panel-header">{t("provider.configuration")}</div>
        <label className="grid gap-1 text-[12px]">{t("provider.model")}<input className="input" value={model} onChange={(event) => setModel(event.target.value)} /></label>
        <label className="grid gap-1 text-[12px]">{t("provider.baseUrl")}<input className="input" value={baseUrl} onChange={(event) => setBaseUrl(event.target.value)} /></label>
        <button type="button" className="h-[30px] rounded-[var(--radius-sm)] border border-[var(--border)] px-3 text-[12px]" onClick={() => void onSaveProvider({ provider, model, baseUrl, contextWindow: 32_000, enabled: true })}>{t("provider.saveProvider")}</button>
        <label className="grid gap-1 text-[12px]">{t("provider.apiKey")}<input aria-label="API key" type="password" autoComplete="off" className="input" value={secret} onChange={(event) => { setSaved(false); setSecret(event.target.value); }} /></label>
        <button type="button" disabled={!secret} className="h-[30px] rounded-[var(--radius-sm)] bg-[var(--foreground)] px-3 text-[12px] text-[var(--text-inverse)] disabled:opacity-50" onClick={() => void saveKey()}>{t("provider.saveKey")}</button>
        <div className="flex gap-2">
          <button type="button" className="h-[30px] rounded-[var(--radius-sm)] border border-[var(--border)] px-3 text-[12px]" onClick={() => void testProvider()}>{t("provider.test")}</button>
          <button type="button" className="h-[30px] rounded-[var(--radius-sm)] border border-[var(--danger)] px-3 text-[12px] text-[var(--danger)]" onClick={() => void onDeleteSecret?.(provider)}>{t("provider.deleteKey")}</button>
        </div>
        {saved ? <span className="text-[12px] text-[var(--accent)]">{t("provider.saved")}</span> : null}
        {testStatus ? <span role="status" className="text-[12px] text-[var(--text-muted)]">{testStatus}</span> : null}
      </section>
    </div>
  );
}
