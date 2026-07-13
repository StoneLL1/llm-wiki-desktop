import type { BackendTask } from "./task";

export type MatchConfidence =
  | "exactStableSourceId"
  | "exactHashUniqueDestination"
  | "exactHashNormalizedUrl";

export interface LegacyRecord {
  recordId: string;
  stableSourceId?: string | null;
  originalPath?: string | null;
  destinationPath?: string | null;
  originalSha256?: string | null;
  normalizedUrl?: string | null;
  recordedContentSha256?: string | null;
  metadataPath: string;
}

export interface MigrationWarning {
  code: string;
  message: string;
  relativePath?: string | null;
  redacted: boolean;
}

export interface LegacyFileEvidence {
  relativePath: string;
  sha256: string;
  sizeBytes: number;
  modifiedNanos?: number | null;
}

export interface LegacyInventory {
  schemaVersion: number;
  projectIdentity: string;
  fingerprint: string;
  records: LegacyRecord[];
  warnings: MigrationWarning[];
  scannedFiles: LegacyFileEvidence[];
}

export type MigrationDecision =
  | { kind: "linkExisting"; sourceId: string; confidence: MatchConfidence }
  | { kind: "createV2Record"; proposedSourceId: string }
  | { kind: "legacyUnmanaged"; reason: string }
  | { kind: "conflict"; candidates: string[]; reason: string };

export interface MigrationCandidate {
  candidateId: string;
  record: LegacyRecord;
  decision: MigrationDecision;
  evidence: string[];
}

export interface MigrationSummary {
  total: number;
  automaticLinks: number;
  proposedRecords: number;
  conflicts: number;
  legacyUnmanaged: number;
  warnings: number;
}

export interface MigrationPlan {
  planVersion: number;
  v2IndexFingerprint: string;
  inventoryFingerprint: string;
  candidates: MigrationCandidate[];
  summary: MigrationSummary;
}

export type MigrationStatus =
  | "not_scanned"
  | "dry_run_ready"
  | "awaiting_confirmation"
  | "applying"
  | "applied"
  | "verification_failed"
  | "cancelled";

export interface MigrationConfirmation {
  planFingerprint: string;
  token: string;
  acknowledgeNoGitRollback: boolean;
}

export interface MigrationGitCheckpoint {
  created: boolean;
  commitHash?: string | null;
  message: string;
  purpose: "initial_project" | "high_risk_operation" | "final_result";
  affectedPaths: string[];
}

export interface MigrationApplyResult {
  status: MigrationStatus;
  planFingerprint: string;
  appliedCandidateIds: string[];
  reportRelativePath: string;
  checkpoint?: MigrationGitCheckpoint | null;
}

export interface MigrationReport {
  reportVersion: number;
  planVersion: number;
  planFingerprint: string;
  inventoryFingerprint: string;
  status: MigrationStatus;
  summary: MigrationSummary;
  automaticLinks: MigrationCandidate[];
  proposedRecords: MigrationCandidate[];
  conflicts: MigrationCandidate[];
  legacyUnmanaged: MigrationCandidate[];
  warnings: MigrationWarning[];
  affectedMetadataPaths: string[];
  untouchedContentPaths: string[];
  rollbackStatement: string;
  requiredConfirmation: boolean;
}

export interface MigrationStatusSnapshot {
  status: MigrationStatus;
  planFingerprint?: string | null;
  report?: MigrationReport | null;
}

export interface MigrationProjectRequest {
  projectId: string;
  projectRootPath: string;
}

export type ScanImportV2MigrationRequest = MigrationProjectRequest;
export interface PlanImportV2MigrationRequest extends MigrationProjectRequest {
  inventory: LegacyInventory;
}
export interface ApplyImportV2MigrationRequest extends MigrationProjectRequest {
  plan: MigrationPlan;
  confirmation: MigrationConfirmation;
}
export type ResumeImportV2MigrationRequest = ApplyImportV2MigrationRequest;

export type MigrationApplyTask = BackendTask;
