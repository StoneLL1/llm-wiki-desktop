import { create } from "zustand";

import { normalizeBackendError, type NormalizedBackendError } from "../lib/backendError";
import * as updateApi from "../services/updateApi";
import type {
  AppUpdateState,
  GlobalUpdatePreferences,
  SaveGlobalUpdatePreferences,
  UpdateInstallGuardSnapshot,
  UpdateProgressEvent,
  UpdateUiStatus,
} from "../types/update";

const DEFAULT_STATE: AppUpdateState = {
  phase: "idle",
  offer: null,
  downloadedBytes: 0,
  totalBytes: null,
  error: null,
};

const DEFAULT_PREFERENCES: GlobalUpdatePreferences = {
  checkUpdates: true,
  updateFrequency: "daily",
  autoDownloadUpdates: false,
  promptChangelogBeforeInstall: true,
  lastCheckedAt: null,
  dismissedOfferId: null,
  dismissedVersion: null,
};

const hasDesktopRuntime = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

let initializePromise: Promise<void> | null = null;
let checkPromise: Promise<AppUpdateState> | null = null;
let downloadPromise: Promise<AppUpdateState> | null = null;
let installPromise: Promise<void> | null = null;
let stateRevision = 0;

function normalizedUpdateError(error: unknown): NormalizedBackendError {
  return normalizeBackendError(error, {
    defaultSummaryKey: "backendError.summary.update",
    defaultActionKind: "retry",
    defaultRecoverable: true,
  });
}

function uiStatusFor(state: AppUpdateState, checkedWithoutOffer: boolean): UpdateUiStatus {
  switch (state.phase) {
    case "checking": return "checking";
    case "available": return "available";
    case "downloading": return "downloading";
    case "downloaded": return "ready_to_install";
    case "installing": return "installing";
    case "installed": return "restart_required";
    case "cancelled": return "paused_or_cancelled";
    case "error": return "error";
    case "idle": return checkedWithoutOffer ? "up_to_date" : "idle";
  }
}

interface UpdateStore {
  backendState: AppUpdateState;
  preferences: GlobalUpdatePreferences;
  currentVersion: string;
  uiStatus: UpdateUiStatus;
  initialized: boolean;
  dialogOpen: boolean;
  preferencesSaving: boolean;
  error: NormalizedBackendError | null;
  installGuard: UpdateInstallGuardSnapshot | null;
  installReviewIntent: "install" | "restart" | null;
  initialize: () => Promise<void>;
  openDialog: (checkNow?: boolean) => void;
  closeDialog: () => void;
  checkNow: (allowAutoDownload?: boolean) => Promise<AppUpdateState>;
  download: () => Promise<AppUpdateState | null>;
  cancelDownload: () => Promise<void>;
  retry: () => Promise<void>;
  ignoreVersion: () => Promise<void>;
  savePreferences: (preferences: SaveGlobalUpdatePreferences) => Promise<void>;
  reviewInstall: (intent: "install" | "restart") => Promise<void>;
  confirmInstallOrRestart: () => Promise<void>;
  clearInstallReview: () => void;
  resetForTests: () => void;
}

