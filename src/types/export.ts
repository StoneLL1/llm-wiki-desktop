import type { AgentKind } from "./agent";
import type { LlmProviderKind } from "./llm";

export type ExportType = "beautiful_read" | "knowledge_card" | "concept_map" | "project_report";

export type ExportRoute = "agent" | "byok";

export type ExportStatus = "succeeded" | "failed";

export type ExportRoutePreference = "auto" | "agent" | "byok";

export interface ExportRecord {
  id: string;
  exportType: ExportType;
  title: string;
  sourcePath?: string;
  outputPath: string;
  createdAt: string;
  route: ExportRoute;
  status: ExportStatus;
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
}

export interface RegenerateExportRequest {
  projectId: string;
  projectRootPath: string;
  exportType: ExportType;
  sourcePath?: string | null;
  route?: ExportRoutePreference;
  agent?: AgentKind | null;
  provider?: LlmProviderKind | null;
}

export interface ListExportsRequest {
  projectId: string;
  projectRootPath: string;
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
