import { Download, RefreshCw, ShieldCheck, X } from "lucide-react";
import { useTranslation } from "react-i18next";

import { ActionableErrorNotice } from "../../components/app/ActionableErrorNotice";
import { useModalDialog } from "../../hooks/useModalDialog";
import { useUpdateStore } from "../../stores/updateStore";
import type { UpdateInstallBlocker } from "../../types/update";

function formatBytes(value: number | null): string | null {
  if (value === null) return null;
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`;
  return `${(value / (1024 * 1024)).toFixed(1)} MB`;
}

export function UpdateDialog() {
  const { t, i18n } = useTranslation();
  const store = useUpdateStore();
  const dialogRef = useModalDialog({
    open: store.dialogOpen,
    onClose: store.closeDialog,
  });

  if (!store.dialogOpen) return null;

  const offer = store.backendState.offer;
  const total = formatBytes(store.backendState.totalBytes);
  const downloaded = formatBytes(store.backendState.downloadedBytes) ?? "0 B";
  const progress = store.backendState.totalBytes && store.backendState.totalBytes > 0
    ? Math.min(100, Math.round((store.backendState.downloadedBytes / store.backendState.totalBytes) * 100))
    : null;
  const reviewing = Boolean(store.installReviewIntent && store.installGuard);
  const blocked = Boolean(store.installGuard?.blockers.length);
  const primaryLabel = store.installReviewIntent === "restart"
    ? t("settings.updates.confirmRestart")
    : t("settings.updates.confirmInstall");

  return (
    <div
      ref={dialogRef}
      aria-labelledby="update-dialog-title"
      aria-modal="true"
      className="dialog-overlay"
      role="dialog"
      tabIndex={-1}
    >
      <div className="dialog dialog--wide">
        <header className="dialog__head">
          <div className="min-w-0">
            <h2 className="dialog__title m-0" id="update-dialog-title">{t("settings.updates.title")}</h2>
            <p className="m-0 mt-0.5 text-[11px] text-[var(--text-muted)]">
              {t(`settings.updates.status.${store.uiStatus}`)}
            </p>
          </div>
          <button
            aria-label={t("settings.updates.close")}
            className="btn btn--ghost btn--icon btn--sm ml-auto"
            onClick={store.closeDialog}
            type="button"
          >
            <X aria-hidden="true" size={16} />
          </button>
        </header>

        <div className="dialog__body" aria-live="polite">
          <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
            <div className="settings-detail-card">
              <div className="text-[11px] text-[var(--text-muted)]">{t("settings.updates.currentVersion")}</div>
              <div className="font-mono text-[13px]">{store.currentVersion}</div>
            </div>
            <div className="settings-detail-card">
              <div className="text-[11px] text-[var(--text-muted)]">{t("settings.updates.latestVersion")}</div>
              <div className="font-mono text-[13px]">
                {offer?.version ?? (store.uiStatus === "up_to_date" ? store.currentVersion : t("settings.updates.notChecked"))}
              </div>
            </div>
          </div>

          {offer ? (
            <section className="rounded-[var(--radius-md)] border border-[var(--border-subtle)] bg-[var(--surface)] p-3" aria-labelledby="update-release-title">
              <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
                <h3 className="m-0 text-[13px] font-semibold" id="update-release-title">
                  {t("settings.updates.release", { version: offer.version })}
                </h3>
                {offer.publishedAt ? (
                  <time className="text-[11px] text-[var(--text-muted)]" dateTime={offer.publishedAt}>
                    {new Date(offer.publishedAt).toLocaleDateString(i18n.language)}
                  </time>
                ) : null}
                <span className="font-mono text-[10.5px] text-[var(--text-muted)]">{offer.target} / {offer.arch}</span>
              </div>
              <p className="mt-2 flex items-start gap-2 text-[11px] leading-4 text-[var(--text-secondary)]">
                <ShieldCheck aria-hidden="true" className="mt-px shrink-0 text-[var(--accent)]" size={14} />
                {t("settings.updates.signedVerification")}
              </p>
              {offer.notes ? (
                <div className="mt-2 max-h-[180px] overflow-auto whitespace-pre-wrap rounded-[var(--radius-sm)] bg-[var(--surface-muted)] p-2 text-[12px] leading-5">
                  {offer.notes}
                </div>
              ) : null}
            </section>
          ) : null}

          {store.uiStatus === "downloading" || store.uiStatus === "paused_or_cancelled" || store.uiStatus === "ready_to_install" ? (
            <div className="space-y-1.5">
              <div className="flex justify-between text-[11px] text-[var(--text-muted)]">
                <span>{t("settings.updates.downloadProgress")}</span>
                <span>{total ? `${downloaded} / ${total}` : downloaded}</span>
              </div>
              <progress
                aria-label={t("settings.updates.downloadProgress")}
                className="h-1.5 w-full accent-[var(--accent)]"
                max={100}
                value={progress ?? undefined}
              />
            </div>
          ) : null}

          {store.error ? (
            <ActionableErrorNotice error={store.error} onAction={() => store.retry()} />
          ) : null}

          {reviewing ? (
            <section className="rounded-[var(--radius-md)] border border-[var(--warning-border)] bg-[var(--warning-subtle)] p-3" aria-labelledby="update-consent-title">
              <h3 className="m-0 text-[13px] font-semibold" id="update-consent-title">{t("settings.updates.restartConsentTitle")}</h3>
              <p className="mb-0 mt-1 text-[12px] leading-5">{t("settings.updates.restartConsentBody")}</p>
              {store.installGuard?.safeRunningTaskCount ? (
                <p className="mb-0 mt-1 text-[11px] text-[var(--text-muted)]">
                  {t("settings.updates.safeTasks", { count: store.installGuard.safeRunningTaskCount })}
                </p>
              ) : null}
              {store.installGuard?.blockers.length ? (
                <ul className="mb-0 mt-2 space-y-1 pl-4 text-[12px]" role="alert">
                  {store.installGuard.blockers.map((blocker: UpdateInstallBlocker) => (
                    <li key={blocker}>{t(`settings.updates.blocker.${blocker}`)}</li>
                  ))}
                </ul>
              ) : null}
            </section>
          ) : null}
        </div>

        <footer className="dialog__foot flex-wrap">
          {offer && ["available", "paused_or_cancelled"].includes(store.uiStatus) ? (
            <button className="btn btn--ghost mr-auto" onClick={() => void store.ignoreVersion()} type="button">
              {t("settings.updates.ignoreVersion")}
            </button>
          ) : null}
          <button className="btn" onClick={reviewing ? store.clearInstallReview : store.closeDialog} type="button">
            {reviewing ? t("confirmation.cancel") : t("settings.updates.remindLater")}
          </button>
          {store.uiStatus === "idle" || store.uiStatus === "up_to_date" || store.uiStatus === "error" ? (
            <button className="btn btn--primary" onClick={() => void store.checkNow().catch(() => undefined)} type="button">
              <RefreshCw aria-hidden="true" size={14} />{t("settings.updates.checkNow")}
            </button>
          ) : null}
          {store.uiStatus === "available" || store.uiStatus === "paused_or_cancelled" ? (
            <button className="btn btn--primary" onClick={() => void store.download().catch(() => undefined)} type="button">
              <Download aria-hidden="true" size={14} />{t("settings.updates.download")}
            </button>
          ) : null}
          {store.uiStatus === "downloading" ? (
            <button className="btn" onClick={() => void store.cancelDownload()} type="button">{t("settings.updates.cancelDownload")}</button>
          ) : null}
          {store.uiStatus === "ready_to_install" && !reviewing ? (
            <button className="btn btn--primary" onClick={() => void store.reviewInstall("install")} type="button">{t("settings.updates.install")}</button>
          ) : null}
          {store.uiStatus === "restart_required" && !reviewing ? (
            <button className="btn btn--primary" onClick={() => void store.reviewInstall("restart")} type="button">{t("settings.updates.restart")}</button>
          ) : null}
          {reviewing ? (
            <button
              className="btn btn--primary"
              disabled={blocked || store.uiStatus === "installing"}
              onClick={() => void store.confirmInstallOrRestart()}
              type="button"
            >
              {primaryLabel}
            </button>
          ) : null}
        </footer>
      </div>
    </div>
  );
}
