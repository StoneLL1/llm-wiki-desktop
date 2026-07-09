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
    <section className="settings-section-panel">
      <div>
        <h2 className="m-0 text-[16px] font-semibold">{t("settings.security.title")}</h2>
        <p className="mt-1 text-[12px] text-[var(--text-muted)]">{t("settings.security.description")}</p>
      </div>

      <div className="grid gap-2">
        <div className="settings-row-card settings-row-card--three">
          <div>
            <div className="settings-row-card__title">{t("settings.security.chatConvenience")}</div>
            <div className="settings-row-card__meta">
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
            className="settings-row-card"
          >
            <div>
              <div className="settings-row-card__title">{provider.provider}</div>
              <div className="settings-row-card__meta">
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
