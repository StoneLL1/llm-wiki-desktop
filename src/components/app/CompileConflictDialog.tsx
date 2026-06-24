import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { PendingAction } from "../../types/backend";
import type { BackendTask } from "../../types/task";
import { useModalDialog } from "../../hooks/useModalDialog";
import { Button } from "../ui/button";

interface CompileConflictDetail {
  path: string;
  currentContent: string | null;
  generatedContent: string | null;
}

interface CompileConflictDialogProps {
  action: PendingAction;
  onCancel: () => void;
  onResolved: (task: BackendTask) => void;
}

export function CompileConflictDialog({
  action,
  onCancel,
  onResolved,
}: CompileConflictDialogProps) {
  const { t } = useTranslation();
  const [details, setDetails] = useState<CompileConflictDetail[]>([]);
  const [manual, setManual] = useState(false);
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const dialogRef = useModalDialog({ open: true, onClose: onCancel });

  useEffect(() => {
    let active = true;
    void invoke<CompileConflictDetail[]>("get_compile_conflict_details", {
      request: { actionId: action.id },
    })
      .then((items) => {
        if (!active) return;
        setDetails(items);
        setDrafts(
          Object.fromEntries(
            items.map((item) => [
              item.path,
              item.generatedContent ?? item.currentContent ?? "",
            ]),
          ),
        );
      })
      .catch((reason) => {
        if (active) setError(errorMessage(reason));
      });
    return () => {
      active = false;
    };
  }, [action.id]);

  const resolve = async (
    resolution: "keep_current" | "use_generated" | "manual_merge",
  ) => {
    setBusy(true);
    setError(null);
    try {
      const task = await invoke<BackendTask>("resolve_compile_conflict", {
        request: {
          actionId: action.id,
          resolution,
          manualFiles:
            resolution === "manual_merge"
              ? details.map((item) => ({ path: item.path, content: drafts[item.path] ?? "" }))
              : [],
        },
      });
      onResolved(task);
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div ref={dialogRef} tabIndex={-1} className="fixed inset-0 z-50 flex items-center justify-center bg-black/20 px-4" role="dialog" aria-modal="true" aria-labelledby="compile-conflict-title">
      <section className="flex max-h-[86vh] w-full max-w-[900px] flex-col rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--surface-raised)] shadow-lg">
        <header className="border-b border-[var(--border)] px-4 py-3">
          <h2 id="compile-conflict-title" className="text-[16px] font-semibold">{action.title}</h2>
          <p className="mt-1 text-[12px] text-[var(--text-muted)]">{action.message}</p>
        </header>
        <div className="min-h-0 flex-1 space-y-4 overflow-auto p-4">
          {error ? <p role="alert" className="text-[12px] text-[var(--danger)]">{error}</p> : null}
          {details.map((item) => (
            <section key={item.path} className="space-y-2">
              <h3 className="font-mono text-[12px] font-medium">{item.path}</h3>
              <div className="compile-diff-grid">
                <div><p className="mb-1 text-[11px] uppercase tracking-[0.08em] text-[var(--text-muted)]">{t("compileConflict.current")}</p><pre className="max-h-44 overflow-auto rounded border border-[var(--border)] bg-[var(--surface)] p-2 text-[11px]">{item.currentContent ?? t("compileConflict.missing")}</pre></div>
                <div><p className="mb-1 text-[11px] uppercase tracking-[0.08em] text-[var(--text-muted)]">{t("compileConflict.generated")}</p><pre className="max-h-44 overflow-auto rounded border border-[var(--border)] bg-[var(--surface)] p-2 text-[11px]">{item.generatedContent ?? t("compileConflict.deletion")}</pre></div>
              </div>
              {manual ? <textarea aria-label={item.path} value={drafts[item.path] ?? ""} onChange={(event) => setDrafts((current) => ({ ...current, [item.path]: event.target.value }))} className="min-h-36 w-full rounded border border-[var(--border)] bg-[var(--background)] p-2 font-mono text-[12px]" /> : null}
            </section>
          ))}
        </div>
        <footer className="flex min-h-[52px] items-center justify-end gap-2 border-t border-[var(--border)] px-4">
          <Button type="button" variant="secondary" onClick={onCancel} disabled={busy}>{t("confirmation.cancel")}</Button>
          <Button type="button" variant="secondary" onClick={() => { void resolve("keep_current"); }} disabled={busy || details.length === 0}>{t("compileConflict.keepCurrent")}</Button>
          <Button type="button" variant="secondary" onClick={() => setManual(true)} disabled={busy || details.length === 0}>{t("compileConflict.manualMerge")}</Button>
          {manual ? <Button type="button" onClick={() => { void resolve("manual_merge"); }} disabled={busy}>{t("compileConflict.applyManual")}</Button> : null}
          <Button type="button" onClick={() => { void resolve("use_generated"); }} disabled={busy || details.length === 0}>{t("compileConflict.useGenerated")}</Button>
        </footer>
      </section>
    </div>
  );
}

function errorMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "object" && error !== null && "message" in error) {
    const message = (error as { message: unknown }).message;
    if (typeof message === "string") return message;
  }
  return String(error);
}
