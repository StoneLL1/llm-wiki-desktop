import { invoke } from "@tauri-apps/api/core";
import type { AddImportPathsV2Request } from "../types/importV2File";
import type { BackendTask } from "../types/task";

/** Starts cancellable, durable file discovery; inspect the task result/logs for skips. */
export function startAddImportPathsV2(request: AddImportPathsV2Request): Promise<BackendTask> {
  return invoke<BackendTask>("start_add_import_paths_v2", { request });
}
