import { invoke } from "@tauri-apps/api/core";
import type { AddImportPathsV2Request, FileScanResult } from "../types/importV2File";
import type { AddImportItemsV2Request, ImportSession } from "../types/importV2";
import type { BackendTask } from "../types/task";

/** Starts cancellable, durable file discovery; inspect the task result/logs for skips. */
export function startAddImportPathsV2(request: AddImportPathsV2Request): Promise<BackendTask> {
  return invoke<BackendTask>("start_add_import_paths_v2", { request });
}

export interface ImportCapabilityStatus {
  capabilityId: string;
  route: string;
  available: boolean;
  reason?: string;
}

export function getImportCapabilityStatuses(): Promise<ImportCapabilityStatus[]> {
  return invoke<ImportCapabilityStatus[]>("get_import_capability_statuses");
}

export interface GetImportScanResultV2Request {
  projectId: string;
  projectRootPath: string;
  sessionId: string;
  taskId: string;
}

export function getImportScanResultV2(
  request: GetImportScanResultV2Request,
): Promise<FileScanResult> {
  return invoke<FileScanResult>("get_import_scan_result_v2", { request });
}

export function addImportItemsV2(
  request: AddImportItemsV2Request,
): Promise<ImportSession> {
  return invoke<ImportSession>("add_import_items_v2", { request });
}

export interface AddImportTextV2Request {
  projectId: string;
  projectRootPath: string;
  sessionId: string;
  sourceName: string;
  content: string;
}

export function addImportTextV2(
  request: AddImportTextV2Request,
): Promise<ImportSession> {
  return invoke<ImportSession>("add_import_text_v2", { request });
}
