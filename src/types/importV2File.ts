import type { SourceIdentity } from "./importV2";

export const FILE_FORMATS = [
  "markdown", "text", "html", "csv", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "pdf",
  "png", "jpeg", "webp", "bmp", "tiff", "heic", "heif", "animated_gif",
  "mp3", "wav", "m4a", "aac", "flac", "ogg", "opus", "wma",
  "mp4", "mov", "mkv", "webm", "avi", "m4v", "wmv",
  "srt", "vtt", "ass", "lrc",
] as const;
export type FileFormat = (typeof FILE_FORMATS)[number];

export const FILE_CONTENT_KINDS = [
  "document", "image", "audio", "video", "subtitle",
] as const;
export type FileContentKind = (typeof FILE_CONTENT_KINDS)[number];

export const FILE_DETECTION_METHODS = [
  "magic", "container", "structured_text", "extension_fallback",
] as const;
export type FileDetectionMethod = (typeof FILE_DETECTION_METHODS)[number];

export const FILE_SKIP_REASONS = [
  "symlink_or_reparse_point", "hidden_or_system", "ignored_directory", "project_internal",
  "unsupported_format", "cycle_detected", "depth_limit_exceeded",
  "file_limit_exceeded", "file_too_large", "insufficient_disk", "large_data_confirmation_required",
  "duplicate", "invalid_path", "non_utf8_path", "unreadable",
] as const;
export type FileSkipReason = (typeof FILE_SKIP_REASONS)[number];

export interface FileIdentity {
  extension: string;
  magic: string;
  mime: string;
  detectionMethod: FileDetectionMethod;
  extensionMismatch: boolean;
}
export interface LargeDataEstimate {
  rowCount: number;
  sheetCount?: number;
  estimatedOutputFiles: number;
  totalBytes: number;
  requiresConfirmation: boolean;
  estimateComplete?: boolean;
}
export interface DiscoveredFile {
  sourcePath: string; relativePath: string; displayName: string; format: FileFormat;
  contentKind: FileContentKind; sizeBytes: number; identity: FileIdentity;
  sourceIdentity: SourceIdentity; largeData?: LargeDataEstimate;
}
export interface SkippedFile {
  sourcePath: string; relativePath?: string; reason: FileSkipReason; detail?: string;
}
export interface FileScanPolicy {
  maxDepth: number; maxFiles: number; maxFileBytes: number; includeHidden: boolean;
}
export type ImportScanConfirmationReason = "file_count" | "total_bytes" | "estimated_output_files";
export interface ImportScanTotals {
  fileCount: number;
  totalBytes: number;
  estimatedOutputFiles?: number;
  requiresConfirmation: boolean;
  reasons: ImportScanConfirmationReason[];
}
export interface FileScanResult {
  files: DiscoveredFile[];
  skipped: SkippedFile[];
  truncated: boolean;
  scanIdentity?: ImportScanIdentity;
  totals?: ImportScanTotals;
  confirmationToken?: string;
  acceptedAt?: string;
  aggregateConfirmedAt?: string;
  discardedAt?: string;
}

export interface ImportScanIdentity {
  projectId: string;
  projectRootPath: string;
  sessionId: string;
  taskId: string;
}
export interface CapabilityRequirement {
  capabilityId: string; minimumVersion?: string; protocolVersion: string;
  targetTriple: string; acceptedLicenseExpressions: string[];
}
export interface AddImportPathsV2Request {
  projectId: string;
  projectRootPath: string;
  sessionId: string;
  sourcePaths: string[];
  largeDataConfirmed?: boolean;
}

export interface GetImportScanResultV2Request {
  projectId: string;
  projectRootPath: string;
  sessionId: string;
  taskId: string;
}

export interface AcceptImportScanV2Request extends GetImportScanResultV2Request {
  confirmationToken: string;
  acknowledgeAggregate?: boolean;
  sourcePaths?: string[];
}

export interface AcceptImportScanV2Result {
  sessionId: string;
  semanticRevision: number;
  acceptedItemCount: number;
  operationTask?: import("./task").BackendTask | null;
  overview: import("./importV2").ImportSessionOverview;
  scan: FileScanResult;
}

export interface DiscardImportScanV2Request extends GetImportScanResultV2Request {
  confirmationToken: string;
}
