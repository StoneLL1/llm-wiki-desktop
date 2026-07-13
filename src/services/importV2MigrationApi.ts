import { invoke } from "@tauri-apps/api/core";
import type {
  ApplyImportV2MigrationRequest,
  LegacyInventory,
  MigrationApplyTask,
  MigrationPlan,
  MigrationStatusSnapshot,
  PlanImportV2MigrationRequest,
  ResumeImportV2MigrationRequest,
  ScanImportV2MigrationRequest,
  MigrationProjectRequest,
} from "../types/importV2Migration";

export function scanImportV2Migration(
  request: ScanImportV2MigrationRequest,
): Promise<LegacyInventory> {
  return invoke<LegacyInventory>("scan_import_v2_migration", { request });
}

export function planImportV2Migration(
  request: PlanImportV2MigrationRequest,
): Promise<MigrationPlan> {
  return invoke<MigrationPlan>("plan_import_v2_migration", { request });
}

export function applyImportV2Migration(
  request: ApplyImportV2MigrationRequest,
): Promise<MigrationApplyTask> {
  return invoke<MigrationApplyTask>("apply_import_v2_migration", { request });
}

export function getImportV2MigrationStatus(
  request: MigrationProjectRequest,
): Promise<MigrationStatusSnapshot> {
  return invoke<MigrationStatusSnapshot>("get_import_v2_migration_status", { request });
}

export function resumeImportV2Migration(
  request: ResumeImportV2MigrationRequest,
): Promise<MigrationApplyTask> {
  return invoke<MigrationApplyTask>("resume_import_v2_migration", { request });
}
