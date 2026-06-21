export type SourceFileType =
  | "pdf"
  | "document"
  | "presentation"
  | "spreadsheet"
  | "markdown"
  | "text"
  | "image"
  | "html"
  | "csv"
  | "url"
  | "unknown";

export type ExtractionStatus =
  | "pending"
  | "extracted"
  | "unsupported"
  | "failed";

export type ConflictType =
  | "exact_duplicate"
  | "name_collision"
  | "path_conflict"
  | "unknown";

export type ConflictResolution =
  | "skip"
  | "link_to_existing"
  | "rename"
  | "overwrite";

export interface SourceMetadata {
  title: string | null;
  author: string | null;
  created: string | null;
  modified: string | null;
  pageCount: number | null;
  wordCount: number | null;
  language: string | null;
}

export interface ImportConflict {
  originalName: string;
  conflictType: ConflictType;
  existingPath: string | null;
  resolvedPath: string;
  existingHash: string | null;
  newHash: string;
  resolution: ConflictResolution | null;
}

export interface ImportFileEntry {
  originalName: string;
  sourcePath: string;
  archivedPath: string;
  fileType: SourceFileType;
  sizeBytes: number;
  hash: string;
  extractionStatus: ExtractionStatus;
  extractionError: string | null;
  textPreview: string | null;
  pageCount: number | null;
  wordCount: number | null;
  metadata: SourceMetadata | null;
  extractedTextPath: string | null;
  extractedAssets: string[];
  conflict: ImportConflict | null;
  renamedFrom: string | null;
}

export interface ImportSummary {
  totalFiles: number;
  archivedFiles: number;
  duplicateFiles: number;
  renamedFiles: number;
  failedFiles: number;
  conflictsCount: number;
}

export interface ImportPreview {
  files: ImportFileEntry[];
  conflicts: ImportConflict[];
  summary: ImportSummary;
}

export interface ExtractResult {
  originalName: string;
  fileType: SourceFileType;
  status: ExtractionStatus;
  error: string | null;
  textPreview: string | null;
  metadata: SourceMetadata | null;
  extractedTextPath: string | null;
  extractedAssets: string[];
}

export interface ConfirmedImport {
  preview: ImportPreview;
  confirmedAt: string;
}

export interface ImportPreviewRequest {
  projectId: string;
  projectRootPath: string;
  sourcePaths: string[];
  allowDuplicates: boolean;
  linkDuplicates: boolean;
}

export interface ConfirmImportRequest {
  projectId: string;
  projectRootPath: string;
  preview: ImportPreview;
  /** When true, the backend creates a scoped Git checkpoint of the archived
   * files plus source/conflict indices. Backward-compatible default: false. */
  createCheckpoint?: boolean;
}

export interface ExtractTextRequest {
  projectId: string;
  projectRootPath: string;
  sourcePath: string;
}

export interface ValidateUrlRequest {
  url: string;
}

export interface PreviewTextImportRequest {
  projectId: string;
  projectRootPath: string;
  kind: "clipboard" | "url";
  sourceName: string;
  content: string;
  title: string | null;
  author: string | null;
}

export interface FetchedImportUrl {
  url: string;
  html: string;
}

export interface ImportedSource {
  path: string;
  sizeBytes: number;
  fileType: SourceFileType;
}

export const FILE_TYPE_LABELS: Record<SourceFileType, string> = {
  pdf: "PDF",
  document: "Document",
  presentation: "Presentation",
  spreadsheet: "Spreadsheet",
  markdown: "Markdown",
  text: "Text",
  image: "Image",
  html: "HTML",
  csv: "CSV",
  url: "URL",
  unknown: "Unknown",
};

export const EXTRACTION_STATUS_LABELS: Record<ExtractionStatus, string> = {
  pending: "Pending",
  extracted: "Extracted",
  unsupported: "Unsupported",
  failed: "Failed",
};
