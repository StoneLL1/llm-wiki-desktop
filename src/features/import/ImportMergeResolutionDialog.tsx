import { useEffect, useState } from "react";
import { GitMerge, LoaderCircle, X } from "lucide-react";
import { useTranslation } from "react-i18next";

import { useModalDialog } from "../../hooks/useModalDialog";
import type {
  ImportItemResolution,
  ImportResolutionBinding,
  ImportThreeWayMergeContext,
} from "../../types/importV2";

export interface ImportMergeResolutionDialogProps {
  open: boolean;
  itemId: string | null;
  title: string;
  loadContext: (itemId: string) => Promise<ImportThreeWayMergeContext>;
  onChoose: (itemId: string, resolution: ImportItemResolution) => Promise<void>;
  onSaveMerged: (itemId: string, mergedMarkdown: string) => Promise<void>;
  onClose: () => void;
}

function boundResolution(
  kind: "keep_current_source" | "apply_import_candidate",
  binding: ImportResolutionBinding,
): ImportItemResolution {
  return {
    kind,
    sourceId: binding.sourceId,
    candidateHash: binding.candidateHash,
    currentHash: binding.currentHash,
    targetVersionId: binding.targetVersionId,
  };
}

export function ImportMergeResolutionDialog({
  open,
  itemId,
  title,
  loadContext,
  onChoose,
  onSaveMerged,
  onClose,
}: ImportMergeResolutionDialogProps) {
  const { t } = useTranslation();
  const dialogRef = useModalDialog({ open, onClose });
  const [context, setContext] = useState<ImportThreeWayMergeContext | null>(null);
  const [mergedMarkdown, setMergedMarkdown] = useState("");
  const [state, setState] = useState<"idle" | "loading" | "saving" | "error">("idle");
  const [technicalError, setTechnicalError] = useState<string | null>(null);

  useEffect(() => {
    if (!open || !itemId) {
      setContext(null);
      setMergedMarkdown("");
      setTechnicalError(null);
      setState("idle");
      return;
    }
    let current = true;
    setContext(null);
    setMergedMarkdown("");
    setTechnicalError(null);
    setState("loading");
    void loadContext(itemId)
      .then((next) => {
        if (!current) return;
        setContext(next);
        setMergedMarkdown(next.candidateMarkdown);
        setState("idle");
      })
      .catch((error: unknown) => {
        if (!current) return;
        setTechnicalError(error instanceof Error ? error.message : String(error));
        setState("error");
      });
    return () => {
      current = false;
    };
  }, [itemId, loadContext, open]);

  async function choose(kind: "keep_current_source" | "apply_import_candidate") {
    const binding = context?.resolution.binding;
    if (!itemId || !binding || state === "saving") return;
    setState("saving");
    setTechnicalError(null);
    try {
      await onChoose(itemId, boundResolution(kind, binding));
      onClose();
    } catch (error) {
      setTechnicalError(error instanceof Error ? error.message : String(error));
      setState("error");
    }
  }

  async function saveMerged() {
    if (!itemId || !context || !mergedMarkdown.trim() || state === "saving") return;
    setState("saving");
    setTechnicalError(null);
    try {
      await onSaveMerged(itemId, mergedMarkdown);
      onClose();
    } catch (error) {
      setTechnicalError(error instanceof Error ? error.message : String(error));
      setState("error");
    }
  }

  if (!open || !itemId) return null;

  const busy = state === "loading" || state === "saving";

  return (
    <div
      ref={dialogRef}
      tabIndex={-1}
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/20 px-4"
      role="dialog"
      aria-modal="true"
      aria-labelledby="import-merge-title"
    >
      <section className="flex max-h-[88vh] w-full max-w-[1120px] flex-col rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--surface-raised)] shadow-lg">
        <header className="flex min-h-[52px] items-center gap-3 border-b border-[var(--border)] px-4">
          <GitMerge size={17} className="shrink-0 text-[var(--accent)]" aria-hidden="true" />
          <div className="min-w-0 flex-1">
            <h2 id="import-merge-title" className="truncate text-[15px] font-semibold" title={title}>
              {t("importV2.merge.title", { title })}
            </h2>
            <p className="m-0 text-[10.5px] text-[var(--text-muted)]">
              {t("importV2.merge.description")}
            </p>
          </div>
          <button type="button" className="icon-button" aria-label={t("importV2.merge.close")} title={t("importV2.merge.close")} onClick={onClose}>
            <X size={16} aria-hidden="true" />
          </button>
        </header>

        <div className="app-pane-scrollbar min-h-0 flex-1 overflow-y-auto p-4">
          {state === "loading" ? (
            <p role="status" className="flex items-center gap-2 text-[12px] text-[var(--text-muted)]">
              <LoaderCircle size={14} className="animate-spin" aria-hidden="true" />
              {t("importV2.merge.loading")}
            </p>
          ) : null}
          {state === "error" ? (
            <div role="alert" className="mb-3 rounded-[var(--radius-md)] border border-[var(--danger)] bg-[var(--danger-soft)] px-3 py-2 text-[11.5px] text-[var(--danger-text)]">
              <strong>{t("importV2.merge.error")}</strong>
              <p className="m-0 mt-1">{t("importV2.merge.errorSafety")}</p>
              {technicalError ? (
                <details className="mt-2 text-[10.5px]">
                  <summary>{t("importV2.preview.technicalDetails")}</summary>
                  <p className="break-all font-mono">{technicalError}</p>
                </details>
              ) : null}
            </div>
          ) : null}
          {context ? (
            <>
              <div className="import-v2-merge-grid">
                <section>
                  <h3>{t("importV2.merge.current")}</h3>
                  <pre>{context.currentMarkdown}</pre>
                </section>
                <section>
                  <h3>{t("importV2.merge.imported")}</h3>
                  <pre>{context.candidateMarkdown}</pre>
                </section>
                <section>
                  <h3>{t("importV2.merge.merged")}</h3>
                  <textarea
                    aria-label={t("importV2.merge.merged")}
                    value={mergedMarkdown}
                    onChange={(event) => setMergedMarkdown(event.target.value)}
                    disabled={busy}
                  />
                </section>
              </div>
              <p className="m-0 mt-3 text-[10.5px] text-[var(--text-muted)]">
                {t("importV2.merge.checkpoint")}
              </p>
            </>
          ) : null}
        </div>

        <footer className="flex min-h-[52px] flex-wrap items-center justify-end gap-2 border-t border-[var(--border)] px-4">
          <button type="button" className="btn btn--sm" onClick={onClose} disabled={state === "saving"}>
            {t("importV2.merge.cancel")}
          </button>
          <button type="button" className="btn btn--sm" onClick={() => void choose("keep_current_source")} disabled={!context || busy}>
            {t("importV2.merge.keepCurrent")}
          </button>
          <button type="button" className="btn btn--sm" onClick={() => void choose("apply_import_candidate")} disabled={!context || busy}>
            {t("importV2.merge.useImported")}
          </button>
          <button type="button" className="btn btn--sm btn--primary" onClick={() => void saveMerged()} disabled={!context || !mergedMarkdown.trim() || busy}>
            {state === "saving" ? <LoaderCircle size={13} className="animate-spin" aria-hidden="true" /> : null}
            {t("importV2.merge.saveMerged")}
          </button>
        </footer>
      </section>
    </div>
  );
}
