import { FileText, LoaderCircle, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import { useModalDialog } from "../../hooks/useModalDialog";

export interface ImportSubtitleDialogProps {
  open: boolean;
  candidates: readonly string[];
  onConfirm: (fileName: string) => Promise<void> | void;
  onCancel: () => void;
}

export function ImportSubtitleDialog({
  open,
  candidates,
  onConfirm,
  onCancel,
}: ImportSubtitleDialogProps) {
  const { t } = useTranslation();
  const initialFocusRef = useRef<HTMLInputElement>(null);
  const dialogRef = useModalDialog({ open, onClose: onCancel, initialFocusRef });
  const [selected, setSelected] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!open) return;
    setSelected(candidates[0] ?? "");
    setBusy(false);
  }, [candidates, open]);

  if (!open) return null;

  async function confirm() {
    if (busy || !selected) return;
    setBusy(true);
    try {
      await onConfirm(selected);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div
      ref={dialogRef}
      tabIndex={-1}
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/20 px-4"
      role="dialog"
      aria-modal="true"
      aria-labelledby="import-subtitle-title"
    >
      <section className="flex max-h-[76vh] w-full max-w-[560px] flex-col rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--surface-raised)] shadow-lg">
        <header className="flex min-h-[52px] items-center gap-3 border-b border-[var(--border)] px-4">
          <FileText size={17} className="text-[var(--accent)]" aria-hidden="true" />
          <h2 id="import-subtitle-title" className="m-0 flex-1 text-[15px] font-semibold">
            {t("importV2.subtitle.title")}
          </h2>
          <button type="button" className="icon-button" aria-label={t("importV2.subtitle.cancel")} title={t("importV2.subtitle.cancel")} onClick={onCancel} disabled={busy}>
            <X size={16} aria-hidden="true" />
          </button>
        </header>

        <div className="min-h-0 flex-1 overflow-y-auto px-4 py-4 text-[12px]">
          <p className="m-0 text-[var(--text-secondary)]">{t("importV2.subtitle.description")}</p>
          <fieldset className="mt-4 border-0 p-0">
            <legend className="mb-2 text-[12px] font-semibold">{t("importV2.subtitle.files")}</legend>
            <div className="grid gap-2">
              {candidates.map((fileName, index) => (
                <label
                  key={fileName}
                  className="flex cursor-pointer items-center gap-2 rounded-[var(--radius-md)] border border-[var(--border-subtle)] px-3 py-2 has-[:checked]:border-[var(--accent)]"
                >
                  <input
                    ref={index === 0 ? initialFocusRef : undefined}
                    type="radio"
                    name="import-subtitle"
                    value={fileName}
                    checked={selected === fileName}
                    onChange={() => setSelected(fileName)}
                    disabled={busy}
                  />
                  <span className="min-w-0 break-all font-mono text-[11px]">{fileName}</span>
                </label>
              ))}
            </div>
          </fieldset>
        </div>

        <footer className="flex items-center justify-end gap-2 border-t border-[var(--border)] px-4 py-3">
          <button type="button" className="btn btn--sm" onClick={onCancel} disabled={busy}>
            {t("importV2.subtitle.cancel")}
          </button>
          <button type="button" className="btn btn--sm btn--primary" onClick={() => void confirm()} disabled={busy || !selected}>
            {busy ? <LoaderCircle size={13} className="mr-1 inline animate-spin" aria-label={t("importV2.common.loading")} /> : null}
            {t("importV2.subtitle.continue")}
          </button>
        </footer>
      </section>
    </div>
  );
}
