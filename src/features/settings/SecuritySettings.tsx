import type { LlmProviderKind } from "../../types/llm";
import type { ChatConvenienceAuthorization } from "../../types/settings";
import { useTranslation } from "react-i18next";

export interface ProviderSecretRow {
  provider: LlmProviderKind;
  hasSecret: boolean;
}

interface SecuritySettingsProps {
  providers: ProviderSecretRow[];
  chatConvenienceAuthorization: ChatConvenienceAuthorization | null;
  onDeleteSecret: (provider: ProviderSecretRow["provider"]) => void;
  onRevokeChatConvenience: () => void;
  onRevokeAllChatConvenience: () => void;
}

export function SecuritySettings({
  providers,
  chatConvenienceAuthorization,
  onDeleteSecret,
  onRevokeChatConvenience,
  onRevokeAllChatConvenience,
}: SecuritySettingsProps) {
  const { t } = useTranslation();
  const chatConvenienceEnabled = Boolean(chatConvenienceAuthorization?.enabled);

  return (
    <section className="grid gap-4">
      <div>
        <h2 className="m-0 text-[16px] font-semibold">{t("settings.security.title")}</h2>
        <p className="mt-1 text-[12px] text-[var(--text-muted)]">{t("settings.security.description")}</p>
      </div>

      <div className="grid gap-2">
        <div className="grid min-h-[52px] grid-cols-[minmax(0,1fr)_auto_auto] items-center gap-3 rounded-[var(--radius-md)] border border-[var(--border)] px-3 py-2">
          <div>
            <div className="text-[13px] font-medium">{t("settings.security.chatConvenience")}</div>
            <div className="mt-1 font-mono text-[11px] text-[var(--text-muted)]">
              {chatConvenienceEnabled
                ? t("settings.security.chatConvenienceEnabled")
                : t("settings.security.chatConvenienceDisabled")}
            </div>
          </div>
          <button
            type="button"
            disabled={!chatConvenienceEnabled}
            className="settings-button settings-button--danger"
            onClick={onRevokeChatConvenience}
          >
            {t("settings.security.revokeCurrent")}
          </button>
          <button
            type="button"
            className="settings-button settings-button--danger"
            onClick={onRevokeAllChatConvenience}
          >
            {t("settings.security.revokeAll")}
          </button>
        </div>

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
