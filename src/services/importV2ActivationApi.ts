import { invoke } from "@tauri-apps/api/core";
import type {
  ActivateImportV2Request,
  ActivationResult,
  GetImportBackendActivationRequest,
  ImportBackendActivation,
} from "../types/importV2Activation";

export function activateImportV2(
  request: ActivateImportV2Request,
): Promise<ActivationResult> {
  return invoke<ActivationResult>("activate_import_v2", { request });
}

export function getImportBackendActivation(
  request: GetImportBackendActivationRequest,
): Promise<ImportBackendActivation | null> {
  return invoke<ImportBackendActivation | null>(
    "get_import_backend_activation",
    { request },
  );
}
