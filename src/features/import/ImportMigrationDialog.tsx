import { FileSearch, GitBranch, LoaderCircle, Play, RotateCcw, X } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";

import { useModalDialog } from "../../hooks/useModalDialog";
import type { MigrationConfirmation, MigrationGitCheckpoint, MigrationPlan, MigrationReport, MigrationStatus, LegacyInventory } from "../../types/importV2Migration";

export interface ImportMigrationDialogProps {
  open: boolean;
  status: ImportMigrationUiStatus;
  inventory?: LegacyInventory | null;
  plan: MigrationPlan | null;
  report: MigrationReport | null;
  confirmation: MigrationConfirmation | null;
  checkpoint: MigrationGitCheckpoint | null;
  resumable?: boolean;
  onScan: () => Promise<void> | void;
  onPlan: (inventory: LegacyInventory) => Promise<void> | void;
  onApply: (plan: MigrationPlan, confirmation: MigrationConfirmation) => Promise<void> | void;
  onResume: (plan: MigrationPlan, confirmation: MigrationConfirmation) => Promise<void> | void;
  onClose: () => void;
}

export type ImportMigrationUiStatus = MigrationStatus | "scanning" | "interrupted" | "resumable" | "not_activated" | "activated";

const STATUS_KEYS: Record<ImportMigrationUiStatus, string> = {
  not_scanned: "importV2.migration.status.not_scanned",
  scanning: "importV2.migration.status.scanning",
  dry_run_ready: "importV2.migration.status.dry_run_ready",
  awaiting_confirmation: "importV2.migration.status.awaiting_confirmation",
  applying: "importV2.migration.status.applying",
  interrupted: "importV2.migration.status.interrupted",
  resumable: "importV2.migration.status.resumable",
  applied: "importV2.migration.status.applied",
  not_activated: "importV2.migration.status.not_activated",
  activated: "importV2.migration.status.activated",
  verification_failed: "importV2.migration.status.verification_failed",
  cancelled: "importV2.migration.status.cancelled",
};

function countLine(label: string) {
  return <span className="font-mono text-[11px]">{label}</span>;
}

function CandidateList({ title, candidates }: { title: string; candidates: MigrationReport["automaticLinks"] }) {
  if (candidates.length === 0) return null;
  return (
    <section>
      <h3 className="import-v2-inspector-heading">{title}</h3>
      <ul className="m-0 list-disc space-y-1 pl-5 font-mono text-[11px]">
        {candidates.map((candidate) => <li key={candidate.candidateId}>{candidate.candidateId}{candidate.decision.kind === "legacyUnmanaged" || candidate.decision.kind === "conflict" ? ` — ${candidate.decision.reason}` : null}</li>)}
      </ul>
    </section>
  );
}

