import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { LoaderCircle } from "lucide-react";

import { useModalDialog } from "../../hooks/useModalDialog";
import type { DeleteSourcePreview, MoveSourcePreview } from "../../types/source";
import { useSourceStore } from "./sourceStore";

interface SourceLifecycleDialogsProps {
  projectId: string;
  rootPath: string;
  onMoved: (path: string) => void;
  onDeleted: () => void;
}

export function SourceLifecycleDialogs({
  projectId,
  rootPath,
  onMoved,
  onDeleted,
}: SourceLifecycleDialogsProps) {
  const deletePreview = useSourceStore((state) => state.deletePreview);
  const movePreview = useSourceStore((state) => state.movePreview);
  const mutating = useSourceStore((state) => state.mutating);
  const confirmDelete = useSourceStore((state) => state.confirmDelete);
  const confirmMove = useSourceStore((state) => state.confirmMove);
  const clearDeletePreview = useSourceStore((state) => state.clearDeletePreview);
  const clearMovePreview = useSourceStore((state) => state.clearMovePreview);

  if (deletePreview) {
    return (
      <DeleteDialog
        preview={deletePreview}
        busy={mutating}
        onCancel={clearDeletePreview}
        onConfirm={() =>
          void confirmDelete(projectId, rootPath, "永久删除此来源").then((result) => {
            if (result) onDeleted();
          })
        }
      />
    );
  }
  if (movePreview) {
    return (
      <MoveDialog
        preview={movePreview}
        busy={mutating}
        onCancel={clearMovePreview}
        onConfirm={() =>
          void confirmMove(projectId, rootPath).then((result) => {
            if (result) onMoved(result.wikiPath);
          })
        }
      />
    );
  }
  return null;
}

