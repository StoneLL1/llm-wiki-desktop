import type { MigrationReport } from "./importV2Migration";

export type ImportBackend = "v2";

export interface PackageGateEvidence {
  package: string;
  contractVersion: string;
  releaseGatePassed: boolean;
}

export interface ExternalToolLicenseEvidence {
  name: string;
  license: string;
  version: string;
  platform: string;
  hashOrSignature: string;
  sizeBytes: number;
  fallback: string;
}

export interface MigrationReadinessEvidence {
  coreRecoveryPassed: boolean;
  packageGates: PackageGateEvidence[];
  fixtureMatrixPassed: boolean;
  idempotencePassed: boolean;
  legacyImmutabilityPassed: boolean;
  longTaskRecoveryPassed: boolean;
  licenseEvidence: ExternalToolLicenseEvidence[];
}

export interface ImportBackendActivation {
  schemaVersion: number;
  activeBackend: ImportBackend;
  coreContractVersion: string;
  migrationReportFingerprint: string;
  activatedAt: string;
  releaseVersion: string;
  legacyMutationsDisabled: boolean;
  rollbackMode: "release_based";
}

export interface ActivationConfirmation {
  reportFingerprint: string;
  token: string;
  acknowledgeNoGitRollback: boolean;
}

export interface ActivationResult {
  record: ImportBackendActivation;
  checkpoint?: MigrationGitCheckpoint | null;
}

export interface MigrationGitCheckpoint {
  created: boolean;
  commitHash?: string | null;
  message: string;
  purpose: "initial_project" | "high_risk_operation" | "final_result";
  affectedPaths: string[];
}

export interface ActivateImportV2Request {
  projectId: string;
  projectRootPath: string;
  report: MigrationReport;
  readiness: MigrationReadinessEvidence;
  releaseVersion: string;
  confirmation: ActivationConfirmation;
}

export interface GetImportBackendActivationRequest {
  projectId: string;
  projectRootPath: string;
}
