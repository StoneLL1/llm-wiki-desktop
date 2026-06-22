import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  Bot,
  Clipboard,
  Eye,
  File as FileIcon,
  FileSpreadsheet,
  FileText,
  FolderOpen,
  Globe,
  Image as ImageIcon,
  Link,
  Presentation,
  Trash,
  Upload,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import type {
  ExtractionStatus,
  ImportFileEntry,
  ImportedSource,
  SourceFileType,
} from "../../types/import";
import { FILE_TYPE_LABELS } from "../../types/import";
import { useImportStore } from "../../stores/importStore";
import { useToastStore } from "../../stores/toastStore";
import { ImportUrlDialog } from "./ImportUrlDialog";
import { OpenFolderAsProjectDialog } from "./OpenFolderAsProjectDialog";
import { subscribeToDragDrop } from "./dragDrop";
import { selectImportFiles } from "./nativeFilePicker";

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function fileTypeIcon(type: SourceFileType): { Icon: LucideIcon; color: string } {
  switch (type) {
    case "pdf":
    case "document":
      return { Icon: FileText, color: "var(--danger)" };
    case "presentation":
      return { Icon: Presentation, color: "var(--warning)" };
    case "spreadsheet":
    case "csv":
      return { Icon: FileSpreadsheet, color: "var(--accent)" };
    case "markdown":
    case "text":
      return { Icon: FileText, color: "var(--text-secondary)" };
    case "image":
      return { Icon: ImageIcon, color: "var(--text-secondary)" };
    case "html":
    case "url":
      return { Icon: Link, color: "var(--accent)" };
    default:
      return { Icon: FileIcon, color: "var(--text-muted)" };
  }
}

function StatusBadge({ status }: { status: ExtractionStatus }) {
  const { t } = useTranslation();
  const map: Record<ExtractionStatus, { cls: string; key: string }> = {
    extracted: { cls: "badge--accent", key: "import.status.extracted" },
    unsupported: { cls: "badge--warn", key: "import.status.unsupported" },
    failed: { cls: "badge--danger", key: "import.status.failed" },
    pending: { cls: "", key: "import.status.pending" },
  };
  const entry = map[status];
  return (
    <span className={`badge ${entry.cls}`}>
      <span className="dot" />
      {t(entry.key)}
    </span>
  );
}

function usePagesOrWords() {
  const { t } = useTranslation();
  return (file: ImportFileEntry) => {
    if (file.pageCount != null) return t("import.table.pagesValue", { n: file.pageCount });
    if (file.wordCount != null) return t("import.table.wordsValue", { n: file.wordCount.toLocaleString() });
    return "—";
  };
}

interface ImportViewProps {
  isConfirming: boolean;
  onRequestPreview: (paths: string[]) => void;
  onRequestClipboard: (content: string) => void;
  onRequestUrl: (url: string) => void;
  importedSources: ImportedSource[];
  onDeleteSource: (path: string) => void;
  onReplaceSource: (path: string, replacementPath: string) => void;
  onConfirm: (opts: { createCheckpoint: boolean; compileAfterImport: boolean }) => void;
}

