import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";

import type { LintFixConfirmRequest, LintIssue } from "../../types/lint";
import type { WikiPageContent } from "../../types/wiki";

interface LintIssueDetailsProps {
  issue: LintIssue | null;
  fixStatus: "idle" | "applying" | "applied" | "error";
  fixConfirm: LintFixConfirmRequest | null;
  projectId: string;
  rootPath: string;
  onApplyFix: (issue: LintIssue) => void;
  onConfirmHighRisk: (expectedHash: string) => void;
  onCancelHighRisk: () => void;
}

const FIXABILITY_LABEL: Record<LintIssue["fixability"], string> = {
  none: "lint.fixability.none",
  safe: "lint.fixability.safe",
  high_risk: "lint.fixability.highRisk",
};

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
  projectId,
  rootPath,
  onApplyFix,
  onConfirmHighRisk,
  onCancelHighRisk,
}: LintIssueDetailsProps) {
  const { t } = useTranslation();
  const [pageHash, setPageHash] = useState<string | null>(null);

  // Resolve the live page hash whenever the confirm panel opens, so the
  // high-risk apply can pass an optimistic-concurrency guard to the backend.
  useEffect(() => {
    setPageHash(null);
    if (!fixConfirm || !issue) return;
    let cancelled = false;
    void invoke<WikiPageContent>("read_wiki_page", {
      request: { projectId, projectRootPath: rootPath, relativePath: issue.path },
    }).then((page) => {
      if (!cancelled) setPageHash(page.meta.hash);
    }).catch(() => {
      if (!cancelled) setPageHash(null);
    });
    return () => {
      cancelled = true;
    };
  }, [fixConfirm, issue, projectId, rootPath]);

  if (!issue) {
    return (
      <div className="flex h-full items-center justify-center px-6 text-center text-[12px] text-[var(--text-muted)]">
        {t("lint.details.empty")}
      </div>
    );
  }

  const confirmForThisIssue = fixConfirm && fixConfirm.issue.id === issue.id ? fixConfirm : null;
  const preview = confirmForThisIssue?.pendingAction.preview ?? null;

  return (
    <div className="flex h-full flex-col overflow-y-auto">
      <div className="border-b border-[var(--border)] px-4 py-3">
        <div className="flex items-center gap-2">
          <span className="text-[13px] font-semibold text-[var(--text-primary)]">
            {t(`lint.issueType.${issue.issueType}`)}
          </span>
          <span className="rounded-[var(--radius-sm)] bg-[var(--surface-muted)] px-1.5 py-0.5 text-[10.5px] text-[var(--text-muted)]">
            {t(`lint.source.${issue.source}`)}
          </span>
          <span className="rounded-[var(--radius-sm)] bg-[var(--surface-muted)] px-1.5 py-0.5 text-[10.5px] text-[var(--text-muted)]">
            {t(FIXABILITY_LABEL[issue.fixability])}
          </span>
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
        <p className="m-0 text-[11px] leading-5 text-[var(--text-muted)]">
          {t("lint.details.checkpointNote")}
        </p>
      </div>

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
                disabled={!pageHash || fixStatus === "applying"}
                onClick={() => pageHash && onConfirmHighRisk(pageHash)}
                className="h-[28px] rounded-[var(--radius-md)] bg-[var(--foreground)] px-3 text-[12px] font-medium text-[var(--text-inverse)] hover:bg-[var(--primary-hover)] disabled:opacity-40"
              >
                {fixStatus === "applying" ? "…" : t("lint.details.confirm")}
              </button>
              <button
                type="button"
                onClick={onCancelHighRisk}
                className="h-[28px] rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface-raised)] px-3 text-[12px] hover:bg-[var(--surface-muted)]"
              >
                {t("lint.details.cancel")}
              </button>
            </div>
          </div>
        ) : issue.fixability === "none" ? (
          <span className="text-[12px] text-[var(--text-muted)]">{t("lint.details.notAutoFixable")}</span>
        ) : (
          <button
            type="button"
            disabled={fixStatus === "applying"}
            onClick={() => onApplyFix(issue)}
            className="h-[28px] rounded-[var(--radius-md)] bg-[var(--foreground)] px-3 text-[12px] font-medium text-[var(--text-inverse)] hover:bg-[var(--primary-hover)] disabled:opacity-40"
          >
            {fixStatus === "applying" ? "…" : t("lint.details.applyFix")}
          </button>
        )}
      </div>
    </div>
  );
}
