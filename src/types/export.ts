import type { AgentKind } from "./agent";
import type { LlmProviderKind } from "./llm";

export type ExportType = "beautiful_read" | "knowledge_card" | "concept_map" | "project_report";

export type ExportRoute = "agent" | "byok";

export type ExportStatus = "succeeded" | "failed";

export type ExportRoutePreference = "auto" | "agent" | "byok";

export type ExportPreviewMode = "inline" | "source";

/**
 * User-controlled content flags for an export. Mirrors the backend
 * `ExportContentOptions`; these adjust the prompt only. `embedCss` reflects the
 * always-on self-contained HTML contract.
 */
export interface ExportContentOptions {
  includeFrontmatter: boolean;
  embedCss: boolean;
  embedImages: boolean;
}

export const DEFAULT_EXPORT_OPTIONS: ExportContentOptions = {
  includeFrontmatter: true,
  embedCss: true,
  embedImages: false,
};

export interface ExportRecord {
  id: string;
  exportType: ExportType;
  title: string;
  sourcePath?: string;
  outputPath: string;
  createdAt: string;
  route: ExportRoute;
  status: ExportStatus;
  bookmarked: boolean;
  taskId?: string;
}

export interface StartExportRequest {
  projectId: string;
  projectRootPath: string;
  exportType: ExportType;
  sourcePath?: string | null;
  route?: ExportRoutePreference;
  agent?: AgentKind | null;
  provider?: LlmProviderKind | null;
  template?: string | null;
  options?: ExportContentOptions;
  acknowledgeRestrictedContent?: boolean;
}

export interface RegenerateExportRequest {
  projectId: string;
  projectRootPath: string;
  exportType: ExportType;
  sourcePath?: string | null;
  route?: ExportRoutePreference;
  agent?: AgentKind | null;
  provider?: LlmProviderKind | null;
  template?: string | null;
  options?: ExportContentOptions;
  acknowledgeRestrictedContent?: boolean;
}

export interface GetExportRestrictedContentStatusRequest {
  projectId: string;
  projectRootPath: string;
  exportType: ExportType;
  sourcePath?: string | null;
}

export interface ExportRestrictedContentStatus {
  containsRestrictedContent: boolean;
  restrictedSourceCount: number;
}

export interface ListExportsRequest {
  projectId: string;
  projectRootPath: string;
}

export interface ToggleExportBookmarkRequest {
  projectId: string;
  projectRootPath: string;
  exportRecordId: string;
}

export interface ToggleExportBookmarkResponse {
  exportRecordId: string;
  bookmarked: boolean;
}

export interface ReadExportPreviewRequest {
  projectId: string;
  projectRootPath: string;
  outputPath: string;
}

export interface OpenExportFolderRequest {
  projectId: string;
  projectRootPath: string;
  outputPath: string;
}

export interface OpenExportInBrowserRequest {
  projectId: string;
  projectRootPath: string;
  outputPath: string;
}

/** Export types scoped to a single source page (vs. project-wide). */
export const SINGLE_PAGE_EXPORT_TYPES: ExportType[] = [
  "beautiful_read",
  "knowledge_card",
  "concept_map",
];

export const EXPORT_TYPE_ORDER: ExportType[] = [
  "beautiful_read",
  "knowledge_card",
  "concept_map",
  "project_report",
];
