import { useTranslation } from "react-i18next";
import {
  CheckCircle,
  File as FileIcon,
  FileSpreadsheet,
  FileText,
  Image as ImageIcon,
  Link,
  Presentation,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import type { ImportFileEntry, SourceFileType } from "../../types/import";
import { FILE_TYPE_LABELS } from "../../types/import";
import { useImportStore } from "../../stores/importStore";

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

const ARCHIVE_RULES: { type: string; target: string }[] = [
  { type: "PDF", target: "raw/sources/pdfs/" },
  { type: "DOCX", target: "raw/sources/docs/" },
  { type: "PPTX", target: "raw/sources/slides/" },
  { type: "XLSX/CSV", target: "raw/sources/sheets/" },
  { type: "MD/TXT", target: "raw/sources/markdown/" },
  { type: "图片", target: "raw/assets/" },
  { type: "URL", target: "raw/sources/links/" },
];

export function ImportRightPanel() {
  const { t } = useTranslation();
  const preview = useImportStore((state) => state.preview);
  const selectedSourcePath = useImportStore((state) => state.selectedSourcePath);

  const file: ImportFileEntry | null = selectedSourcePath
    ? preview?.files.find((entry) => entry.sourcePath === selectedSourcePath) ?? null
    : null;

  return (
    <aside
      aria-label={t("import.rightpanel.title")}
      className="flex w-[var(--rightpanel-w)] flex-col border-l border-[var(--border)] bg-[var(--surface)]"
    >
      <div className="flex h-[52px] items-center border-b border-[var(--border-subtle)] bg-[var(--background)] px-4">
        <span className="text-xs font-semibold uppercase tracking-[0.08em] text-[var(--text-secondary)]">
          {t("import.rightpanel.title")}
        </span>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto">
        {!file ? (
          <p className="px-4 py-3 text-[12px] text-[var(--text-muted)]">{t("import.rightpanel.noSelection")}</p>
        ) : (
          <SelectedFileSection file={file} />
        )}
        <ArchiveRulesSection />
        <ConflictsSection />
      </div>
    </aside>
  );
}

function SelectedFileSection({ file }: { file: ImportFileEntry }) {
  const { t } = useTranslation();
  const { Icon, color } = fileTypeIcon(file.fileType);
  const meta = file.metadata;
  const sizePage = [
    formatBytes(file.sizeBytes),
    file.pageCount != null ? t("import.table.pagesValue", { n: file.pageCount }) : null,
    file.wordCount != null ? t("import.table.wordsValue", { n: file.wordCount.toLocaleString() }) : null,
  ]
    .filter(Boolean)
    .join(" · ");

  return (
    <div className="border-b border-[var(--border-subtle)] px-4 py-3">
      <h4 className="mb-2 text-[11px] font-semibold uppercase tracking-[0.06em] text-[var(--text-muted)]">
        {t("import.rightpanel.selectedFile")}
      </h4>
      <div className="mb-3 flex items-center gap-2">
        <Icon size={16} style={{ color }} />
        <div className="min-w-0 flex-1">
          <div className="truncate text-[13px] font-semibold text-[var(--text-primary)]">{file.originalName}</div>
          <div className="font-mono text-[11px] text-[var(--text-muted)]">{sizePage || "—"}</div>
        </div>
      </div>
      <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-[12px]">
        <dt className="text-[var(--text-muted)]">{t("import.table.col.type")}</dt>
        <dd className="m-0 text-[var(--text-primary)]">{FILE_TYPE_LABELS[file.fileType]}</dd>
        {file.pageCount != null && (
          <>
            <dt className="text-[var(--text-muted)]">{t("import.metaPages")}</dt>
            <dd className="m-0 text-[var(--text-primary)]">{file.pageCount}</dd>
          </>
        )}
        {file.wordCount != null && (
          <>
            <dt className="text-[var(--text-muted)]">{t("import.metaWords")}</dt>
            <dd className="m-0 text-[var(--text-primary)]">{file.wordCount.toLocaleString()}</dd>
          </>
        )}
        {meta?.language && (
          <>
            <dt className="text-[var(--text-muted)]">{t("import.meta.language")}</dt>
            <dd className="m-0 text-[var(--text-primary)]">{meta.language}</dd>
          </>
        )}
        {meta?.created && (
          <>
            <dt className="text-[var(--text-muted)]">{t("import.meta.created")}</dt>
            <dd className="m-0 font-mono text-[11.5px] text-[var(--text-primary)]">{meta.created.slice(0, 10)}</dd>
          </>
        )}
        {meta?.modified && (
          <>
            <dt className="text-[var(--text-muted)]">{t("import.meta.modified")}</dt>
            <dd className="m-0 font-mono text-[11.5px] text-[var(--text-primary)]">{meta.modified.slice(0, 10)}</dd>
          </>
        )}
        {file.extractedAssets.length > 0 && (
          <>
            <dt className="text-[var(--text-muted)]">{t("import.meta.assets")}</dt>
            <dd className="m-0 text-[var(--text-primary)]">{file.extractedAssets.length}</dd>
          </>
        )}
      </dl>

      {file.extractionStatus === "unsupported" && (
        <p className="mt-3 rounded-[var(--radius-md)] border border-[var(--warning)] bg-[var(--warning-soft)] px-3 py-2 text-[11.5px]" style={{ color: "#a06a00" }}>
          {t("import.unsupported.note")}
        </p>
      )}

      {file.textPreview && (
        <div className="mt-3">
          <h4 className="mb-1 text-[11px] font-semibold uppercase tracking-[0.06em] text-[var(--text-muted)]">
            {t("import.rightpanel.textPreview")}
          </h4>
          <pre className="m-0 max-h-[240px] overflow-auto whitespace-pre-wrap rounded-[var(--radius-md)] border border-[var(--border-subtle)] bg-[var(--background)] p-3 font-mono text-[11.5px] leading-[1.65] text-[var(--text-secondary)]">
            {file.textPreview}
          </pre>
        </div>
      )}
    </div>
  );
}

function ArchiveRulesSection() {
  const { t } = useTranslation();
  return (
    <div className="border-b border-[var(--border-subtle)] px-4 py-3">
      <h4 className="mb-2 text-[11px] font-semibold uppercase tracking-[0.06em] text-[var(--text-muted)]">
        {t("import.rightpanel.archiveRules")}
      </h4>
      <div className="flex flex-col gap-1 font-mono text-[11.5px] text-[var(--text-secondary)]">
        {ARCHIVE_RULES.map((rule) => (
          <div key={rule.type}>
            {rule.type} → {rule.target}
          </div>
        ))}
      </div>
      <p className="mt-2 text-[11px] text-[var(--text-muted)]">{t("import.rightpanel.rule.original")}</p>
    </div>
  );
}

function ConflictsSection() {
  const { t } = useTranslation();
  const preview = useImportStore((state) => state.preview);
  const conflicts = preview?.conflicts ?? [];
  return (
    <div className="px-4 py-3">
      <h4 className="mb-2 text-[11px] font-semibold uppercase tracking-[0.06em] text-[var(--text-muted)]">
        {t("import.rightpanel.conflicts")}
      </h4>
      {conflicts.length === 0 ? (
        <div className="flex items-center gap-2 rounded-[var(--radius-md)] border border-[var(--accent-border)] bg-[var(--accent-soft)] px-3 py-2 text-[11.5px]" style={{ color: "var(--accent-hover)" }}>
          <CheckCircle size={14} />
          {t("import.rightpanel.conflicts.empty")}
        </div>
      ) : (
        <ul className="m-0 flex flex-col gap-2 p-0 text-[11.5px] text-[var(--text-secondary)]" style={{ listStyle: "none" }}>
          {conflicts.map((conflict, idx) => (
            <li key={`${conflict.newHash}-${conflict.resolvedPath}-${idx}`} className="rounded-[var(--radius-md)] border border-[var(--border-subtle)] bg-[var(--background)] px-3 py-2">
              <div className="font-medium text-[var(--text-primary)]">{conflict.originalName}</div>
              <div className="font-mono text-[10.5px] text-[var(--text-muted)]">→ {conflict.resolvedPath}</div>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
