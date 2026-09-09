import { useEffect, useRef } from "react";
import { ArrowLeft, FileSearch } from "lucide-react";
import { useTranslation } from "react-i18next";

import type {
  LintFixConfirmRequest,
  LintIssue,
  LintSafetyPrefs,
} from "../../types/lint";

interface LintIssueDetailsProps {
  issue: LintIssue | null;
  fixStatus: "idle" | "applying" | "applied" | "error";
  fixConfirm: LintFixConfirmRequest | null;
  ignoring: boolean;
  actionsDisabled?: boolean;
  safetyPrefs: LintSafetyPrefs;
  onSafetyPrefsChange: (prefs: Partial<LintSafetyPrefs>) => void;
  onApplyFix: (issue: LintIssue) => void;
  onConfirmHighRisk: (expectedHash: string) => void;
  onCancelHighRisk: () => void;
  onIgnore: (issue: LintIssue) => void;
  onBack?: () => void;
}

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-0.5">
      <span className="text-[11px] text-[var(--text-muted)]">{label}</span>
      <span className="text-[12px] leading-5 text-[var(--text-primary)]">{children}</span>
    </div>
  );
}

export function LintIssueDetails({
  issue,
  fixStatus,
  fixConfirm,
  ignoring,
  actionsDisabled = false,
  safetyPrefs,
  onSafetyPrefsChange,
  onApplyFix,
  onConfirmHighRisk,
  onCancelHighRisk,
  onIgnore,
  onBack,
}: LintIssueDetailsProps) {
  const { t } = useTranslation();
  const headingRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (issue && window.matchMedia?.("(max-width: 980px)").matches) headingRef.current?.focus();
  }, [issue?.id]);
  const hasSnapshot = Boolean(issue?.scanHash);
  const fixable = issue ? issue.fixability !== "none" && hasSnapshot : false;
  if (!issue) {
    return (
      <div className="lint-view__details lint-empty">
        <FileSearch size={22} aria-hidden="true" /><p>{t("lint.details.empty")}</p>
      </div>
    );
  }

  const confirmForThisIssue = fixConfirm && fixConfirm.issue.id === issue.id ? fixConfirm : null;
  const pageHash = confirmForThisIssue?.expectedHash || issue.scanHash || null;
  const preview = confirmForThisIssue?.pendingAction.preview ?? null;

  return (
    <div className="lint-view__details flex h-full flex-col overflow-y-auto">
      <div ref={headingRef} tabIndex={-1} className="lint-detail-heading border-b border-[var(--border)] px-4 py-3">
        {onBack ? <button type="button" className="btn btn--ghost btn--sm lint-details-back" onClick={onBack}><ArrowLeft size={14} aria-hidden="true" />{t("lint.details.back")}</button> : null}
        <div className="flex items-center gap-2">
          <span className="text-[13px] font-semibold text-[var(--text-primary)]">
            {t(`lint.issueType.${issue.issueType}`)}
          </span>
          {issue.fixability === "high_risk" ? <span className="badge badge--warn">{t("lint.tag.highRisk")}</span> : null}
          {(issue.origins ?? [issue.source]).map((source) => (
            <span
              key={source}
              className="rounded-[var(--radius-sm)] bg-[var(--surface-muted)] px-1.5 py-0.5 text-[10.5px] text-[var(--text-muted)]"
            >
              {t(`lint.source.${source}`)}
            </span>
          ))}
        </div>
      </div>

      <div className="flex flex-col gap-3 px-4 py-3">
        <Row label={t("lint.details.path")}>
          <span className="font-mono text-[11.5px]">{issue.path}</span>
        </Row>
        <Row label={t("lint.details.message")}>{issue.message}</Row>
        {issue.target ? (
          <Row label={t("lint.details.target")}>
            <span className="font-mono text-[11.5px]">{issue.target}</span>
          </Row>
        ) : null}
        {issue.range ? (
          <Row label={t("lint.details.line")}>{issue.range.line}</Row>
        ) : null}
        {issue.evidence ? (
          <Row label={t("lint.details.evidence")}>
            <code className="block whitespace-pre-wrap rounded-[var(--radius-sm)] bg-[var(--surface-muted)] px-2 py-1 font-mono text-[11px]">
              {issue.evidence}
            </code>
          </Row>
        ) : null}
        {issue.suggestedAction ? (
          <Row label={t("lint.details.suggestedAction")}>{issue.suggestedAction}</Row>
        ) : null}
      </div>

      {fixable ? (
        <div className="lint-fix-options">
          <p>{t("lint.safety.recoveryNote")}</p>
          <label>
            <input type="checkbox" checked={safetyPrefs.recompile} disabled={actionsDisabled || Boolean(fixConfirm)}
              onChange={(event) => onSafetyPrefsChange({ recompile: event.target.checked })} />
            {t("lint.safety.recompile")}
          </label>
        </div>
      ) : null}

      <div className="mt-auto border-t border-[var(--border)] px-4 py-3">
        {fixStatus === "applied" ? (
          <span className="text-[12px] text-[var(--text-muted)]">{t("lint.details.applied")}</span>
        ) : confirmForThisIssue ? (
          <div className="flex flex-col gap-2">
            <div className="text-[12px] font-medium">{t("lint.details.confirmTitle")}</div>
            <p className="m-0 text-[11.5px] leading-5 text-[var(--text-secondary)]">
              {confirmForThisIssue.pendingAction.message}
            </p>
            {preview ? (
              <div className="grid grid-cols-2 gap-2">
                <div className="flex flex-col gap-1">
                  <span className="text-[11px] text-[var(--text-muted)]">
                    {t("lint.details.before")}
                  </span>
                  <code className="block max-h-[120px] overflow-y-auto whitespace-pre-wrap rounded-[var(--radius-sm)] bg-[var(--surface-muted)] px-2 py-1 font-mono text-[11px]">
                    {preview.before ?? ""}
                  </code>
                </div>
                <div className="flex flex-col gap-1">
                  <span className="text-[11px] text-[var(--text-muted)]">
                    {t("lint.details.after")}
                  </span>
                  <code className="block max-h-[120px] overflow-y-auto whitespace-pre-wrap rounded-[var(--radius-sm)] bg-[var(--surface-muted)] px-2 py-1 font-mono text-[11px]">
                    {preview.after ?? ""}
                  </code>
                </div>
              </div>
            ) : null}
            <div className="flex gap-2">
              <button
                type="button"
                disabled={!pageHash || fixStatus === "applying" || actionsDisabled}
                onClick={() => pageHash && onConfirmHighRisk(pageHash)}
                className="btn--block h-[28px] rounded-[var(--radius-md)] lint-primary bg-[var(--foreground)] px-3 text-[12px] font-medium text-[var(--text-inverse)] hover:bg-[var(--primary-hover)] disabled:opacity-40"
              >
                {fixStatus === "applying" ? "…" : t("lint.details.confirm")}
              </button>
              <button
                type="button"
                onClick={onCancelHighRisk}
                disabled={fixStatus === "applying" || actionsDisabled}
                className="btn--block h-[28px] rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface-raised)] px-3 text-[12px] hover:bg-[var(--surface-muted)]"
              >
                {t("lint.details.cancel")}
              </button>
            </div>
          </div>
        ) : issue.fixability === "none" ? (
          <span className="text-[12px] text-[var(--text-muted)]">{t("lint.details.notAutoFixable")}</span>
        ) : !hasSnapshot ? (
          <span className="text-[12px] text-[var(--text-muted)]">{t("lint.details.rescanRequired")}</span>
        ) : (
          <button
            type="button"
            disabled={
              fixStatus === "applying" || actionsDisabled || Boolean(fixConfirm)
            }
            onClick={() => onApplyFix(issue)}
            className="btn--block h-[28px] rounded-[var(--radius-md)] lint-primary bg-[var(--foreground)] px-3 text-[12px] font-medium text-[var(--text-inverse)] hover:bg-[var(--primary-hover)] disabled:opacity-40"
          >
            {fixStatus === "applying" ? "…" : t("lint.details.applyFix")}
          </button>
        )}
        {!fixConfirm ? (
          <button type="button" className="btn btn--ghost btn--sm lint-ignore-action"
            title={t("lint.plan.ignoreHint")} disabled={ignoring || actionsDisabled}
            onClick={() => onIgnore(issue)}>
            {ignoring ? t("lint.details.ignoring") : t("lint.plan.ignoreAction")}
          </button>
        ) : null}
      </div>
    </div>
  );
}