export function ImportMigrationDialog({ open, status, inventory = null, plan, report, confirmation, checkpoint, resumable = false, onScan, onPlan, onApply, onResume, onClose }: ImportMigrationDialogProps) {
  const { t } = useTranslation();
  const dialogRef = useModalDialog({ open, onClose });
  const [fingerprintConfirmed, setFingerprintConfirmed] = useState(false);
  const [noGitAcknowledged, setNoGitAcknowledged] = useState(false);
  const [busy, setBusy] = useState<"scan" | "plan" | "apply" | "resume" | null>(null);
  if (!open) return null;

  const reportFingerprint = report?.planFingerprint ?? confirmation?.planFingerprint ?? "—";
  const requiresNoGitAck = checkpoint === null && Boolean(plan);
  const readyToApply = Boolean(plan && report && confirmation && fingerprintConfirmed && (!requiresNoGitAck || noGitAcknowledged || confirmation?.acknowledgeNoGitRollback) && !busy);
  const statusLabel = t(STATUS_KEYS[status]);

  async function run(key: "scan" | "plan" | "apply" | "resume", action: () => Promise<void> | void) {
    setBusy(key);
    try { await action(); } finally { setBusy(null); }
  }

  function apply() {
    if (!readyToApply || !plan || !confirmation) return;
    void run("apply", () => onApply(plan, { ...confirmation, acknowledgeNoGitRollback: confirmation.acknowledgeNoGitRollback || noGitAcknowledged }));
  }

  function resume() {
    if (!plan || !confirmation || !fingerprintConfirmed || (requiresNoGitAck && !noGitAcknowledged && !confirmation.acknowledgeNoGitRollback)) return;
    void run("resume", () => onResume(plan, { ...confirmation, acknowledgeNoGitRollback: confirmation.acknowledgeNoGitRollback || noGitAcknowledged }));
  }

  return (
    <div ref={dialogRef} tabIndex={-1} className="fixed inset-0 z-50 flex items-center justify-center bg-black/20 px-4" role="dialog" aria-modal="true" aria-labelledby="import-migration-title">
      <section className="flex max-h-[88vh] w-full max-w-[860px] flex-col rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--surface-raised)] shadow-lg">
        <header className="flex min-h-[52px] items-center gap-3 border-b border-[var(--border)] px-4">
          <FileSearch size={17} className="text-[var(--accent)]" aria-hidden="true" />
          <div className="min-w-0 flex-1"><h2 id="import-migration-title" className="m-0 text-[15px] font-semibold">{t("importV2.migration.title")}</h2><p className="m-0 font-mono text-[10.5px] text-[var(--text-muted)]">{t("importV2.migration.status", { status: statusLabel })}</p></div>
          <button type="button" className="icon-button" aria-label={t("importV2.migration.close")} onClick={onClose}><X size={16} aria-hidden="true" /></button>
        </header>
        <div className="min-h-0 flex-1 overflow-y-auto space-y-4 px-4 py-4 text-[12px]">
          {status === "not_scanned" || status === "cancelled" ? <button type="button" className="btn btn--sm" disabled={busy !== null} onClick={() => void run("scan", onScan)}><FileSearch size={13} className="mr-1 inline" aria-hidden="true" />{t("importV2.migration.scan")}</button> : null}
          {inventory && !plan ? <button type="button" className="btn btn--sm" disabled={busy !== null} onClick={() => void run("plan", () => onPlan(inventory))}><FileSearch size={13} className="mr-1 inline" aria-hidden="true" />{t("importV2.migration.plan")}</button> : null}
          {report || plan ? <section aria-labelledby="migration-summary-title"><h3 id="migration-summary-title" className="import-v2-inspector-heading">{t("importV2.migration.summary")}</h3><div className="flex flex-wrap gap-x-4 gap-y-1">{countLine(t("importV2.migration.total", { count: report?.summary.total ?? plan?.summary.total ?? 0 }))}{countLine(t("importV2.migration.automaticLinks", { count: report?.summary.automaticLinks ?? plan?.summary.automaticLinks ?? 0 }))}{countLine(t("importV2.migration.proposedRecords", { count: report?.summary.proposedRecords ?? plan?.summary.proposedRecords ?? 0 }))}{countLine(t("importV2.migration.conflicts", { count: report?.summary.conflicts ?? plan?.summary.conflicts ?? 0 }))}{countLine(t("importV2.migration.unmanaged", { count: report?.summary.legacyUnmanaged ?? plan?.summary.legacyUnmanaged ?? 0 }))}{countLine(t("importV2.migration.warnings", { count: report?.summary.warnings ?? plan?.summary.warnings ?? 0 }))}</div></section> : null}
          {report ? <>
            <CandidateList title={t("importV2.migration.automaticLinks")} candidates={report.automaticLinks} />
            <CandidateList title={t("importV2.migration.proposedRecords")} candidates={report.proposedRecords} />
            <CandidateList title={t("importV2.migration.conflicts")} candidates={report.conflicts} />
            <CandidateList title={t("importV2.migration.unmanaged")} candidates={report.legacyUnmanaged} />
            <section><h3 className="import-v2-inspector-heading">{t("importV2.migration.metadataPaths")}</h3><ul className="m-0 list-disc pl-5 font-mono text-[11px]">{report.affectedMetadataPaths.map((path) => <li key={path}>{path}</li>)}</ul></section>
            <section><h3 className="import-v2-inspector-heading">{t("importV2.migration.untouchedPaths")}</h3><ul className="m-0 list-disc pl-5 font-mono text-[11px]">{report.untouchedContentPaths.map((path) => <li key={path}>{path}</li>)}</ul></section>
            <section><h3 className="import-v2-inspector-heading">{t("importV2.migration.rollback")}</h3><p className="m-0 text-[11px] text-[var(--text-secondary)]">{report.rollbackStatement}</p></section>
            {report.warnings.length ? <ul role="status" className="m-0 list-disc pl-5 text-[11px] text-[var(--warning-text)]">{report.warnings.map((warning) => <li key={`${warning.code}:${warning.relativePath ?? ""}`}>{warning.message}</li>)}</ul> : null}
          </> : null}
          {checkpoint ? <p className="m-0 flex items-center gap-1.5 text-[11px] text-[var(--success-text)]"><GitBranch size={13} aria-hidden="true" /><strong>{t("importV2.migration.checkpoint")}:</strong> {checkpoint.commitHash ?? checkpoint.message}</p> : null}
          {plan && confirmation ? <p className="m-0 font-mono text-[10.5px] text-[var(--text-muted)]">{t("importV2.migration.reportFingerprint", { fingerprint: reportFingerprint })}</p> : null}
          {plan && !confirmation ? <p role="alert" className="m-0 text-[11px] text-[var(--danger-text)]">{t("importV2.migration.confirmationMissing")}</p> : null}
          {plan && confirmation ? <div className="space-y-2 border-t border-[var(--border)] pt-3"><label className="flex items-start gap-2"><input type="checkbox" aria-label={t("importV2.migration.confirmFingerprint", { fingerprint: reportFingerprint })} checked={fingerprintConfirmed} onChange={(event) => setFingerprintConfirmed(event.target.checked)} disabled={busy !== null} /><span>{t("importV2.migration.confirmFingerprint", { fingerprint: reportFingerprint })}</span></label>{requiresNoGitAck ? <label className="flex items-start gap-2"><input type="checkbox" aria-label={t("importV2.migration.noGitAck")} checked={noGitAcknowledged} onChange={(event) => setNoGitAcknowledged(event.target.checked)} disabled={busy !== null} /><span>{t("importV2.migration.noGitAck")}</span></label> : null}</div> : null}
        </div>
        <footer className="flex flex-wrap items-center justify-end gap-2 border-t border-[var(--border)] px-4 py-3">
          {resumable && plan && confirmation ? <button type="button" className="btn btn--sm" onClick={resume} disabled={!fingerprintConfirmed || (requiresNoGitAck && !noGitAcknowledged && !confirmation.acknowledgeNoGitRollback) || busy !== null}><RotateCcw size={13} className="mr-1 inline" aria-hidden="true" />{t("importV2.migration.resume")}</button> : null}
          {plan && confirmation && status !== "applied" ? <button type="button" className="btn btn--sm btn--primary" onClick={apply} disabled={!readyToApply}><Play size={13} className="mr-1 inline" aria-hidden="true" />{busy === "apply" ? <LoaderCircle size={13} className="animate-spin" aria-label={t("importV2.common.loading")} /> : t("importV2.migration.apply")}</button> : null}
          <button type="button" className="btn btn--sm" onClick={onClose}>{t("importV2.migration.close")}</button>
        </footer>
      </section>
    </div>
  );
}