export function ImportView({
  isConfirming,
  onRequestPreview,
  onRequestClipboard,
  onRequestUrl,
  importedSources,
  onDeleteSource,
  onReplaceSource,
  onConfirm,
}: ImportViewProps) {
  const { t } = useTranslation();
  const formatPagesOrWords = usePagesOrWords();
  const preview = useImportStore((state) => state.preview);
  const selectedSourcePath = useImportStore((state) => state.selectedSourcePath);
  const setSelectedSourcePath = useImportStore((state) => state.setSelectedSourcePath);
  const urlDialogOpen = useImportStore((state) => state.urlDialogOpen);
  const setUrlDialogOpen = useImportStore((state) => state.setUrlDialogOpen);
  const folderDialogOpen = useImportStore((state) => state.folderDialogOpen);
  const setFolderDialogOpen = useImportStore((state) => state.setFolderDialogOpen);
  const createCheckpoint = useImportStore((state) => state.createCheckpoint);
  const setCreateCheckpoint = useImportStore((state) => state.setCreateCheckpoint);
  const compileAfterImport = useImportStore((state) => state.compileAfterImport);
  const setCompileAfterImport = useImportStore((state) => state.setCompileAfterImport);
  const pushToast = useToastStore((state) => state.pushToast);

  const [paths, setPaths] = useState("");
  const [clipboardContent, setClipboardContent] = useState("");
  const [clipboardOpen, setClipboardOpen] = useState(false);
  const [dropActive, setDropActive] = useState(false);
  const [checkedPaths, setCheckedPaths] = useState<Set<string>>(new Set());
  const [replacementPaths, setReplacementPaths] = useState<Record<string, string>>({});

  const files = useMemo(() => preview?.files ?? [], [preview]);
  const hasPreview = preview !== null && files.length > 0;

  useEffect(() => {
    setCheckedPaths(hasPreview ? new Set(files.map((file) => file.sourcePath)) : new Set());
  }, [files, hasPreview]);

  // Tauri native OS file drag-drop (paths available without the dialog plugin).
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void (async () => {
      if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) return;
      try {
        const { getCurrentWebview } = await import("@tauri-apps/api/webview");
        unlisten = await subscribeToDragDrop({
          listen: (handler) => getCurrentWebview().onDragDropEvent(handler),
          isCancelled: () => cancelled,
          onActive: setDropActive,
          onPaths: onRequestPreview,
        });
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        pushToast("error", t("import.dragDropUnavailable", { message }));
      }
    })();
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [onRequestPreview, pushToast, t]);

  const chooseFiles = async () => {
    try {
      const selectedPaths = await selectImportFiles();
      if (selectedPaths.length > 0) onRequestPreview(selectedPaths);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      pushToast("error", t("import.filePickerError", { message }));
    }
  };

  const totals = useMemo(() => {
    let ok = 0;
    let partial = 0;
    let failed = 0;
    let bytes = 0;
    for (const file of files) {
      bytes += file.sizeBytes;
      if (file.extractionStatus === "extracted") ok += 1;
      else if (file.extractionStatus === "failed") failed += 1;
      else if (file.extractionStatus === "unsupported" || file.extractionStatus === "pending") partial += 1;
    }
    return { ok, partial, failed, bytes };
  }, [files]);

  const allChecked = hasPreview && checkedPaths.size === files.length && files.length > 0;
  const noneChecked = checkedPaths.size === 0;
  const canConfirm = hasPreview && !isConfirming && !noneChecked;

  const submitPaths = () => {
    const list = paths
      .split(/\r?\n/)
      .map((line) => line.trim())
      .filter((line) => line.length > 0);
    if (list.length === 0) return;
    onRequestPreview(list);
  };

  const toggleCheck = (path: string) => {
    setCheckedPaths((current) => {
      const next = new Set(current);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  };

  const toggleAll = () => {
    if (allChecked) setCheckedPaths(new Set());
    else setCheckedPaths(new Set(files.map((file) => file.sourcePath)));
  };

  const clearPreview = () => {
    setCheckedPaths(new Set());
    setSelectedSourcePath(null);
    useImportStore.getState().setPreview(null);
  };

  return (
    <div className="import-layout">
      {/* Source selection */}
      <div className="import-section">
        <h2 className="import-section__label">{t("import.section.sources")}</h2>
        <div className="import-grid">
          <button
            type="button"
            className={`import-source ${dropActive ? "import-source--drop" : ""}`}
            onClick={() => void chooseFiles()}
            aria-label={t("import.source.files")}
          >
            <span className="import-source__icon"><Upload size={18} /></span>
            <span className="import-source__title">{t("import.source.files")}</span>
            <span className="import-source__desc">{t("import.source.files.desc")}</span>
          </button>
          <button type="button" className="import-source" onClick={() => setFolderDialogOpen(true)} aria-label={t("import.source.folder")}>
            <span className="import-source__icon"><FolderOpen size={18} /></span>
            <span className="import-source__title">{t("import.source.folder")}</span>
            <span className="import-source__desc">{t("import.source.folder.desc")}</span>
          </button>
          <button type="button" className="import-source" onClick={() => setUrlDialogOpen(true)} aria-label={t("import.source.url")}>
            <span className="import-source__icon"><Link size={18} /></span>
            <span className="import-source__title">{t("import.source.url")}</span>
            <span className="import-source__desc">{t("import.source.url.desc")}</span>
          </button>
          <button type="button" className="import-source" onClick={() => setClipboardOpen((value) => !value)} aria-label={t("import.source.clipboard")}>
            <span className="import-source__icon"><Clipboard size={18} /></span>
            <span className="import-source__title">{t("import.source.clipboard")}</span>
            <span className="import-source__desc">{t("import.source.clipboard.desc")}</span>
          </button>
          <button type="button" className="import-source import-source--disabled" disabled aria-disabled="true" aria-label={t("import.source.browser")}>
            <span className="import-source__icon"><Globe size={18} /></span>
            <span className="import-source__title">{t("import.source.browser")}</span>
            <span className="import-source__desc">{t("import.source.browser.desc")}</span>
          </button>
          <button type="button" className="import-source import-source--disabled" disabled aria-disabled="true" aria-label={t("import.source.advanced")}>
            <span className="import-source__icon"><Bot size={18} /></span>
            <span className="import-source__title">{t("import.source.advanced")}</span>
            <span className="import-source__desc">{t("import.source.advanced.desc")}</span>
          </button>
        </div>

        {/* Paths entry */}
        <div className="import-paths">
          <div className="import-paths__row">
            <div className="input-group" style={{ alignItems: "stretch" }}>
              <textarea
                className="input input--mono"
                style={{ minHeight: 56, resize: "vertical", padding: "8px 12px" }}
                value={paths}
                onChange={(event) => setPaths(event.target.value)}
                placeholder={t("import.paths.placeholder")}
                aria-label={t("import.paths.label")}
              />
            </div>
            <button
              type="button"
              className="btn btn--sm btn--primary"
              onClick={submitPaths}
              disabled={!paths.trim()}
              style={{ alignSelf: "center" }}
            >
              <Upload size={14} className="mr-1 inline-block align-[-2px]" />
              {t("import.paths.add")}
            </button>
          </div>
          <p className="import-paths__hint m-0">{t("import.paths.hint")}</p>
        </div>

        {/* Clipboard */}
        {clipboardOpen && (
          <div className="import-paths">
            <div className="import-paths__row">
              <textarea
                className="input input--mono"
                style={{ minHeight: 72, resize: "vertical", padding: "8px 12px" }}
                value={clipboardContent}
                onChange={(event) => setClipboardContent(event.target.value)}
                placeholder={t("import.clipboardPlaceholder")}
                aria-label={t("import.clipboardMarkdown")}
              />
              <button
                type="button"
                className="btn btn--sm"
                onClick={() => {
                  const trimmed = clipboardContent.trim();
                  if (trimmed) onRequestClipboard(trimmed);
                }}
                disabled={!clipboardContent.trim()}
                style={{ alignSelf: "center" }}
              >
                {t("import.paths.add")}
              </button>
            </div>
          </div>
        )}

        {/* Manage existing sources */}
        {importedSources.length > 0 && (
          <details className="mt-3 rounded-[var(--radius-md)] border border-[var(--border)] px-3 py-2">
            <summary className="cursor-pointer text-[12px] font-medium text-[var(--text-secondary)]">
              {t("import.manageSources", { count: importedSources.length })}
            </summary>
            <div className="mt-2 max-h-40 overflow-auto">
              {importedSources.map((source) => (
                <div key={source.path} className="flex items-center gap-2 border-t border-[var(--border-subtle)] py-2 first:border-t-0">
                  <span className="min-w-0 flex-1 truncate font-mono text-[11px]" title={source.path}>{source.path}</span>
                  <input
                    aria-label={t("import.replacementPathFor", { path: source.path })}
                    className="settings-input w-56 font-mono text-[11px]"
                    value={replacementPaths[source.path] ?? ""}
                    onChange={(event) => setReplacementPaths((current) => ({ ...current, [source.path]: event.target.value }))}
                    placeholder={t("import.replacementPath")}
                  />
                  <button
                    type="button"
                    className="settings-button settings-button--secondary"
                    disabled={!replacementPaths[source.path]?.trim()}
                    onClick={() => onReplaceSource(source.path, replacementPaths[source.path]?.trim() ?? "")}
                  >
                    {t("import.replaceSource")}
                  </button>
                  <button type="button" className="settings-button text-[var(--danger)]" onClick={() => onDeleteSource(source.path)}>
                    {t("import.deleteSource")}
                  </button>
                </div>
              ))}
            </div>
          </details>
        )}
      </div>

      {/* File table */}
      <div className="file-table-wrap">
        <div className="file-table__toolbar">
          <span className="file-table__caption">{hasPreview ? t("import.table.caption", { count: files.length }) : t("import.subtitle.empty")}</span>
          {hasPreview && (
            <div className="file-table__badges">
              <span className="badge badge--accent"><span className="dot" />{t("import.table.ok", { count: totals.ok })}</span>
              <span className="badge badge--warn"><span className="dot" />{t("import.table.partial", { count: totals.partial })}</span>
              <span className="badge badge--danger"><span className="dot" />{t("import.table.failed", { count: totals.failed })}</span>
              <span className="file-table__total">{t("import.table.total", { size: formatBytes(totals.bytes) })}</span>
            </div>
          )}
        </div>
        <div className="table-wrap">
          {hasPreview ? (
            <table className="table">
              <thead>
                <tr>
                  <th className="col-check">
                    <input type="checkbox" checked={allChecked} onChange={toggleAll} aria-label={t("import.table.col.name")} />
                  </th>
                  <th className="col-icon" />
                  <th>{t("import.table.col.name")}</th>
                  <th>{t("import.table.col.type")}</th>
                  <th>{t("import.table.col.size")}</th>
                  <th>{t("import.table.col.pages")}</th>
                  <th>{t("import.table.col.target")}</th>
                  <th>{t("import.table.col.status")}</th>
                  <th className="col-action">{t("import.table.col.preview")}</th>
                </tr>
              </thead>
              <tbody>
                {files.map((file) => {
                  const { Icon, color } = fileTypeIcon(file.fileType);
                  const checked = checkedPaths.has(file.sourcePath);
                  const isSelected = selectedSourcePath === file.sourcePath;
                  const summaryText = fileSummary(file);
                  return (
                    <tr key={file.sourcePath} className={isSelected ? "is-selected" : ""}>
                      <td className="col-check">
                        <input type="checkbox" checked={checked} onChange={() => toggleCheck(file.sourcePath)} aria-label={file.originalName} />
                      </td>
                      <td className="col-icon"><Icon size={14} style={{ color }} /></td>
                      <td>
                        <button
                          type="button"
                          className="block w-full text-left"
                          onClick={() => setSelectedSourcePath(file.sourcePath)}
                        >
                          <span className="primary block truncate">
                            {file.originalName}
                            {file.renamedFrom && <span className="ml-1 text-[10.5px] text-[var(--text-muted)]">→ {t("import.renamedBadge")}</span>}
                          </span>
                          {summaryText && <span className="secondary truncate">{summaryText}</span>}
                        </button>
                      </td>
                      <td><span className="badge">{FILE_TYPE_LABELS[file.fileType]}</span></td>
                      <td className="col-num">{formatBytes(file.sizeBytes)}</td>
                      <td className="col-num">{formatPagesOrWords(file)}</td>
                      <td className="col-path">{archiveDir(file)}</td>
                      <td><StatusBadge status={file.extractionStatus} /></td>
                      <td className="col-action">
                        <button
                          type="button"
                          className="btn btn--ghost btn--icon btn--sm"
                          aria-label={t("import.table.col.preview")}
                          onClick={() => setSelectedSourcePath(file.sourcePath)}
                        >
                          <Eye size={14} />
                        </button>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          ) : (
            <div className="empty" style={{ height: "100%" }}>
              <div className="empty__icon"><Upload size={18} /></div>
              <div className="empty__title">{t("view.import.emptyState")}</div>
            </div>
          )}
        </div>
      </div>

      {/* Actions bar */}
      <div className="import-actions">
        <div className="import-actions__note">
          <strong>{t("import.actions.note")}</strong> · {t("import.actions.note.detail")}
        </div>
        <div className="import-actions__controls">
          <label className="checkbox">
            <input type="checkbox" checked={createCheckpoint} onChange={(event) => setCreateCheckpoint(event.target.checked)} />
            {t("import.actions.gitCheckpoint")}
          </label>
          <label className="checkbox">
            <input type="checkbox" checked={compileAfterImport} onChange={(event) => setCompileAfterImport(event.target.checked)} />
            {t("import.actions.compileAfter")}
          </label>
          <button type="button" className="btn btn--sm" onClick={clearPreview} disabled={!hasPreview}>
            <Trash size={14} className="mr-1 inline-block align-[-2px]" />
            {t("import.actions.cancel")}
          </button>
          <button
            type="button"
            className="btn btn--sm btn--primary"
            disabled={!canConfirm}
            onClick={() => onConfirm({ createCheckpoint, compileAfterImport })}
            title={!hasPreview ? t("import.compileDisabled") : undefined}
          >
            <Upload size={14} className="mr-1 inline-block align-[-2px]" />
            {isConfirming
              ? t("import.compileConfirming")
              : compileAfterImport
                ? t("import.actions.confirm")
                : t("import.actions.confirmNoCompile")}
          </button>
        </div>
      </div>

      <ImportUrlDialog
        open={urlDialogOpen}
        onClose={() => setUrlDialogOpen(false)}
        onSubmit={(url) => {
          setUrlDialogOpen(false);
          onRequestUrl(url);
        }}
      />
      <OpenFolderAsProjectDialog open={folderDialogOpen} onClose={() => setFolderDialogOpen(false)} />
    </div>
  );
}

function fileSummary(file: ImportFileEntry): string {
  if (file.textPreview) return file.textPreview.slice(0, 120);
  if (file.extractionStatus === "unsupported") return "";
  if (file.metadata?.title) return file.metadata.title;
  return "";
}

function archiveDir(file: ImportFileEntry): string {
  const archived = file.archivedPath;
  const idx = archived.lastIndexOf("/");
  const dir = idx >= 0 ? archived.slice(0, idx + 1) : "";
  if (dir) return dir;
  if (file.fileType === "url") return "raw/sources/links/";
  return "raw/sources/";
}
