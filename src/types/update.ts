import type { BackendError } from "./task";

export type UpdatePhase =
  | "idle"
  | "checking"
  | "available"
  | "downloading"
  | "downloaded"
  | "installing"
  | "installed"
  | "cancelled"
  | "error";

export type UpdateFrequency = "daily" | "weekly" | "never";

export interface UpdateOffer {
  offerId: string;
  currentVersion: string;
  version: string;
  target: string;
  arch: string;
  notes: string | null;
  publishedAt: string | null;
  createdAtUnixSeconds: number;
  expiresAtUnixSeconds: number;
}

export interface AppUpdateState {
  phase: UpdatePhase;
  offer: UpdateOffer | null;
  downloadedBytes: number;
  totalBytes: number | null;
  error: BackendError | null;
}

export interface UpdateProgressEvent {
  phase: UpdatePhase;
  downloadedBytes: number;
  totalBytes: number | null;
}

export interface GlobalUpdatePreferences {
  checkUpdates: boolean;
  updateFrequency: UpdateFrequency;
  autoDownloadUpdates: boolean;
  promptChangelogBeforeInstall: boolean;
  lastCheckedAt: string | null;
  dismissedOfferId: string | null;
  dismissedVersion: string | null;
}

export interface SaveGlobalUpdatePreferences {
  checkUpdates: boolean;
  updateFrequency: UpdateFrequency;
  autoDownloadUpdates: boolean;
  promptChangelogBeforeInstall: boolean;
}

export interface UpdateInstallRequest {
  offerId: string;
  restartConsent: boolean;
  unsavedEditor: boolean;
  importCommitActive: boolean;
  pendingUserConfirmation: boolean;
}

export type UpdateUiStatus =
  | "idle"
  | "checking"
  | "up_to_date"
  | "available"
  | "downloading"
  | "paused_or_cancelled"
  | "ready_to_install"
  | "installing"
  | "restart_required"
  | "error";

export type UpdateInstallBlocker =
  | "unsaved_editor"
  | "editor_saving"
  | "import_commit"
  | "workflow_apply"
  | "critical_task"
  | "pending_confirmation";

export interface UpdateInstallGuardSnapshot {
  blockers: UpdateInstallBlocker[];
  safeRunningTaskCount: number;
  request: Omit<UpdateInstallRequest, "offerId" | "restartConsent">;
}
