import { Check, GitCompareArrows, X } from "lucide-react";
import { useTranslation } from "react-i18next";

import { useModalDialog } from "../../hooks/useModalDialog";
import type { AgentCandidateView } from "../../types/importV2Agent";

export type ImportCandidateDiffIntentKind = "choose_deterministic" | "choose_agent" | "apply_merged" | "keep_current" | "create_new" | "discard";

export interface ImportCandidateDiffIntent {
  kind: ImportCandidateDiffIntentKind;
  candidateId: string;
}

export interface ImportCandidateDiffDialogProps {
  open: boolean;
  view: AgentCandidateView | null;
  onClose: () => void;
  onAction: (intent: ImportCandidateDiffIntent) => void;
}

function Evidence({ label, content }: { label: string; content: string | null }) {
  return (
    <section className="min-w-0 flex-1 border border-[var(--border)]">
      <h3 className="m-0 border-b border-[var(--border)] bg-[var(--surface-sunken)] px-2 py-1.5 text-[11px] font-semibold">{label}</h3>
      <pre className="m-0 max-h-[260px] overflow-auto whitespace-pre-wrap px-2 py-2 font-mono text-[11px] text-[var(--text-primary)]">{content ?? "—"}</pre>
    </section>
  );
}

export function ImportCandidateDiffDialog({ open, view, onClose, onAction }: ImportCandidateDiffDialogProps) {
  const { t } = useTranslation();
  const dialogRef = useModalDialog({ open, onClose });
  if (!open || !view) return null;
  const dispatch = (kind: ImportCandidateDiffIntentKind) => onAction({ kind, candidateId: view.candidate.candidateId });

  return (
    <div ref={dialogRef} tabIndex={-1} className="fixed inset-0 z-50 flex items-center justify-center bg-black/20 px-4" role="dialog" aria-modal="true" aria-labelledby="import-diff-title">
      <section className="flex max-h-[88vh] w-full max-w-[1080px] flex-col rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--surface-raised)] shadow-lg">
        <header className="flex min-h-[52px] items-center gap-3 border-b border-[var(--border)] px-4">
          <GitCompareArrows size={17} className="text-[var(--accent)]" aria-hidden="true" />
          <h2 id="import-diff-title" className="m-0 flex-1 text-[15px] font-semibold">{t("importV2.diff.title")}</h2>
          <button type="button" className="icon-button" aria-label={t("importV2.diff.close")} onClick={onClose}><X size={16} aria-hidden="true" /></button>
        </header>
        <div className="min-h-0 flex-1 overflow-y-auto px-4 py-4">
          <div className="flex flex-col gap-2 md:flex-row">
            <Evidence label={t("importV2.diff.baseline")} content={view.diff.baselineMarkdown} />
            <Evidence label={t("importV2.diff.current")} content={view.diff.currentMarkdown} />
            <Evidence label={t("importV2.diff.agent")} content={view.diff.agentMarkdown} />
          </div>
          <details className="mt-3 border border-[var(--border)]">
            <summary className="cursor-pointer px-2 py-1.5 text-[11px] font-semibold">{t("importV2.diff.patch")}</summary>
            <pre className="m-0 max-h-[180px] overflow-auto whitespace-pre-wrap border-t border-[var(--border)] px-2 py-2 font-mono text-[11px]">{view.diff.unifiedDiff}</pre>
          </details>
        </div>
        <footer className="flex flex-wrap items-center justify-end gap-1.5 border-t border-[var(--border)] px-4 py-3">
          <button type="button" className="btn btn--sm" onClick={() => dispatch("choose_deterministic")}><Check size={13} className="mr-1 inline" aria-hidden="true" />{t("importV2.diff.chooseDeterministic")}</button>
          <button type="button" className="btn btn--sm" onClick={() => dispatch("choose_agent")}>{t("importV2.diff.chooseAgent")}</button>
          {view.diff.needsThreeWayMerge ? <button type="button" className="btn btn--sm btn--primary" onClick={() => dispatch("apply_merged")}>{t("importV2.diff.applyMerged")}</button> : null}
          {view.diff.currentMarkdown !== null ? <button type="button" className="btn btn--sm" onClick={() => dispatch("keep_current")}>{t("importV2.diff.keepCurrent")}</button> : null}
          <button type="button" className="btn btn--sm" onClick={() => dispatch("create_new")}>{t("importV2.diff.createNew")}</button>
          <button type="button" className="btn btn--sm btn--ghost" onClick={() => dispatch("discard")}>{t("importV2.diff.discard")}</button>
        </footer>
      </section>
    </div>
  );
}
