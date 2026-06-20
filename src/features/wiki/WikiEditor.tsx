import { useTranslation } from "react-i18next";
import { LoaderCircle, Save } from "lucide-react";

import type { SaveState } from "./wikiStore";

interface WikiEditorProps {
  draft: string;
  saveState: SaveState;
  onDraftChange: (draft: string) => void;
  onSave: () => void;
  onCancel: () => void;
  onReload: () => void;
}

export function WikiEditor({
  draft,
  saveState,
  onDraftChange,
  onSave,
  onCancel,
  onReload,
}: WikiEditorProps) {
  const { t } = useTranslation();
  const saving = saveState === "saving";

  return (
    <div className="flex h-full flex-col">
      <div className="flex shrink-0 items-center gap-2 border-b border-[var(--border-subtle)] px-1 pb-2 pt-1">
        <span
          className={`inline-flex items-center gap-1.5 rounded-full px-2 py-0.5 text-[11px] font-medium ${
            saveState === "saved"
              ? "bg-[var(--accent-soft)] text-[var(--accent-hover)]"
              : saveState === "conflict" || saveState === "error"
                ? "bg-[var(--warning-soft)] text-[var(--warning)]"
                : "bg-[var(--surface-muted)] text-[var(--text-muted)]"
          }`}
        >
          <span
            className={`inline-block h-[6px] w-[6px] rounded-full ${
              saveState === "saved"
                ? "bg-[var(--accent)]"
                : saveState === "conflict" || saveState === "error"
                  ? "bg-[var(--warning)]"
                  : "bg-[var(--text-muted)]"
            }`}
          />
          {t(`wiki.editor.saveState.${saveState}`)}
        </span>
        <div className="ml-auto flex items-center gap-2">
          <button
            type="button"
            className="h-[26px] rounded-[var(--radius-sm)] border border-[var(--border)] bg-[var(--surface-raised)] px-3 text-[12px] font-medium text-[var(--text-primary)] hover:bg-[var(--surface-muted)]"
            onClick={onCancel}
            disabled={saving}
          >
            {t("wiki.editor.cancel")}
          </button>
          {saveState === "conflict" ? (
            <button
              type="button"
              className="h-[26px] rounded-[var(--radius-sm)] border border-[var(--border)] bg-[var(--surface-raised)] px-3 text-[12px] font-medium text-[var(--text-primary)] hover:bg-[var(--surface-muted)]"
              onClick={onReload}
            >
              {t("wiki.editor.reload")}
            </button>
          ) : null}
          <button
            type="button"
            className="inline-flex h-[26px] items-center gap-1.5 rounded-[var(--radius-sm)] bg-[var(--accent)] px-3 text-[12px] font-medium text-white hover:bg-[var(--accent-hover)] disabled:opacity-60"
            onClick={onSave}
            disabled={saving}
          >
            {saving ? (
              <LoaderCircle size={13} className="animate-spin" />
            ) : (
              <Save size={13} />
            )}
            {t("wiki.editor.save")}
          </button>
        </div>
      </div>
      {saveState === "conflict" ? (
        <div className="shrink-0 border-b border-[var(--warning)] bg-[var(--warning-soft)] px-3 py-2 text-[11.5px] text-[var(--warning)]">
          {t("wiki.editor.conflictCopy")}
        </div>
      ) : null}
      <textarea
        className="min-h-0 flex-1 resize-none border-none bg-transparent p-2 font-mono text-[13px] leading-[1.7] text-[var(--text-primary)] outline-none"
        spellCheck={false}
        value={draft}
        onChange={(event) => onDraftChange(event.target.value)}
      />
    </div>
  );
}