export const useUpdateStore = create<UpdateStore>((set, get) => ({
  backendState: DEFAULT_STATE,
  preferences: DEFAULT_PREFERENCES,
  currentVersion: "0.1.0",
  uiStatus: "idle",
  initialized: false,
  dialogOpen: false,
  preferencesSaving: false,
  error: null,
  installGuard: null,
  installReviewIntent: null,

  initialize: () => {
    if (get().initialized) return Promise.resolve();
    if (initializePromise) return initializePromise;
    initializePromise = (async () => {
      if (!hasDesktopRuntime()) {
        set({ initialized: true });
        return;
      }
      const revision = stateRevision;
      try {
        const [backendState, preferences, summary] = await Promise.all([
          updateApi.getUpdateState(),
          updateApi.getGlobalUpdatePreferences(),
          updateApi.getAppSummary(),
        ]);
        if (revision === stateRevision) {
          set({
            backendState,
            preferences,
            currentVersion: summary.version,
            uiStatus: uiStatusFor(backendState, false),
            initialized: true,
            error: backendState.error ? normalizedUpdateError(backendState.error) : null,
          });
        } else {
          // A manual operation completed while the startup snapshot was in
          // flight. Keep that newer state and only finish immutable startup
          // metadata instead of restoring the stale backend snapshot.
          set({ currentVersion: summary.version, initialized: true });
        }
      } catch (error) {
        if (revision === stateRevision) {
          set({ initialized: true, uiStatus: "error", error: normalizedUpdateError(error) });
        } else {
          set({ initialized: true });
        }
      }
    })().finally(() => {
      initializePromise = null;
    });
    return initializePromise;
  },

  openDialog: (checkNow = false) => {
    set({ dialogOpen: true });
    if (checkNow) void get().checkNow().catch(() => undefined);
  },
  closeDialog: () => set({ dialogOpen: false, installGuard: null, installReviewIntent: null }),

  checkNow: (allowAutoDownload = true) => {
    if (checkPromise) return checkPromise;
    stateRevision += 1;
    set({ uiStatus: "checking", error: null, installGuard: null, installReviewIntent: null });
    checkPromise = (async () => {
      try {
        let backendState = await updateApi.checkAppUpdate();
        const preferences = await updateApi.getGlobalUpdatePreferences();
        if (
          backendState.offer
          && preferences.dismissedVersion === backendState.offer.version
        ) {
          backendState = await updateApi.dismissAppUpdate(backendState.offer.offerId);
        }
        set({
          backendState,
          preferences,
          uiStatus: uiStatusFor(backendState, backendState.phase === "idle"),
          error: null,
        });
        if (
          allowAutoDownload
          && preferences.autoDownloadUpdates
          && backendState.phase === "available"
        ) {
          const downloaded = (await get().download()) ?? backendState;
          if (
            preferences.promptChangelogBeforeInstall
            && downloaded.phase === "downloaded"
          ) {
            set({ dialogOpen: true });
          }
          return downloaded;
        }
        return backendState;
      } catch (error) {
        const normalized = normalizedUpdateError(error);
        set({ uiStatus: "error", error: normalized });
        throw error;
      } finally {
        checkPromise = null;
      }
    })();
    return checkPromise;
  },

  download: () => {
    if (downloadPromise) return downloadPromise;
    if (get().backendState.phase === "cancelled") {
      return get().checkNow(false).then((checked) => (
        checked.phase === "available" ? get().download() : checked
      ));
    }
    const offer = get().backendState.offer;
    if (!offer) return Promise.resolve(null);
    set((state) => ({
      backendState: { ...state.backendState, phase: "downloading", error: null },
      uiStatus: "downloading",
      error: null,
    }));
    downloadPromise = updateApi.downloadAppUpdate(offer.offerId, (event: UpdateProgressEvent) => {
      if (useUpdateStore.getState().backendState.offer?.offerId !== offer.offerId) return;
      set((state) => ({
        backendState: {
          ...state.backendState,
          phase: event.phase,
          downloadedBytes: event.downloadedBytes,
          totalBytes: event.totalBytes,
        },
        uiStatus: uiStatusFor({
          ...state.backendState,
          phase: event.phase,
          downloadedBytes: event.downloadedBytes,
          totalBytes: event.totalBytes,
        }, false),
      }));
    }).then((backendState) => {
      set({ backendState, uiStatus: uiStatusFor(backendState, false), error: null });
      return backendState;
    }).catch((error) => {
      const normalized = normalizedUpdateError(error);
      const cancelled = (error as { code?: string } | null)?.code === "UPDATE_DOWNLOAD_CANCELLED";
      set({ uiStatus: cancelled ? "paused_or_cancelled" : "error", error: cancelled ? null : normalized });
      throw error;
    }).finally(() => {
      downloadPromise = null;
    });
    return downloadPromise;
  },

  cancelDownload: async () => {
    const offer = get().backendState.offer;
    if (!offer) return;
    try {
      const backendState = await updateApi.cancelAppUpdateDownload(offer.offerId);
      if (downloadPromise) await downloadPromise.catch(() => undefined);
      set({ backendState, uiStatus: "paused_or_cancelled", error: null });
    } catch (error) {
      set({ uiStatus: "error", error: normalizedUpdateError(error) });
    }
  },

  retry: async () => {
    const state = get();
    if (state.uiStatus === "paused_or_cancelled") {
      await state.download();
      return;
    }
    const retryDownload = state.error?.code?.startsWith("UPDATE_DOWNLOAD_") === true;
    const checked = await state.checkNow(false);
    if (retryDownload && checked.phase === "available") {
      await get().download();
    }
  },

  ignoreVersion: async () => {
    const offer = get().backendState.offer;
    if (!offer) return;
    try {
      const backendState = await updateApi.dismissAppUpdate(offer.offerId);
      const preferences = await updateApi.getGlobalUpdatePreferences();
      set({ backendState, preferences, uiStatus: "idle", dialogOpen: false, error: null });
    } catch (error) {
      set({ uiStatus: "error", error: normalizedUpdateError(error) });
    }
  },

  savePreferences: async (preferences) => {
    set({ preferencesSaving: true, error: null });
    try {
      const saved = await updateApi.saveGlobalUpdatePreferences(preferences);
      set({ preferences: saved });
    } catch (error) {
      set({ error: normalizedUpdateError(error) });
      throw error;
    } finally {
      set({ preferencesSaving: false });
    }
  },

  reviewInstall: async (installReviewIntent) => {
    const { collectRuntimeUpdateInstallGuard } = await import(
      "../features/update/installGuardRuntime"
    );
    set({
      installGuard: collectRuntimeUpdateInstallGuard(),
      installReviewIntent,
      error: null,
    });
  },

  confirmInstallOrRestart: () => {
    if (installPromise) return installPromise;
    installPromise = (async () => {
      const offer = get().backendState.offer;
      const intent = get().installReviewIntent;
      if (!offer || !intent) return;
      const { collectRuntimeUpdateInstallGuard } = await import(
        "../features/update/installGuardRuntime"
      );
      const installGuard = collectRuntimeUpdateInstallGuard();
      set({ installGuard });
      if (installGuard.blockers.length > 0) return;
      const request = {
        offerId: offer.offerId,
        restartConsent: true,
        ...installGuard.request,
      };
      set({ uiStatus: "installing", error: null });
      try {
        if (intent === "restart") {
          await updateApi.restartAppAfterUpdate(request);
          return;
        }
        const backendState = await updateApi.installAppUpdate(request);
        set({
          backendState,
          uiStatus: uiStatusFor(backendState, false),
          installGuard: null,
          installReviewIntent: null,
        });
      } catch (error) {
        set({ uiStatus: "error", error: normalizedUpdateError(error) });
      }
    })().finally(() => {
      installPromise = null;
    });
    return installPromise;
  },

  clearInstallReview: () => set({ installGuard: null, installReviewIntent: null }),

  resetForTests: () => {
    initializePromise = null;
    checkPromise = null;
    downloadPromise = null;
    installPromise = null;
    stateRevision = 0;
    set({
      backendState: DEFAULT_STATE,
      preferences: DEFAULT_PREFERENCES,
      currentVersion: "0.1.0",
      uiStatus: "idle",
      initialized: false,
      dialogOpen: false,
      preferencesSaving: false,
      error: null,
      installGuard: null,
      installReviewIntent: null,
    });
  },
}));

export function updateCheckDue(
  preferences: GlobalUpdatePreferences,
  now = Date.now(),
): boolean {
  if (!preferences.checkUpdates || preferences.updateFrequency === "never") return false;
  if (!preferences.lastCheckedAt) return true;
  const checkedAt = Date.parse(preferences.lastCheckedAt);
  if (!Number.isFinite(checkedAt)) return true;
  const interval = preferences.updateFrequency === "weekly"
    ? 7 * 24 * 60 * 60 * 1000
    : 24 * 60 * 60 * 1000;
  return now - checkedAt >= interval;
}
