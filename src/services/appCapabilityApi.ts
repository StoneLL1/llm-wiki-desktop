import { invoke } from "@tauri-apps/api/core";

import type {
  AppCapabilityTaskControlRequest,
  AppCapabilityView,
  InstallAppCapabilityV1Request,
} from "../types/appCapability";
import type { BackendTask } from "../types/task";

export const listAppCapabilities = (): Promise<AppCapabilityView[]> =>
  invoke<AppCapabilityView[]>("list_app_capabilities_v1");

export const installAppCapability = (
  request: InstallAppCapabilityV1Request,
): Promise<BackendTask> =>
  invoke<BackendTask>("install_app_capability_v1", { request });

export const pauseAppCapabilityInstall = (
  request: AppCapabilityTaskControlRequest,
): Promise<BackendTask> =>
  invoke<BackendTask>("pause_app_capability_install_v1", { request });

export const resumeAppCapabilityInstall = (
  request: AppCapabilityTaskControlRequest,
): Promise<BackendTask> =>
  invoke<BackendTask>("resume_app_capability_install_v1", { request });

export const cancelAppCapabilityInstall = (
  request: AppCapabilityTaskControlRequest,
): Promise<BackendTask> =>
  invoke<BackendTask>("cancel_app_capability_install_v1", { request });
