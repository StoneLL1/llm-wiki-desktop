import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { ExternalLink, Plus, X } from "lucide-react";

interface ImportUrlDialogProps {
  open: boolean;
  onClose: () => void;
  onSubmit: (url: string) => void;
}

export function ImportUrlDialog({ open, onClose, onSubmit }: ImportUrlDialogProps) {
  const { t } = useTranslation();
  const [url, setUrl] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!open) return;
    setUrl("");
    const id = window.setTimeout(() => inputRef.current?.focus(), 30);
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        onClose();
      }
    };
    document.addEventListener("keydown", onKey);
    return () => {
      window.clearTimeout(id);
      document.removeEventListener("keydown", onKey);
    };
  }, [open, onClose]);

  if (!open) return null;

  const submit = () => {
    const trimmed = url.trim();
    if (!trimmed) return;
    onSubmit(trimmed);
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/20 px-4"
      role="dialog"
      aria-modal="true"
      aria-labelledby="import-url-dialog-title"
    >
      <section className="w-full max-w-[560px] rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--surface-raised)] shadow-lg">
        <header className="flex min-h-[52px] items-center justify-between border-b border-[var(--border)] px-4">
          <h2 id="import-url-dialog-title" className="m-0 text-[16px] font-semibold text-[var(--text-primary)]">
            {t("import.urlDialog.title")}
          </h2>
          <button type="button" className="btn btn--ghost btn--icon btn--sm" aria-label={t("import.actions.cancel")} onClick={onClose}>
            <X size={16} />
          </button>
        </header>
        <div className="space-y-2 px-4 py-4 text-[13px]">
          <label htmlFor="import-url-input" className="block text-[12.5px] font-medium text-[var(--text-primary)]">
            {t("import.urlDialog.label")}
          </label>
          <div className="input-group">
            <span className="input-group__lead"><ExternalLink size={14} /></span>
            <input
              id="import-url-input"
              ref={inputRef}
              type="url"
              className="input input--mono"
              placeholder={t("import.urlDialog.placeholder")}
              value={url}
              onChange={(event) => setUrl(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  event.preventDefault();
                  submit();
                }
              }}
            />
          </div>
          <p className="m-0 text-[11.5px] text-[var(--text-muted)]">{t("import.urlDialog.hint")}</p>
        </div>
        <footer className="flex min-h-[52px] items-center justify-end gap-2 border-t border-[var(--border)] px-4">
          <button type="button" className="btn btn--sm" onClick={onClose}>{t("import.actions.cancel")}</button>
          <button type="button" className="btn btn--sm btn--primary" disabled={!url.trim()} onClick={submit}>
            <Plus size={14} className="mr-1 inline-block align-[-2px]" />
            {t("import.urlDialog.fetch")}
          </button>
        </footer>
      </section>
    </div>
  );
}
