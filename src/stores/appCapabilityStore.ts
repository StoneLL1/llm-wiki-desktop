import { create } from "zustand";

import {
  normalizeBackendError,
  type NormalizedBackendError,
} from "../lib/backendError";
import * as api from "../services/appCapabilityApi";
import { listAppTasks } from "../services/appTaskClient";
import type { AppCapabilityView } from "../types/appCapability";
import type { BackendTask } from "../types/task";
import { selectTaskById, useTaskStore } from "./taskStore";

export type AppCapabilityCategoryFilter = "all" | "documents" | "web" | "ocr" | "media_asr";
export type AppCapabilityStatusFilter = "all" | "installed" | "available" | "updating" | "active" | "attention" | "unpublished";
export type AppCapabilityDialogIntent = "details" | "install" | "update" | "retry";
export type AppCapabilityPrimaryAction = "details" | "install" | "update" | "continue" | "cancel" | "retry";

interface AppCapabilityStore {
  capabilities: AppCapabilityView[];
  initialized: boolean;
  loading: boolean;
  error: NormalizedBackendError | null;
  actionError: NormalizedBackendError | null;
  actionErrorCapabilityId: string | null;
  actionErrorOperation: "install" | "continue" | "cancel" | null;
  managementOpen: boolean;
  dialogCapabilityId: string | null;
  dialogIntent: AppCapabilityDialogIntent | null;
  search: string;
  categoryFilter: AppCapabilityCategoryFilter;
  statusFilter: AppCapabilityStatusFilter;
  initialize: () => Promise<void>;
  refresh: (force?: boolean) => Promise<void>;
  openManagement: () => void;
  closeManagement: () => void;
  openDialog: (capabilityId: string, intent: AppCapabilityDialogIntent) => void;
  closeDialog: () => void;
  setSearch: (search: string) => void;
  setCategoryFilter: (filter: AppCapabilityCategoryFilter) => void;
  setStatusFilter: (filter: AppCapabilityStatusFilter) => void;
  confirmInstall: (capabilityId: string) => Promise<BackendTask | null>;
  continueInstall: (capabilityId: string) => Promise<BackendTask | null>;
  cancelInstall: (capabilityId: string) => Promise<BackendTask | null>;
  resetForTests: () => void;
}

const hasDesktopRuntime = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

let initializePromise: Promise<void> | null = null;
let refreshPromise: Promise<void> | null = null;
let refreshPromiseEpoch = 0;
let requestEpoch = 0;
const mutationPromises = new Map<string, Promise<BackendTask | null>>();

function capabilityError(error: unknown): NormalizedBackendError {
  return normalizeBackendError(error, {
    defaultSummaryKey: "backendError.summary.importCapabilityUnavailable",
    defaultActionKind: "retry",
    defaultRecoverable: true,
  });
}

function taskRequestFor(capability: AppCapabilityView) {
  const task = selectTaskById(
    useTaskStore.getState(),
    capability.activeTaskId ?? capability.operation.taskId,
  );
  return task ? {
    task,
    request: {
      taskId: task.id,
      taskRevision: task.updatedAt,
      scope: "app_global" as const,
    },
  } : null;
}

async function requireTaskRequestFor(capability: AppCapabilityView | undefined) {
  if (!capability) {
    throw {
      code: "APP_CAPABILITY_TASK_NOT_FOUND",
      message: "The capability installation task was not found.",
      recoverable: true,
      userActionRequired: false,
    };
  }
  let target = taskRequestFor(capability);
  if (target) return target;
  const appTasks = await listAppTasks();
  for (const task of appTasks) useTaskStore.getState().upsertTask(task);
  target = taskRequestFor(capability);
  if (target) return target;
  throw {
    code: "APP_CAPABILITY_TASK_NOT_FOUND",
    message: "The capability installation task was not found.",
    recoverable: true,
    userActionRequired: false,
  };
}

export function appCapabilityPrimaryAction(
  capability: AppCapabilityView,
): AppCapabilityPrimaryAction {
  switch (capability.operation.state) {
    case "queued":
    case "downloading":
    case "verifying":
    case "installing":
    case "health_checking":
    case "activating":
    case "recovering":
      return "cancel";
    case "paused":
      return "continue";
    case "failed":
      return capability.installAllowed ? "retry" : "details";
    default:
      break;
  }
  if (capability.installation.state === "healthy") {
    return capability.update.state === "available" ? "update" : "details";
  }
  return capability.installAllowed && capability.distribution.state === "published"
    ? "install"
    : "details";
}

export function matchesAppCapabilityStatus(
  capability: AppCapabilityView,
  filter: AppCapabilityStatusFilter,
): boolean {
  if (filter === "all") return true;
  const action = appCapabilityPrimaryAction(capability);
  switch (filter) {
    case "installed": return capability.installation.state === "healthy";
    case "available": return action === "install";
    case "updating": return capability.update.state === "available" || capability.update.state === "in_progress";
    case "active": return ["cancel", "continue"].includes(action);
    case "attention": return action === "retry"
      || capability.installation.state === "unhealthy"
      || capability.update.state === "rollback_restored"
      || ["source_catalog_empty", "unsupported"].includes(capability.distribution.state);
    case "unpublished": return capability.distribution.state === "not_published_for_target";
  }
}

