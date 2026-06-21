import { useState } from "react";
import { useTranslation } from "react-i18next";
import { AlertTriangle, Check, File, FileWarning, LoaderCircle, Upload } from "lucide-react";
import {
  type ImportFileEntry,
  type ImportPreview,
  EXTRACTION_STATUS_LABELS,
  FILE_TYPE_LABELS,
} from "../../types/import";

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function StatusIcon({ status }: { status: ImportFileEntry["extractionStatus"] }) {
  switch (status) {
    case "extracted":
      return <Check size={14} className="text-[var(--accent)]" />;
    case "failed":
      return <AlertTriangle size={14} className="text-[var(--danger)]" />;
    case "unsupported":
      return <FileWarning size={14} className="text-[var(--warning)]" />;
    case "pending":
      return <LoaderCircle size={14} className="text-[var(--text-muted)] animate-spin" />;
    default:
      return <File size={14} className="text-[var(--text-muted)]" />;
  }
}

interface ImportViewProps {
  preview: ImportPreview | null;
  isConfirming: boolean;
  onRequestPreview: (paths: string[]) => void;
  onConfirm: () => void;
}

export function ImportView({
  preview,
  isConfirming,
  onRequestPreview,
  onConfirm,
}: ImportViewProps) {
  const { t } = useTranslation();
  const [sourcePath, setSourcePath] = useState("");
  const [selectedIndex, setSelectedIndex] = useState<number | null>(null);

  const hasPreview = preview !== null;
  const hasConflicts = (preview?.conflicts.length ?? 0) > 0;
  const canCompile = hasPreview && !isConfirming;

  const selectedFile = selectedIndex !== null ? preview?.files[selectedIndex] : null;

  return (
    <div className="flex h-full flex-col overflow-hidden">
      {/* Drop zone / toolbar */}
      <div className="shrink-0 border-b border-[var(--border)] px-4 py-3">
        <form
          className="flex items-center gap-3 rounded-lg border border-[var(--border)] px-4 py-3"
          onSubmit={(event) => {
            event.preventDefault();
            if (sourcePath.trim()) onRequestPreview([sourcePath.trim()]);
          }}
        >
          <Upload size={16} className="text-[var(--text-muted)] shrink-0" />
          <label className="flex min-w-0 flex-1 items-center gap-2 text-[12px] text-[var(--text-muted)]">
            <span className="shrink-0">{t("import.sourcePath")}</span>
            <input aria-label={t("import.sourcePath")} className="settings-input min-w-0 flex-1 font-mono" value={sourcePath} onChange={(event) => setSourcePath(event.target.value)} placeholder={t("import.sourcePathPlaceholder")} />
          </label>
          <button type="submit" disabled={!sourcePath.trim()} className="rounded-md bg-[var(--foreground)] px-3 py-1.5 text-[12px] font-medium text-[var(--text-inverse)] hover:bg-[var(--text-primary)] disabled:opacity-50">{t("view.import.actionPrimary")}</button>
        </form>

        {/* Summary bar */}
        {hasPreview && (
          <div className="mt-3 flex items-center gap-4 text-[12px]">
            <span className="font-medium text-[var(--text-primary)]">
              {t("import.summary.files", { count: preview!.summary.totalFiles })}
            </span>
            <span className="text-[var(--text-muted)]">
              {t("import.summary.ready", { count: preview!.summary.archivedFiles })}
            </span>
            {preview!.summary.duplicateFiles > 0 && (
              <span className="text-[var(--warning)]">
                {t("import.summary.duplicates", { count: preview!.summary.duplicateFiles })}
              </span>
            )}
            {preview!.summary.renamedFiles > 0 && (
              <span className="text-[var(--info)]">
                {t("import.summary.renamed", { count: preview!.summary.renamedFiles })}
              </span>
            )}
            {preview!.summary.failedFiles > 0 && (
              <span className="text-[var(--danger)]">
                {t("import.summary.failed", { count: preview!.summary.failedFiles })}
              </span>
            )}
            <button
              type="button"
              disabled={!canCompile}
              onClick={onConfirm}
              className={`ml-auto rounded-md px-3 py-1.5 text-[12px] font-medium transition-colors ${
                canCompile
                  ? "bg-[var(--accent)] text-white hover:bg-[var(--accent-hover)]"
                  : "bg-[var(--surface-muted)] text-[var(--text-disabled)] cursor-not-allowed"
              }`}
              title={
                !hasPreview
                  ? t("import.compileDisabled")
                  : isConfirming
                    ? t("import.compileConfirming")
                    : t("import.compile")
              }
            >
              {isConfirming
                ? t("import.compileConfirming")
                : canCompile
                  ? t("import.compile")
                  : t("import.compileDisabled")}
            </button>
          </div>
        )}
      </div>

      {/* File list and preview */}
      <div className="flex flex-1 overflow-hidden">
        {/* File list */}
        <div className="w-1/2 overflow-auto border-r border-[var(--border)]">
          {!hasPreview ? (
            <div className="flex h-full items-center justify-center text-[13px] text-[var(--text-muted)]">
              {t("view.import.emptyState")}
            </div>
          ) : (
            <div className="flex flex-col">
              {preview!.files.map((file, i) => (
                <button
                  key={`${file.archivedPath}-${i}`}
                  type="button"
                  onClick={() => setSelectedIndex(i)}
                  className={`flex items-center gap-2 border-b border-[var(--border-subtle)] px-4 py-2 text-left text-[13px] transition-colors hover:bg-[var(--surface-muted)] ${
                    selectedIndex === i
                      ? "bg-[var(--accent-soft)] border-l-[3px] border-l-[var(--accent)]"
                      : "border-l-[3px] border-l-transparent"
                  }`}
                >
                  <StatusIcon status={file.extractionStatus} />
                  <div className="min-w-0 flex-1">
                    <div className="truncate font-medium text-[var(--text-primary)]">
                      {file.originalName}
                      {file.renamedFrom && (
                        <span className="ml-1 text-[11px] text-[var(--text-muted)]">
                          → {t("import.renamedBadge")}
                        </span>
                      )}
                    </div>
                    <div className="flex gap-2 text-[11px] text-[var(--text-muted)]">
                      <span>{FILE_TYPE_LABELS[file.fileType]}</span>
                      <span>{formatBytes(file.sizeBytes)}</span>
                      {file.wordCount && <span>{file.wordCount} words</span>}
                      {file.pageCount && <span>{file.pageCount} pages</span>}
                    </div>
                  </div>
                  <span
                    className={`shrink-0 rounded-full px-1.5 py-0.5 text-[10px] font-medium ${
                      file.extractionStatus === "extracted"
                        ? "bg-[var(--accent-soft)] text-[var(--accent)]"
                        : file.extractionStatus === "failed"
                          ? "bg-[var(--danger-soft)] text-[var(--danger)]"
                          : file.extractionStatus === "unsupported"
                            ? "bg-[var(--warning-soft)] text-[var(--warning)]"
                            : "bg-[var(--surface-muted)] text-[var(--text-muted)]"
                    }`}
                  >
                    {EXTRACTION_STATUS_LABELS[file.extractionStatus]}
                  </span>
                </button>
              ))}
            </div>
          )}
        </div>

        {/* Right panel: details */}
        <div className="w-1/2 overflow-auto p-4">
          {!selectedFile ? (
            <div className="flex h-full items-center justify-center text-[13px] text-[var(--text-muted)]">
              {hasPreview
                ? t("import.selectToPreview")
                : t("import.importToBegin")}
            </div>
          ) : (
            <div className="flex flex-col gap-3 text-[13px]">
              <div>
                <h3 className="text-[14px] font-semibold text-[var(--text-primary)] truncate">
                  {selectedFile.originalName}
                </h3>
                <p className="mt-0.5 text-[11px] font-mono text-[var(--text-muted)]">
                  {selectedFile.archivedPath}
                </p>
              </div>

              {/* Status + error */}
              <div className="flex items-center gap-2">
                <StatusIcon status={selectedFile.extractionStatus} />
                <span className="text-[12px] font-medium">
                  {EXTRACTION_STATUS_LABELS[selectedFile.extractionStatus]}
                </span>
                {selectedFile.extractionError && (
                  <span className="text-[12px] text-[var(--danger)]">
                    — {selectedFile.extractionError}
                  </span>
                )}
              </div>

              {/* Conflict */}
              {selectedFile.conflict && (
                <div className="rounded-md border border-[var(--warning)] bg-[var(--warning-soft)] p-3 text-[12px]">
                  <div className="font-medium text-[var(--warning)]">
                    {t("import.conflictLabel")}: {selectedFile.conflict.conflictType.replace("_", " ")}
                  </div>
                  <div className="mt-1 text-[var(--text-secondary)]">
                    {t("import.resolvedPath")}:{" "}
                    <span className="font-mono">{selectedFile.conflict.resolvedPath}</span>
                  </div>
                  <div className="mt-1 text-[var(--text-muted)]">
                    {t("import.resolution")}: {selectedFile.conflict.resolution?.replace("_", " ") ?? "pending"}
                  </div>
                  {selectedFile.renamedFrom && (
                    <div className="mt-1 text-[var(--text-muted)]">
                      {t("import.renamedFrom")}:{" "}
                      <span className="font-mono">{selectedFile.renamedFrom}</span>
                    </div>
                  )}
                </div>
              )}

              {/* Text preview */}
              {selectedFile.textPreview && (
                <div>
                  <h4 className="text-[11px] font-medium uppercase tracking-[0.08em] text-[var(--text-muted)] mb-1">
                    {t("import.previewSection")}
                  </h4>
                  <pre className="max-h-48 overflow-auto whitespace-pre-wrap rounded-md border border-[var(--border)] bg-[var(--surface)] p-2 font-mono text-[11px] leading-relaxed text-[var(--text-secondary)]">
                    {selectedFile.textPreview}
                  </pre>
                </div>
              )}

              {/* Metadata */}
              {selectedFile.metadata && (
                <div>
                  <h4 className="text-[11px] font-medium uppercase tracking-[0.08em] text-[var(--text-muted)] mb-1">
                    {t("import.metadataSection")}
                  </h4>
                  <dl className="grid grid-cols-2 gap-x-3 gap-y-1 text-[12px]">
                    {selectedFile.metadata.title && (
                      <>
                        <dt className="text-[var(--text-muted)]">{t("import.metaTitle")}</dt>
                        <dd className="text-[var(--text-primary)]">{selectedFile.metadata.title}</dd>
                      </>
                    )}
                    {selectedFile.metadata.author && (
                      <>
                        <dt className="text-[var(--text-muted)]">{t("import.metaAuthor")}</dt>
                        <dd className="text-[var(--text-primary)]">{selectedFile.metadata.author}</dd>
                      </>
                    )}
                    {selectedFile.wordCount != null && (
                      <>
                        <dt className="text-[var(--text-muted)]">{t("import.metaWords")}</dt>
                        <dd className="text-[var(--text-primary)]">{selectedFile.wordCount}</dd>
                      </>
                    )}
                    {selectedFile.pageCount != null && (
                      <>
                        <dt className="text-[var(--text-muted)]">{t("import.metaPages")}</dt>
                        <dd className="text-[var(--text-primary)]">{selectedFile.pageCount}</dd>
                      </>
                    )}
                  </dl>
                </div>
              )}

              {/* Hash */}
              <div>
                <h4 className="text-[11px] font-medium uppercase tracking-[0.08em] text-[var(--text-muted)] mb-1">
                  {t("import.hashSection")}
                </h4>
                <code className="text-[11px] font-mono text-[var(--text-muted)] break-all">
                  {selectedFile.hash}
                </code>
              </div>
            </div>
          )}
        </div>
      </div>

      {/* Conflicts summary footer */}
      {hasConflicts && (
        <div className="shrink-0 border-t border-[var(--warning)] bg-[var(--warning-soft)] px-4 py-1.5 text-[11px] text-[var(--warning)]">
          {preview!.conflicts.length} conflict(s) found. Duplicates are skipped; conflicting names
          are renamed deterministically. Review details in the file list.
        </div>
      )}
    </div>
  );
}
