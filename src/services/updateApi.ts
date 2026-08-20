import { Channel, invoke } from "@tauri-apps/api/core";

import type {
  AppUpdateState,
  GlobalUpdatePreferences,
  SaveGlobalUpdatePreferences,
  UpdateInstallRequest,
  UpdateProgressEvent,
} from "../types/update";

export interface AppSummary {
  name: string;
  version: string;
}

export const getAppSummary = (): Promise<AppSummary> =>
  invoke<AppSummary>("get_app_summary");

export const getUpdateState = (): Promise<AppUpdateState> =>
  invoke<AppUpdateState>("get_update_state");

export const getGlobalUpdatePreferences = (): Promise<GlobalUpdatePreferences> =>
  invoke<GlobalUpdatePreferences>("get_global_update_preferences");

export const saveGlobalUpdatePreferences = (
  preferences: SaveGlobalUpdatePreferences,
): Promise<GlobalUpdatePreferences> =>
  invoke<GlobalUpdatePreferences>("save_global_update_preferences", { preferences });

export const checkAppUpdate = (): Promise<AppUpdateState> =>
  invoke<AppUpdateState>("check_app_update");

export const downloadAppUpdate = (
  offerId: string,
  onProgress: (event: UpdateProgressEvent) => void,
): Promise<AppUpdateState> => {
  const channel = new Channel<UpdateProgressEvent>();
  channel.onmessage = onProgress;
  return invoke<AppUpdateState>("download_app_update", {
    request: { offerId },
    onProgress: channel,
  });
};

export const cancelAppUpdateDownload = (offerId: string): Promise<AppUpdateState> =>
  invoke<AppUpdateState>("cancel_app_update_download", { request: { offerId } });

export const installAppUpdate = (request: UpdateInstallRequest): Promise<AppUpdateState> =>
  invoke<AppUpdateState>("install_app_update", { request });

export const restartAppAfterUpdate = (request: UpdateInstallRequest): Promise<void> =>
  invoke<void>("restart_app_after_update", { request });

export const dismissAppUpdate = (offerId: string): Promise<AppUpdateState> =>
  invoke<AppUpdateState>("dismiss_app_update", { request: { offerId } });
