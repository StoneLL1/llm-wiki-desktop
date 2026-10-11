import { useEffect, useRef, useState } from "react";
import type { LlmProviderKind } from "../../types/llm";
import type { ChatConvenienceAuthorization } from "../../types/settings";
import { useTranslation } from "react-i18next";
import { KeyRound, ShieldCheck } from "lucide-react";
import { ActionableErrorNotice } from "../../components/app/ActionableErrorNotice";
import { BrandMark } from "./BrandMark";

export interface ProviderSecretRow { provider: LlmProviderKind; hasSecret: boolean; }
interface SecuritySettingsProps {
  providers: ProviderSecretRow[];
  chatConvenienceAuthorization: ChatConvenienceAuthorization | null;
  onDeleteSecret: (provider: LlmProviderKind) => Promise<unknown> | unknown;
  onRevokeChatConvenience: () => Promise<unknown> | unknown;
  onRevokeAllChatConvenience: () => Promise<unknown> | unknown;
}

export function SecuritySettings({ providers, chatConvenienceAuthorization, onDeleteSecret, onRevokeChatConvenience, onRevokeAllChatConvenience }: SecuritySettingsProps) {
  const { t } = useTranslation();
  const mounted = useRef(true);
  const busyRef = useRef(false);
  const [pending, setPending] = useState<string | null>(null);
  const [feedback, setFeedback] = useState<{ id: string; error?: unknown; success?: boolean } | null>(null);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);
  const run = async (id: string, operation: () => Promise<unknown> | unknown) => {
    if (busyRef.current) return;
    busyRef.current = true;
    setPending(id);
    setFeedback(null);
    try { await operation(); if (mounted.current) setFeedback({ id, success: true }); }
    catch (error) { if (mounted.current) setFeedback({ id, error }); }
    finally { busyRef.current = false; if (mounted.current) setPending(null); }
  };
  const status = (id: string, retry: () => Promise<unknown> | unknown) => feedback?.id === id ? feedback.success
    ? <p className="settings-inline-status" role="status">{t("settings.security.actionComplete")}</p>
    : <ActionableErrorNotice error={feedback.error} onAction={() => run(id, retry)} /> : null;
  return <section className="settings-section-panel">
    <div><h2 className="settings-view__section-title">{t("settings.security.title")}</h2><p className="settings-view__section-desc">{t("settings.security.description")}</p></div>
    <div className="settings-security-group"><div className="settings-info-row"><div><strong>{t("settings.security.chatConvenience")}</strong><p>{t(chatConvenienceAuthorization?.enabled ? "settings.security.chatConvenienceEnabled" : "settings.security.chatConvenienceDisabled")}</p></div><ShieldCheck aria-hidden="true" size={18} /></div>
      <div className="settings-inline-actions"><button type="button" className="settings-button settings-button--secondary" disabled={Boolean(pending) || !chatConvenienceAuthorization?.enabled} onClick={() => void run("current", onRevokeChatConvenience)}>{pending === "current" ? t("backendError.action.working") : t("settings.security.revokeCurrent")}</button><button type="button" className="settings-button settings-button--secondary" disabled={Boolean(pending)} onClick={() => void run("all", onRevokeAllChatConvenience)}>{pending === "all" ? t("backendError.action.working") : t("settings.security.revokeAll")}</button></div>
      {status("current", onRevokeChatConvenience)}{status("all", onRevokeAllChatConvenience)}
    </div>
    <div className="settings-security-group"><h3 className="settings-subheading"><KeyRound size={14} aria-hidden="true" />{t("settings.security.credentials")}</h3>
      {providers.map(({ provider, hasSecret }) => <div key={provider}><div className="settings-credential-row"><BrandMark kind={provider} type="provider" /><div><strong>{t(`provider.name.${provider}`)}</strong><p>{t(hasSecret ? "settings.ai.connection.credentialSaved" : "settings.security.notConfigured")}</p></div><button type="button" className="settings-button settings-button--secondary" disabled={Boolean(pending) || !hasSecret} aria-label={t("settings.security.clearNamed", { provider: t(`provider.name.${provider}`) })} onClick={() => void run(provider, () => onDeleteSecret(provider))}>{pending === provider ? t("backendError.action.working") : t("settings.security.clear")}</button></div>{status(provider, () => onDeleteSecret(provider))}</div>)}
    </div>
  </section>;
}
