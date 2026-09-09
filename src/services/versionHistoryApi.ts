import { invoke } from "@tauri-apps/api/core";
import type { PendingAction } from "../types/backend";
import type { GitRepositoryStatus } from "../stores/projectFactsStore";

export type VersionKind = "lint_fix" | "wiki_update" | "agent_repair" | "chat_edit" | "page_change" | "source_change" | "manual_snapshot" | "restore";
export type VersionState = "prepared" | "applied" | "restoring" | "restored" | "aborted";
export interface VersionSummary {
  operationId: string;
  kind: VersionKind;
  createdAt: string;
  state: VersionState;
  fileCount: number;
  taskId: string | null;
}
export interface VersionOperation {
  schemaVersion: number;
  summary: VersionSummary;
  before: string;
  after: string | null;
  restorationId?: string | null;
  restoredFrom?: string | null;
  sourceDeletion?: { sourceId: string; title: string; wikiPath: string } | null;
  beforeHashes: Record<string, string | null>;
  expectedHashes: Record<string, string | null>;
}
export interface VersionPage {
  operations: VersionSummary[];
  nextCursor: string | null;
  unreadableCount: number;
}
export interface VersionStatus {
  enabled: boolean;
  git: GitRepositoryStatus;
  gitVersion: string;
}
export interface VersionFileDiff {
  path: string;
  beforeText: string | null;
  afterText: string | null;
  beforeBytes: number;
  afterBytes: number;
  binary: boolean;
  truncated: boolean;
}
export interface VersionRequest {
  projectId: string;
  projectRootPath: string;
  operationId?: string;
  path?: string;
  cursor?: string;
  limit?: number;
  save?: boolean;
}
export const getVersionStatus = (request: VersionRequest) => invoke<VersionStatus>("get_version_history_status", { request });
export const listVersions = (request: VersionRequest) => invoke<VersionPage>("list_version_operations", { request });
export const getVersion = (request: VersionRequest) => invoke<VersionOperation>("get_version_operation", { request });
export const getVersionDiff = (request: VersionRequest) => invoke<VersionFileDiff>("get_version_file_diff", { request });
export const prepareVersionAction = (request: VersionRequest) => invoke<PendingAction>("prepare_version_action", { request });
export const confirmVersionAction = (actionId: string, confirmed: boolean) => invoke<void>("confirm_version_action", {
  request: { actionId, status: confirmed ? "confirmed" : "cancelled" },
});
