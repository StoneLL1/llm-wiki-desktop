import { CheckCircle2, ExternalLink, KeyRound, LoaderCircle, LogIn, X } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";

import { useModalDialog } from "../../hooks/useModalDialog";
import { translateBackendError } from "../../lib/backendError";
import type { WebAuthState } from "../../types/importV2Web";
import type { ConnectorSessionRef } from "../../types/importV2Presentation";

export interface ImportLoginDialogProps {
  open: boolean;
  platform: string;
  publicDomain: string;
  authState: WebAuthState;
  connectorSession: ConnectorSessionRef | null;
  onBeginLogin: () => Promise<ConnectorSessionRef | void>;
  onCheckAgain: (connectorSessionId: string) => Promise<ConnectorSessionRef | void>;
  onRevoke: (connectorSessionId: string) => Promise<void>;
  onCancel: () => void;
}

export function ImportLoginDialog({ open, platform, publicDomain, authState, connectorSession, onBeginLogin, onCheckAgain, onRevoke, onCancel }: ImportLoginDialogProps) {
  const { t } = useTranslation();
  const dialogRef = useModalDialog({ open, onClose: onCancel });
  const [currentSession, setCurrentSession] = useState<ConnectorSessionRef | null>(connectorSession);
  const [busy, setBusy] = useState<"begin" | "check" | "revoke" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [lastAction, setLastAction] = useState<"begin" | "check" | "revoke">("begin");
  useEffect(() => setCurrentSession(connectorSession), [connectorSession]);
  if (!open) return null;

  async function begin() {
    setBusy("begin");
    setLastAction("begin");
    setError(null);
    try {
      const next = await onBeginLogin();
      if (next) setCurrentSession(next);
    } catch (nextError) {
      setError(translateBackendError(nextError, t));
    } finally {
      setBusy(null);
    }
  }
  async function check() {
    if (!currentSession) return;
    setBusy("check");
    setLastAction("check");
    setError(null);
    try {
      const next = await onCheckAgain(currentSession.sessionId);
      if (next) setCurrentSession(next);
    } catch (nextError) {
      setError(translateBackendError(nextError, t));
    } finally {
      setBusy(null);
    }
  }
  async function revoke() {
    if (!currentSession) return;
    setBusy("revoke");
    setLastAction("revoke");
    setError(null);
    try {
      await onRevoke(currentSession.sessionId);
      setCurrentSession(null);
    } catch (nextError) {
      setError(translateBackendError(nextError, t));
    } finally {
      setBusy(null);
    }
  }

  function retryLastAction() {
    if (lastAction === "begin") void begin();
    else if (lastAction === "check") void check();
    else void revoke();
  }

  const state = currentSession?.state ?? authState;
  const verifiedAt = currentSession?.lastVerifiedAt
    ? new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" })
      .format(new Date(currentSession.lastVerifiedAt))
    : "—";
  return (
    <div ref={dialogRef} tabIndex={-1} className="fixed inset-0 z-50 flex items-center justify-center bg-black/20 px-4" role="dialog" aria-modal="true" aria-labelledby="import-login-title">
      <section className="w-full max-w-[620px] rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--surface-raised)] shadow-lg">
        <header className="flex min-h-[52px] items-center gap-3 border-b border-[var(--border)] px-4">
          <KeyRound size={17} className="text-[var(--accent)]" aria-hidden="true" />
          <h2 id="import-login-title" className="m-0 flex-1 text-[15px] font-semibold">{t("importV2.login.title")}</h2>
          <button type="button" className="icon-button" aria-label={t("importV2.login.cancel")} title={t("importV2.login.cancel")} onClick={onCancel}><X size={16} aria-hidden="true" /></button>
        </header>
        <div className="space-y-3 px-4 py-4 text-[12px]">
          <dl className="grid grid-cols-[110px_1fr] gap-x-4 gap-y-1.5">
            <dt className="text-[var(--text-muted)]">{t("importV2.login.connector")}</dt><dd className="m-0">{platform}</dd>
            <dt className="text-[var(--text-muted)]">{t("importV2.login.domain")}</dt><dd className="m-0 font-mono text-[11px]">{publicDomain}</dd>
            <dt className="text-[var(--text-muted)]">{t("importV2.login.state")}</dt><dd className="m-0">{t(`importV2.login.stateValue.${state}`, { defaultValue: state })}</dd>
            <dt className="text-[var(--text-muted)]">{t("importV2.login.account")}</dt><dd className="m-0">{currentSession?.accountSummary ?? "—"}</dd>
            <dt className="text-[var(--text-muted)]">{t("importV2.login.lastVerified")}</dt><dd className="m-0">{verifiedAt}</dd>
          </dl>
          <p className="m-0 rounded-[var(--radius-md)] border border-[var(--border-subtle)] px-3 py-2 text-[11px] text-[var(--text-muted)]">{t("importV2.login.profile")}</p>
          {authState === "captcha_required" || state === "captcha_required" ? <p className="m-0 rounded-[var(--radius-md)] border border-[var(--warning)] px-3 py-2 text-[11px] text-[var(--warning-text)]">{t("importV2.login.captcha")}</p> : null}
          {state === "authenticated" ? <p className="m-0 flex items-center gap-1.5 text-[var(--success-text)]" role="status"><CheckCircle2 size={14} aria-hidden="true" />{t("importV2.login.authenticated")}</p> : null}
          {error ? <div role="alert" className="flex items-start gap-2 rounded-[var(--radius-md)] border border-[var(--danger)] bg-[var(--danger-soft)] px-3 py-2 text-[11px] text-[var(--danger-text)]"><span className="min-w-0 flex-1">{error}</span><button type="button" className="btn btn--sm" onClick={retryLastAction} disabled={busy !== null}>{t("importV2.login.retry")}</button></div> : null}
        </div>
        <footer className="flex flex-wrap items-center justify-end gap-2 border-t border-[var(--border)] px-4 py-3">
          <button type="button" className="btn btn--sm" onClick={onCancel} disabled={busy !== null}>{t("importV2.login.cancel")}</button>
          {!currentSession ? <button type="button" className="btn btn--sm btn--primary" onClick={() => void begin()} disabled={busy !== null}><LogIn size={13} className="mr-1 inline" aria-hidden="true" />{busy === "begin" ? <LoaderCircle size={13} className="animate-spin" aria-label={t("importV2.login.loading")} /> : t("importV2.login.begin")}</button> : null}
          {currentSession ? <button type="button" className="btn btn--sm" onClick={() => void check()} disabled={busy !== null}><ExternalLink size={13} className="mr-1 inline" aria-hidden="true" />{busy === "check" ? <LoaderCircle size={13} className="animate-spin" aria-label={t("importV2.login.loading")} /> : t("importV2.login.check")}</button> : null}
          {currentSession ? <button type="button" className="btn btn--sm btn--ghost" onClick={() => void revoke()} disabled={busy !== null}>{busy === "revoke" ? <LoaderCircle size={13} className="mr-1 inline animate-spin" aria-label={t("importV2.login.loading")} /> : null}{t("importV2.login.revoke")}</button> : null}
        </footer>
      </section>
    </div>
  );
}
