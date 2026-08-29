import type { BackendTask } from "../../types/task";

export type CapabilityInstallStateKind =
  | "not_installed"
  | "downloading"
  | "paused"
  | "verifying"
  | "installing"
  | "health_check_failed"
  | "catalog_unavailable"
  | "runtime_confinement_unavailable"
  | "signed_release_unavailable"
  | "installed";

export interface CapabilityInstallState {
  kind: CapabilityInstallStateKind;
  downloadedBytes: number | null;
  totalBytes: number | null;
  task: BackendTask | null;
}

export function capabilityInstallState(
  task: BackendTask | null,
  installable: boolean,
  available: boolean,
  unavailableReasonCode: string | null = null,
): CapabilityInstallState {
  const active = task !== null && ["queued", "running", "cancelling"].includes(task.status);
  if (task?.status === "interrupted") {
    return { kind: "paused", downloadedBytes: task.progress?.current ?? null, totalBytes: task.progress?.total ?? null, task };
  }
  if (available || task?.status === "succeeded") {
    return { kind: "installed", downloadedBytes: task?.progress?.current ?? null, totalBytes: task?.progress?.total ?? null, task };
  }
  if (!installable && !active) {
    const unavailableKind =
      unavailableReasonCode === "catalog_unavailable"
        ? "catalog_unavailable"
        : unavailableReasonCode === "runtime_confinement_unavailable"
          ? "runtime_confinement_unavailable"
          : "signed_release_unavailable";
    return {
      kind: unavailableKind,
      downloadedBytes: null,
      totalBytes: null,
      task,
    };
  }
  if (task?.status === "cancelled") {
    return { kind: "not_installed", downloadedBytes: null, totalBytes: null, task };
  }
  if (task?.status === "failed" && task.error?.code === "IMPORT_V2_CAPABILITY_HEALTH_CHECK_FAILED") {
    return { kind: "health_check_failed", downloadedBytes: task.progress?.current ?? null, totalBytes: task.progress?.total ?? null, task };
  }
  const phase = task?.progress?.label;
  if (task && ["queued", "running", "cancelling"].includes(task.status)) {
    if (phase === "capability.verifying" || phase === "capability.health_check") {
      return { kind: "verifying", downloadedBytes: task.progress?.current ?? null, totalBytes: task.progress?.total ?? null, task };
    }
    if (phase === "capability.installing") {
      return { kind: "installing", downloadedBytes: task.progress?.current ?? null, totalBytes: task.progress?.total ?? null, task };
    }
    return { kind: "downloading", downloadedBytes: task.progress?.current ?? 0, totalBytes: task.progress?.total ?? null, task };
  }
  return { kind: "not_installed", downloadedBytes: null, totalBytes: null, task };
}
