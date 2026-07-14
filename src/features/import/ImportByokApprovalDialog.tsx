import { AlertTriangle, Check, KeyRound, X } from "lucide-react";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";

import { useModalDialog } from "../../hooks/useModalDialog";
import type { AgentSendScope } from "../../types/importV2Agent";

export interface ImportByokApprovalDialogProps {
  open: boolean;
  scope: AgentSendScope | null;
  onCancel: () => void;
  onConfirm: (scope: AgentSendScope, acknowledgePossibleDuplicateCharge: boolean) => Promise<void> | void;
}

function formatMicros(value: number | null): string {
  if (value === null) return "—";
  return `$${(value / 1_000_000).toFixed(4)}`;
}

export function ImportByokApprovalDialog({ open, scope, onCancel, onConfirm }: ImportByokApprovalDialogProps) {
  const { t } = useTranslation();
  const dialogRef = useModalDialog({ open, onClose: onCancel });
  const [acknowledged, setAcknowledged] = useState(false);
  const [busy, setBusy] = useState(false);
  const expired = useMemo(() => Boolean(scope && Date.parse(scope.expiresAt) <= Date.now()), [scope]);

  if (!open || !scope) return null;

  const canConfirm = !expired && (!scope.requiresDuplicateChargeAcknowledgement || acknowledged) && !busy;
  async function confirm() {
    if (!canConfirm) return;
    setBusy(true);
    try {
      await onConfirm(scope!, acknowledged);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div ref={dialogRef} tabIndex={-1} className="fixed inset-0 z-50 flex items-center justify-center bg-black/20 px-4" role="dialog" aria-modal="true" aria-labelledby="import-byok-title">
      <section className="flex max-h-[84vh] w-full max-w-[720px] flex-col rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--surface-raised)] shadow-lg">
        <header className="flex min-h-[52px] items-center gap-3 border-b border-[var(--border)] px-4">
          <KeyRound size={17} className="text-[var(--accent)]" aria-hidden="true" />
          <h2 id="import-byok-title" className="m-0 flex-1 text-[15px] font-semibold">{t("importV2.byok.title")}</h2>
          <button type="button" className="icon-button" aria-label={t("importV2.byok.cancel")} onClick={onCancel}><X size={16} aria-hidden="true" /></button>
        </header>
        <div className="min-h-0 flex-1 overflow-y-auto space-y-3 px-4 py-4 text-[12px]">
          {expired ? <div className="flex items-start gap-2 rounded-[var(--radius-md)] border border-[var(--danger)] px-3 py-2 text-[var(--danger-text)]" role="alert"><AlertTriangle size={15} aria-hidden="true" />{t("importV2.byok.expired")}</div> : null}
          <dl className="grid grid-cols-[minmax(110px,auto)_1fr] gap-x-4 gap-y-1.5">
            <dt className="text-[var(--text-muted)]">{t("importV2.byok.provider")}</dt><dd className="m-0">{scope.provider}</dd>
            <dt className="text-[var(--text-muted)]">{t("importV2.byok.model")}</dt><dd className="m-0">{scope.model}</dd>
            <dt className="text-[var(--text-muted)]">{t("importV2.byok.destination")}</dt><dd className="m-0 font-mono text-[11px]">{scope.destination}</dd>
            <dt className="text-[var(--text-muted)]">{t("importV2.byok.tokens", { count: scope.estimatedInputTokens.toLocaleString() })}</dt><dd className="m-0">{scope.estimatedInputTokens.toLocaleString()}</dd>
            <dt className="text-[var(--text-muted)]">{t("importV2.byok.estimatedCost", { cost: formatMicros(scope.estimatedCostMicros) })}</dt><dd className="m-0">{formatMicros(scope.estimatedCostMicros)}</dd>
            <dt className="text-[var(--text-muted)]">{t("importV2.byok.expires", { date: new Date(scope.expiresAt).toLocaleString() })}</dt><dd className="m-0 font-mono text-[11px]">{t("importV2.byok.scopeHash", { hash: scope.scopeSha256 })}</dd>
          </dl>
          <div>
            <h3 className="m-0 mb-1 text-[12px] font-semibold">{t("importV2.byok.files")}</h3>
            <ul className="m-0 space-y-1 pl-4">
              {scope.files.map((file) => <li key={`${file.relativePath}:${file.sha256}`}><span className="font-mono text-[11px]">{file.relativePath}</span> · {t("importV2.byok.bytes", { count: file.sizeBytes.toLocaleString() })} · {file.estimatedTokens.toLocaleString()} tokens{file.redactions.length ? ` · ${file.redactions.join(", ")}` : ""}</li>)}
            </ul>
          </div>
          {scope.publicMetadata.length ? <p className="m-0 text-[11px] text-[var(--text-muted)]">{t("importV2.byok.metadata")}: {scope.publicMetadata.join(" · ")}</p> : null}
          {scope.requiresDuplicateChargeAcknowledgement ? <label className="flex items-start gap-2 rounded-[var(--radius-md)] border border-[var(--border-subtle)] px-3 py-2"><input type="checkbox" aria-label={t("importV2.byok.duplicateCharge")} checked={acknowledged} onChange={(event) => setAcknowledged(event.target.checked)} disabled={expired || busy} /><span>{t("importV2.byok.duplicateCharge")}</span></label> : null}
        </div>
        <footer className="flex min-h-[52px] items-center justify-end gap-2 border-t border-[var(--border)] px-4">
          <button type="button" className="btn btn--sm" onClick={onCancel} disabled={busy}>{t("importV2.byok.cancel")}</button>
          <button type="button" className="btn btn--sm btn--primary" onClick={() => void confirm()} disabled={!canConfirm}><Check size={13} className="mr-1 inline" aria-hidden="true" />{t("importV2.byok.approve")}</button>
        </footer>
      </section>
    </div>
  );
}
