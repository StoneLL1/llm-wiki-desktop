import type { LlmProviderKind } from "../../types/llm";
import { useTranslation } from "react-i18next";

export interface ProviderSecretRow {
  provider: LlmProviderKind;
  hasSecret: boolean;
}

interface SecuritySettingsProps {
  providers: ProviderSecretRow[];
  onDeleteSecret: (provider: ProviderSecretRow["provider"]) => void;
}

export function SecuritySettings({ providers, onDeleteSecret }: SecuritySettingsProps) {
  const { t } = useTranslation();

  return (
    <section className="grid gap-4">
      <div>
        <h2 className="m-0 text-[16px] font-semibold">{t("settings.security.title")}</h2>
        <p className="mt-1 text-[12px] text-[var(--text-muted)]">{t("settings.security.description")}</p>
      </div>

      <div className="grid gap-2">
        {providers.map((provider) => (
          <div
            key={provider.provider}
            className="grid min-h-[52px] grid-cols-[minmax(0,1fr)_auto] items-center gap-3 rounded-[var(--radius-md)] border border-[var(--border)] px-3 py-2"
          >
            <div>
              <div className="text-[13px] font-medium">{provider.provider}</div>
              <div className="mt-1 font-mono text-[11px] text-[var(--text-muted)]">
                {provider.hasSecret ? t("provider.configured") : t("settings.security.notConfigured")}
              </div>
            </div>
            <button
              type="button"
              disabled={!provider.hasSecret}
              className="settings-button settings-button--danger"
              onClick={() => onDeleteSecret(provider.provider)}
            >
              {t("settings.security.clear")}
            </button>
          </div>
        ))}
      </div>
    </section>
  );
}