export const useAppCapabilityStore = create<AppCapabilityStore>((set, get) => ({
  capabilities: [],
  initialized: false,
  loading: false,
  error: null,
  actionError: null,
  actionErrorCapabilityId: null,
  actionErrorOperation: null,
  managementOpen: false,
  dialogCapabilityId: null,
  dialogIntent: null,
  search: "",
  categoryFilter: "all",
  statusFilter: "all",

  initialize: () => {
    if (get().initialized) return Promise.resolve();
    if (initializePromise) return initializePromise;
    initializePromise = get().refresh().finally(() => {
      initializePromise = null;
    });
    return initializePromise;
  },

  refresh: (force = false) => {
    if (!force && refreshPromise && refreshPromiseEpoch === requestEpoch) return refreshPromise;
    if (!hasDesktopRuntime()) {
      set({ initialized: true, loading: false });
      return Promise.resolve();
    }
    const epoch = ++requestEpoch;
    refreshPromiseEpoch = epoch;
    set({ loading: true, error: null });
    refreshPromise = Promise.resolve(api.listAppCapabilities()).then((capabilities) => {
      if (epoch !== requestEpoch) return;
      set({
        capabilities: Array.isArray(capabilities) ? capabilities : [],
        initialized: true,
        loading: false,
        error: null,
      });
    }).catch((error) => {
      if (epoch === requestEpoch) {
        set({ initialized: true, loading: false, error: capabilityError(error) });
      }
      throw error;
    }).finally(() => {
      if (refreshPromiseEpoch === epoch) refreshPromise = null;
    });
    return refreshPromise;
  },

  openManagement: () => {
    set({ managementOpen: true });
    void get().initialize().catch(() => undefined);
  },
  closeManagement: () => set({ managementOpen: false, dialogCapabilityId: null, dialogIntent: null }),
  openDialog: (dialogCapabilityId, dialogIntent) => set({ dialogCapabilityId, dialogIntent }),
  closeDialog: () => set({ dialogCapabilityId: null, dialogIntent: null }),
  setSearch: (search) => set({ search }),
  setCategoryFilter: (categoryFilter) => set({ categoryFilter }),
  setStatusFilter: (statusFilter) => set({ statusFilter }),

  confirmInstall: (capabilityId) => {
    const existing = mutationPromises.get(`install:${capabilityId}`);
    if (existing) return existing;
    const capability = get().capabilities.find((candidate) => candidate.capabilityId === capabilityId);
    if (!capability?.installAllowed || !capability.targetVersion || !capability.acknowledgementVersion) {
      return Promise.resolve(null);
    }
    requestEpoch += 1;
    set({ actionError: null, actionErrorCapabilityId: null, actionErrorOperation: null });
    const mutation = api.installAppCapability({
      capabilityId,
      expectedVersion: capability.targetVersion,
      acknowledgementVersion: capability.acknowledgementVersion,
    }).then(async (task) => {
      useTaskStore.getState().upsertTask(task);
      if (get().dialogCapabilityId === capabilityId) {
        set({ dialogCapabilityId: null, dialogIntent: null });
      }
      await get().refresh();
      return task;
    }).catch((error) => {
      set({ actionError: capabilityError(error), actionErrorCapabilityId: capabilityId, actionErrorOperation: "install" });
      throw error;
    }).finally(() => {
      mutationPromises.delete(`install:${capabilityId}`);
    });
    mutationPromises.set(`install:${capabilityId}`, mutation);
    return mutation;
  },

  continueInstall: (capabilityId) => {
    const existing = mutationPromises.get(`continue:${capabilityId}`);
    if (existing) return existing;
    const capability = get().capabilities.find((candidate) => candidate.capabilityId === capabilityId);
    set({ actionError: null, actionErrorCapabilityId: null, actionErrorOperation: null });
    const mutation = requireTaskRequestFor(capability).then((target) => api.resumeAppCapabilityInstall(target.request)).then(async (task) => {
      useTaskStore.getState().upsertTask(task);
      await get().refresh();
      return task;
    }).catch((error) => {
      set({ actionError: capabilityError(error), actionErrorCapabilityId: capabilityId, actionErrorOperation: "continue" });
      throw error;
    }).finally(() => {
      mutationPromises.delete(`continue:${capabilityId}`);
    });
    mutationPromises.set(`continue:${capabilityId}`, mutation);
    return mutation;
  },

  cancelInstall: (capabilityId) => {
    const existing = mutationPromises.get(`cancel:${capabilityId}`);
    if (existing) return existing;
    const capability = get().capabilities.find((candidate) => candidate.capabilityId === capabilityId);
    set({ actionError: null, actionErrorCapabilityId: null, actionErrorOperation: null });
    const mutation = requireTaskRequestFor(capability).then((target) => api.cancelAppCapabilityInstall(target.request)).then(async (task) => {
      useTaskStore.getState().upsertTask(task);
      await get().refresh();
      return task;
    }).catch((error) => {
      set({ actionError: capabilityError(error), actionErrorCapabilityId: capabilityId, actionErrorOperation: "cancel" });
      throw error;
    }).finally(() => {
      mutationPromises.delete(`cancel:${capabilityId}`);
    });
    mutationPromises.set(`cancel:${capabilityId}`, mutation);
    return mutation;
  },

  resetForTests: () => {
    initializePromise = null;
    refreshPromise = null;
    refreshPromiseEpoch = 0;
    requestEpoch = 0;
    mutationPromises.clear();
    set({
      capabilities: [],
      initialized: false,
      loading: false,
      error: null,
      actionError: null,
      actionErrorCapabilityId: null,
      actionErrorOperation: null,
      managementOpen: false,
      dialogCapabilityId: null,
      dialogIntent: null,
      search: "",
      categoryFilter: "all",
      statusFilter: "all",
    });
  },
}));
