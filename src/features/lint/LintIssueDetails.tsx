import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";

import type {
  LintFixChoice,
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
}

function SectionTitle({ children }: { children: React.ReactNode }) {
  return (
    <h3 className="mb-2 mt-1 text-[10.5px] font-medium uppercase tracking-[0.08em] text-[var(--text-muted)]">
      {children}
    </h3>
  );
}

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-0.5">
      <span className="text-[10.5px] uppercase tracking-[0.08em] text-[var(--text-muted)]">{label}</span>
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
}: LintIssueDetailsProps) {
  const { t } = useTranslation();
  const hasSnapshot = Boolean(issue?.scanHash);
  const fixable = issue ? issue.fixability !== "none" && hasSnapshot : false;
  const [choice, setChoice] = useState<LintFixChoice>(fixable ? "fix" : "ignore");

  // Reset the fix-plan choice when switching issues, defaulting to "fix" when
  // the issue is auto-fixable and "ignore" otherwise.
  useEffect(() => {
    setChoice(
      fixConfirm?.issue.id === issue?.id || fixable ? "fix" : "ignore",
    );
  }, [issue?.id, issue?.fixability, hasSnapshot, fixable, fixConfirm?.issue.id]);

  if (!issue) {
    return (
      <div className="flex h-full items-center justify-center px-6 text-center text-[12px] text-[var(--text-muted)]">
        {t("lint.details.empty")}
      </div>
    );
  }

  const confirmForThisIssue = fixConfirm && fixConfirm.issue.id === issue.id ? fixConfirm : null;
  const effectiveChoice = fixConfirm ? "fix" : choice;
  const pageHash = confirmForThisIssue?.expectedHash || issue.scanHash || null;
  const preview = confirmForThisIssue?.pendingAction.preview ?? null;
  const applyHint =
    issue.fixability === "high_risk"
      ? t("lint.plan.applyHighRiskHint")
      : t("lint.plan.applySafeHint");

  return (
    <div className="lint-view__details flex h-full flex-col overflow-y-auto">
      <div className="border-b border-[var(--border)] px-4 py-3">
        <div className="flex items-center gap-2">
          <span className="text-[13px] font-semibold text-[var(--text-primary)]">
            {t(`lint.issueType.${issue.issueType}`)}
          </span>
          <span className="rounded-[var(--radius-sm)] bg-[var(--surface-muted)] px-1.5 py-0.5 text-[10.5px] text-[var(--text-muted)]">
            {t(`lint.source.${issue.source}`)}
          </span>
        </div>
      </div>

      <div className="flex flex-col gap-3 px-4 py-3">
        <Row label={t("lint.details.path")}>
          <span className="font-mono text-[11.5px]">{issue.path}</span>
        </Row>
        {issue.scanHash ? (
          <Row label={t("lint.details.scanBaseline")}>
            <span className="font-mono text-[10.5px]" title={issue.scanHash}>
              {issue.scanHash.slice(0, 12)}
            </span>
          </Row>
        ) : null}
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

      {/* Fix plan: apply fix vs ignore this issue. */}
      <div className="flex flex-col gap-1 px-4 pb-3">
        <SectionTitle>{t("lint.plan.title")}</SectionTitle>
        <label className={`check-row ${effectiveChoice === "fix" ? "is-selected" : ""}`}>
          <input
            type="radio"
            name={`lint-fix-${issue.id}`}
            checked={effectiveChoice === "fix"}
            disabled={!fixable || Boolean(fixConfirm)}
            onChange={() => setChoice("fix")}
          />
          <div className="min-w-0 flex-1">
            <div className="font-medium text-[var(--text-primary)]">{t("lint.plan.apply")}</div>
            <div className="font-mono text-[11px] text-[var(--text-muted)]">{applyHint}</div>
          </div>
          {fixable && issue.fixability === "high_risk" ? (
            <span className="badge badge--danger">{t("lint.tag.highRisk")}</span>
          ) : null}
        </label>
        <label className={`check-row ${effectiveChoice === "ignore" ? "is-selected" : ""}`}>
          <input
            type="radio"
            name={`lint-fix-${issue.id}`}
            checked={effectiveChoice === "ignore"}
            disabled={Boolean(fixConfirm) || actionsDisabled}
            onChange={() => setChoice("ignore")}
          />
          <div className="min-w-0 flex-1">
            <div className="font-medium text-[var(--text-primary)]">{t("lint.plan.ignore")}</div>
            <div className="font-mono text-[11px] text-[var(--text-muted)]">{t("lint.plan.ignoreHint")}</div>
          </div>
        </label>
      </div>

      {/* Safety checks: checkpoint + commit are hard boundaries (always on). */}
      <div className="flex flex-col gap-1 px-4 pb-3">
        <SectionTitle>{t("lint.safety.title")}</SectionTitle>
        <label className="check-row" style={{ cursor: "default" }}>
          <input type="checkbox" checked disabled onChange={() => undefined} />
          <span className="flex-1 text-[var(--text-primary)]">{t("lint.safety.checkpoint")}</span>
          <span className="text-[10.5px] text-[var(--text-muted)]">{t("lint.safety.mandatory")}</span>
        </label>
        <label className="check-row" style={{ cursor: "default" }}>
          <input type="checkbox" checked disabled onChange={() => undefined} />
          <span className="flex-1 text-[var(--text-primary)]">{t("lint.safety.commit")}</span>
          <span className="text-[10.5px] text-[var(--text-muted)]">{t("lint.safety.mandatory")}</span>
        </label>
        <label className="check-row">
          <input
            type="checkbox"
            checked={safetyPrefs.recompile}
            onChange={(event) => onSafetyPrefsChange({ recompile: event.target.checked })}
          />
          <span className="flex-1 text-[var(--text-primary)]">{t("lint.safety.recompile")}</span>
        </label>
      </div>

      <div className="mt-auto border-t border-[var(--border)] px-4 py-3">
        {effectiveChoice === "ignore" ? (
          <button
            type="button"
            disabled={ignoring || actionsDisabled || Boolean(fixConfirm)}
            onClick={() => onIgnore(issue)}
            className="h-[28px] w-full rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface-raised)] text-[12px] hover:bg-[var(--surface-muted)] disabled:opacity-40"
          >
            {ignoring ? "…" : t("lint.plan.ignoreAction")}
          </button>
        ) : fixStatus === "applied" ? (
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
                  <span className="text-[10.5px] uppercase tracking-[0.08em] text-[var(--text-muted)]">
                    {t("lint.details.before")}
                  </span>
                  <code className="block max-h-[120px] overflow-y-auto whitespace-pre-wrap rounded-[var(--radius-sm)] bg-[var(--surface-muted)] px-2 py-1 font-mono text-[11px]">
                    {preview.before ?? ""}
                  </code>
                </div>
                <div className="flex flex-col gap-1">
                  <span className="text-[10.5px] uppercase tracking-[0.08em] text-[var(--text-muted)]">
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
                className="btn--block h-[28px] rounded-[var(--radius-md)] bg-[var(--foreground)] px-3 text-[12px] font-medium text-[var(--text-inverse)] hover:bg-[var(--primary-hover)] disabled:opacity-40"
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
            className="btn--block h-[28px] rounded-[var(--radius-md)] bg-[var(--foreground)] px-3 text-[12px] font-medium text-[var(--text-inverse)] hover:bg-[var(--primary-hover)] disabled:opacity-40"
          >
            {fixStatus === "applying" ? "…" : t("lint.details.applyFix")}
          </button>
        )}
      </div>
    </div>
  );
}
