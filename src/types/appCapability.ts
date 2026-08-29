export type AppCapabilityDistributionState =
  | "published"
  | "source_catalog_empty"
  | "not_published_for_target"
  | "unsupported";

export type AppCapabilityInstallationState = "absent" | "healthy" | "unhealthy";

export type AppCapabilityOperationState =
  | "queued"
  | "downloading"
  | "paused"
  | "verifying"
  | "installing"
  | "health_checking"
  | "activating"
  | "recovering"
  | "failed"
  | "cancelled"
  | "succeeded";

export interface AppCapabilityView {
  capabilityId: string;
  nameKey: string;
  purposeKey: string;
  category: string;
  routes: string[];
  formats: string[];
  platformContentTypes: string[];
  targetTriple: string;
  targetVersion?: string;
  acknowledgementVersion?: string;
  distribution: { state: AppCapabilityDistributionState; errorCode?: string };
  installation: { state: AppCapabilityInstallationState; healthyVersion?: string };
  operation: {
    state?: AppCapabilityOperationState;
    taskId?: string;
    progressCurrent?: number;
    progressTotal?: number;
    errorCode?: string;
  };
  update: {
    state: "none" | "available" | "in_progress" | "rollback_restored";
    availableVersion?: string;
  };
  displayState: string;
  compressedBytes?: number;
  installedBytes?: number;
  modelBytes?: number;
  licenseExpression: string;
  thirdPartyNotices: string[];
  runtimeNetwork: boolean;
  runtimeSubprocess: boolean;
  runtimeFilesystem: string[];
  activeTaskId?: string;
  currentProjectWaitingCount: number;
  errorCode?: string;
}

export interface InstallAppCapabilityV1Request {
  capabilityId: string;
  expectedVersion: string;
  acknowledgementVersion: string;
}

export interface AppCapabilityTaskControlRequest {
  taskId: string;
  taskRevision: string;
  scope: "app_global";
}
