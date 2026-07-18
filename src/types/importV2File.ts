export const FILE_FORMATS = [
  "markdown", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "pdf",
] as const;
export type FileFormat = (typeof FILE_FORMATS)[number];

export const FILE_SKIP_REASONS = [
  "symlink_or_reparse_point", "hidden_or_system", "project_internal",
  "unsupported_format", "cycle_detected", "depth_limit_exceeded",
  "file_limit_exceeded", "file_too_large", "duplicate", "case_collision",
  "unicode_normalization_collision", "invalid_path", "unreadable",
] as const;
export type FileSkipReason = (typeof FILE_SKIP_REASONS)[number];

export interface FileIdentity { extension: string; magic: string; mime: string }
export interface DiscoveredFile {
  sourcePath: string; relativePath: string; displayName: string; format: FileFormat;
  sizeBytes: number; identity: FileIdentity;
}
export interface SkippedFile {
  sourcePath: string; relativePath?: string; reason: FileSkipReason; detail?: string;
}
export interface FileScanPolicy {
  maxDepth: number; maxFiles: number; maxFileBytes: number; includeHidden: boolean;
}
export interface FileScanResult { files: DiscoveredFile[]; skipped: SkippedFile[]; truncated: boolean }
export interface CapabilityRequirement {
  capabilityId: string; minimumVersion?: string; protocolVersion: string;
  targetTriple: string; acceptedLicenseExpressions: string[];
}
export interface AddImportPathsV2Request {
  projectId: string;
  projectRootPath: string;
  sessionId: string;
  sourcePaths: string[];
}

export interface GetImportScanResultV2Request {
  projectId: string;
  projectRootPath: string;
  sessionId: string;
  taskId: string;
}