function DeleteDialog({
  preview,
  busy,
  onCancel,
  onConfirm,
}: {
  preview: DeleteSourcePreview;
  busy: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const { t } = useTranslation();
  const cancelRef = useRef<HTMLButtonElement>(null);
  const dialogRef = useModalDialog({
    open: true,
    onClose: onCancel,
    initialFocusRef: cancelRef,
  });
  return (
    <div
      ref={dialogRef}
      tabIndex={-1}
      role="dialog"
      aria-modal="true"
      aria-labelledby="source-delete-title"
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/25 px-4"
    >
      <div className="flex max-h-[82vh] w-full max-w-[720px] flex-col rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--surface-raised)] shadow-xl">
        <header className="shrink-0 border-b border-[var(--border)] px-5 py-4">
          <h2 id="source-delete-title" className="m-0 text-[16px] font-semibold">
            {t("source.delete.title")}
          </h2>
          <p className="mb-0 mt-1 text-[12px] leading-5 text-[var(--text-muted)]">
            {t("source.delete.description", {
              count: preview.referenceCount,
              size: formatBytes(preview.expectedFreedBytes),
            })}
          </p>
        </header>
        <div className="app-pane-scrollbar min-h-0 flex-1 overflow-y-auto px-5 py-4">
          <div className="mb-4 grid grid-cols-3 gap-3">
            <Stat label={t("source.delete.versions")} value={String(preview.versions.length)} />
            <Stat label={t("source.delete.references")} value={String(preview.referenceCount)} />
            <Stat
              label={t("source.delete.space")}
              value={formatBytes(preview.expectedFreedBytes)}
            />
          </div>
          <h3 className="mb-2 text-[10.5px] font-semibold uppercase tracking-[0.08em] text-[var(--text-muted)]">
            {t("source.delete.versionList")}
          </h3>
          <ol className="mb-4 space-y-1 rounded-[var(--radius-md)] border border-[var(--border)] p-3">
            {preview.versions.map((version) => (
              <li
                key={version.versionId}
                className="grid grid-cols-[1fr_auto] gap-3 font-mono text-[10.5px] leading-4"
              >
                <span className="min-w-0 break-all">
                  {version.versionId}
                  {version.current ? ` · ${t("source.delete.currentVersion")}` : ""}
                </span>
                <time className="text-[var(--text-muted)]">
                  {new Date(version.createdAt).toLocaleString()}
                </time>
              </li>
            ))}
          </ol>
          <h3 className="mb-2 text-[10.5px] font-semibold uppercase tracking-[0.08em] text-[var(--text-muted)]">
            {t("source.delete.paths")}
          </h3>
          <ul className="space-y-1 rounded-[var(--radius-md)] bg-[var(--surface-muted)] p-3">
            {preview.paths.map((entry) => (
              <li key={entry.path} className="flex gap-3 font-mono text-[10.5px] leading-4">
                <span className="min-w-0 flex-1 break-all">{entry.path}</span>
                <span className="shrink-0 text-[var(--text-muted)]">{formatBytes(entry.sizeBytes)}</span>
              </li>
            ))}
          </ul>
          {preview.referencedBy.length ? (
            <>
              <h3 className="mb-2 mt-4 text-[10.5px] font-semibold uppercase tracking-[0.08em] text-[var(--text-muted)]">
                {t("source.delete.referencedBy")}
              </h3>
              <ul className="space-y-1 font-mono text-[10.5px]">
                {preview.referencedBy.map((path) => <li key={path}>{path}</li>)}
              </ul>
              <p className="mt-2 text-[11.5px] leading-5 text-[var(--text-muted)]">
                {t("source.delete.derivedPreserved")}
              </p>
            </>
          ) : null}
        </div>
        <footer className="flex shrink-0 items-center justify-end gap-2 border-t border-[var(--border)] px-5 py-3">
          <button
            ref={cancelRef}
            type="button"
            disabled={busy}
            onClick={onCancel}
            className="h-[30px] rounded-[var(--radius-md)] border border-[var(--border)] px-3 text-[12px]"
          >
            {t("confirmation.cancel")}
          </button>
          <button
            type="button"
            disabled={busy}
            onClick={onConfirm}
            className="inline-flex h-[30px] items-center gap-1.5 rounded-[var(--radius-md)] bg-[var(--danger)] px-3 text-[12px] font-semibold text-[var(--text-inverse)] disabled:opacity-40"
          >
            {busy ? <LoaderCircle size={13} className="animate-spin" /> : null}
            {t("source.delete.confirm")}
          </button>
        </footer>
      </div>
    </div>
  );
}

function MoveDialog({
  preview,
  busy,
  onCancel,
  onConfirm,
}: {
  preview: MoveSourcePreview;
  busy: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const { t } = useTranslation();
  const confirmRef = useRef<HTMLButtonElement>(null);
  const dialogRef = useModalDialog({
    open: true,
    onClose: onCancel,
    initialFocusRef: confirmRef,
  });
  return (
    <div
      ref={dialogRef}
      tabIndex={-1}
      role="dialog"
      aria-modal="true"
      aria-labelledby="source-move-title"
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/25 px-4"
    >
      <div className="w-full max-w-[560px] rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--surface-raised)] shadow-xl">
        <header className="border-b border-[var(--border)] px-5 py-4">
          <h2 id="source-move-title" className="m-0 text-[16px] font-semibold">
            {t("source.move.title")}
          </h2>
        </header>
        <div className="space-y-3 px-5 py-4 text-[12px]">
          <PathRow label={t("source.move.from")} value={preview.oldWikiPath} />
          <PathRow label={t("source.move.to")} value={preview.newWikiPath} />
          <p className="m-0 text-[var(--text-muted)]">
            {t("source.move.affected", { count: preview.affectedPaths.length })}
          </p>
        </div>
        <footer className="flex justify-end gap-2 border-t border-[var(--border)] px-5 py-3">
          <button type="button" disabled={busy} onClick={onCancel} className="h-[30px] rounded-[var(--radius-md)] border border-[var(--border)] px-3 text-[12px]">
            {t("confirmation.cancel")}
          </button>
          <button ref={confirmRef} type="button" disabled={busy} onClick={onConfirm} className="h-[30px] rounded-[var(--radius-md)] bg-[var(--foreground)] px-3 text-[12px] font-medium text-[var(--text-inverse)] disabled:opacity-40">
            {t("source.move.confirm")}
          </button>
        </footer>
      </div>
    </div>
  );
}

export function SourceMovePathDialog({
  currentPath,
  onCancel,
  onPreview,
}: {
  currentPath: string;
  onCancel: () => void;
  onPreview: (path: string) => void;
}) {
  const { t } = useTranslation();
  const [path, setPath] = useState(currentPath);
  const inputRef = useRef<HTMLInputElement>(null);
  const dialogRef = useModalDialog({
    open: true,
    onClose: onCancel,
    initialFocusRef: inputRef,
  });
  useEffect(() => setPath(currentPath), [currentPath]);
  return (
    <div ref={dialogRef} tabIndex={-1} role="dialog" aria-modal="true" aria-labelledby="source-move-path-title" className="fixed inset-0 z-50 flex items-center justify-center bg-black/25 px-4">
      <form
        className="w-full max-w-[520px] rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--surface-raised)] shadow-xl"
        onSubmit={(event) => {
          event.preventDefault();
          if (path.trim() !== currentPath) onPreview(path.trim());
        }}
      >
        <header className="border-b border-[var(--border)] px-5 py-4">
          <h2 id="source-move-path-title" className="m-0 text-[16px] font-semibold">{t("source.move.title")}</h2>
        </header>
        <label className="block px-5 py-4 text-[12px]">
          <span className="mb-1.5 block text-[var(--text-secondary)]">{t("source.move.path")}</span>
          <input ref={inputRef} value={path} onChange={(event) => setPath(event.target.value)} className="h-[32px] w-full rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--background)] px-2 font-mono text-[11.5px] outline-none focus:border-[var(--accent)]" />
        </label>
        <footer className="flex justify-end gap-2 border-t border-[var(--border)] px-5 py-3">
          <button type="button" onClick={onCancel} className="h-[30px] rounded-[var(--radius-md)] border border-[var(--border)] px-3 text-[12px]">{t("confirmation.cancel")}</button>
          <button type="submit" disabled={!path.trim() || path.trim() === currentPath} className="h-[30px] rounded-[var(--radius-md)] bg-[var(--foreground)] px-3 text-[12px] text-[var(--text-inverse)] disabled:opacity-40">{t("source.move.preview")}</button>
        </footer>
      </form>
    </div>
  );
}

function PathRow({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <div className="mb-1 text-[10.5px] uppercase tracking-[0.08em] text-[var(--text-muted)]">{label}</div>
      <div className="break-all rounded-[var(--radius-sm)] bg-[var(--surface-muted)] p-2 font-mono text-[11px]">{value}</div>
    </div>
  );
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-[var(--radius-md)] border border-[var(--border)] p-3">
      <div className="text-[10.5px] uppercase tracking-[0.08em] text-[var(--text-muted)]">{label}</div>
      <div className="mt-1 text-[16px] font-semibold">{value}</div>
    </div>
  );
}

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  return `${(bytes / 1024 / 1024 / 1024).toFixed(1)} GB`;
}
