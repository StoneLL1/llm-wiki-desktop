import { invoke } from "@tauri-apps/api/core";
import { create } from "zustand";
import { registerProjectScopeResetHandler } from "./projectScopeResetRegistry";

import { activateLocale, LANGUAGE_STORAGE_KEY } from "../i18n";
import { applyColorThemePresetToRoot } from "../lib/colorThemePresets";
import { createProjectResourceController } from "../lib/projectResourceFreshness";
import type { ChatConvenienceAuthorization, GlobalPreferences, GlobalPreferencesPatch, Settings, ThemePreference } from "../types/settings";
import {
  captureProjectScope,
  isProjectScopeCurrent,
  registerProjectResource,
} from "./projectScope";

const THEME_STORAGE_KEY = "llm-wiki-desktop.theme";
const COLOR_THEME_STORAGE_KEY = "llm-wiki-desktop.colorThemePreset";
const DENSITY_STORAGE_KEY = "llm-wiki-desktop.density";
const FONT_STORAGE_KEY = "llm-wiki-desktop.fonts";

const hasTauri = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

function errorMessage(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error) {
    const message = (error as { message: unknown }).message;
    if (typeof message === "string") return message;
  }
  return String(error);
}

export const defaultSettings: Settings = {
  language: "en",
  theme: "auto",
  colorThemePreset: "codex",
  density: "standard",
  uiFont: "",
  readingFont: "",
  codeFont: "",
  agentOutputLanguage: "follow_ui",
  closeBehavior: "minimize_to_tray",
  systemNotifications: {
    onTaskCompleted: true,
    onTaskFailed: true,
    onConfirmationNeeded: true,
    onLongTaskProgress: false,
  },
  notificationClickBehavior: "result_page",
  maxConcurrentTasks: 2,
  checkUpdates: true,
  updateFrequency: "daily",
  autoDownloadUpdates: false,
  promptChangelogBeforeInstall: true,
  startupBehavior: "open_last_project",
  defaultProjectLocation: "",
  externalEditor: "",
  associateMdFiles: true,
  associateWikiFolders: false,
  contextWindow: 32_000,
  maxTokens: 4096,
  temperature: 0.3,
  agentTaskTimeoutSecs: 300,
  allowAgentInstall: false,
  installCommandDisplayOnly: true,
  promptOnNewAgent: true,
  skillAutoload: true,
  autoGitCheckpoint: true,
  manualEditProtection: true,
  rawSourcesImmutable: true,
  agentDefault: null,
  aiDefaultRoute: null,
  llmProviders: [],
  template: "general",
};

const globalPreferenceKeys = [
  "language", "theme", "colorThemePreset", "density", "uiFont", "readingFont", "codeFont",
  "agentOutputLanguage", "closeBehavior", "systemNotifications", "notificationClickBehavior",
  "maxConcurrentTasks", "startupBehavior", "defaultProjectLocation", "externalEditor",
  "associateMdFiles", "associateWikiFolders",
] as const satisfies readonly (keyof GlobalPreferences)[];

function pickGlobalPreferences(settings: Settings): GlobalPreferences {
  return Object.fromEntries(globalPreferenceKeys.map((key) => [key, settings[key]])) as unknown as GlobalPreferences;
}

function mergeGlobalPatch(settings: GlobalPreferences, patch: GlobalPreferencesPatch): GlobalPreferences {
  return {
    ...settings,
    ...patch,
    systemNotifications: { ...settings.systemNotifications, ...patch.systemNotifications },
  };
}

function applyVisualPreferences(settings: GlobalPreferences) {
  applyThemePreference(settings.theme);
  applyColorThemePresetPreference(settings.colorThemePreset, settings.theme);
  applyDensityPreference(settings.density);
  applyFontPreference({ ui: settings.uiFont, reading: settings.readingFont, code: settings.codeFont });
}

let activeColorThemePreference: Pick<Settings, "colorThemePreset" | "theme"> = {
  colorThemePreset: defaultSettings.colorThemePreset,
  theme: defaultSettings.theme,
};
let colorSchemeQueryList: MediaQueryList | null = null;
let colorSchemeQueryListener: ((event: MediaQueryListEvent) => void) | null = null;

function addColorSchemeChangeListener(
  mediaQuery: MediaQueryList,
  listener: (event: MediaQueryListEvent) => void,
): boolean {
  if (typeof mediaQuery.addEventListener === "function") {
    mediaQuery.addEventListener("change", listener);
    return true;
  }
  if (typeof mediaQuery.addListener === "function") {
    mediaQuery.addListener(listener);
    return true;
  }
  return false;
}

function removeColorSchemeChangeListener(
  mediaQuery: MediaQueryList,
  listener: (event: MediaQueryListEvent) => void,
) {
  if (typeof mediaQuery.removeEventListener === "function") {
    mediaQuery.removeEventListener("change", listener);
    return;
  }
  if (typeof mediaQuery.removeListener === "function") {
    mediaQuery.removeListener(listener);
  }
}

export function applyThemePreference(theme: ThemePreference) {
  if (typeof document === "undefined") return;
  document.documentElement.dataset.theme = theme;
  try {
    window.localStorage.setItem(THEME_STORAGE_KEY, theme);
  } catch {
    // Ignore localStorage errors in restricted environments.
  }
}

export function applyColorThemePresetPreference(
  colorThemePreset: Settings["colorThemePreset"],
  theme: ThemePreference,
) {
  if (typeof document === "undefined") return;
  activeColorThemePreference = { colorThemePreset, theme };
  const mediaQuery =
    typeof window !== "undefined" && typeof window.matchMedia === "function"
      ? window.matchMedia("(prefers-color-scheme: dark)")
      : null;
  applyColorThemePresetToRoot(
    colorThemePreset,
    theme,
    document.documentElement,
    mediaQuery?.matches ?? false,
  );
  syncColorSchemePreferenceListener(theme, mediaQuery);
  try {
    window.localStorage.setItem(COLOR_THEME_STORAGE_KEY, colorThemePreset);
  } catch {
    // Ignore localStorage errors in restricted environments.
  }
}

function syncColorSchemePreferenceListener(theme: ThemePreference, mediaQuery: MediaQueryList | null) {
  if (colorSchemeQueryList && colorSchemeQueryListener) {
    removeColorSchemeChangeListener(colorSchemeQueryList, colorSchemeQueryListener);
    colorSchemeQueryList = null;
    colorSchemeQueryListener = null;
  }
  if (theme !== "auto" || !mediaQuery) return;
  colorSchemeQueryListener = (event) => {
    if (typeof document === "undefined") return;
    applyColorThemePresetToRoot(
      activeColorThemePreference.colorThemePreset,
      activeColorThemePreference.theme,
      document.documentElement,
      event.matches,
    );
  };
  if (addColorSchemeChangeListener(mediaQuery, colorSchemeQueryListener)) {
    colorSchemeQueryList = mediaQuery;
  } else {
    colorSchemeQueryListener = null;
  }
}

interface FontPreference {
  ui: string;
  reading: string;
  code: string;
}

const FONT_FALLBACKS: Record<keyof FontPreference, string> = {
  ui: 'Inter, -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif',
  reading: '"Source Serif Pro", "Iowan Old Style", Georgia, serif',
  code: "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace",
};

function disabledChatConvenienceAuthorization(projectId: string): ChatConvenienceAuthorization {
  return {
    enabled: false,
    confirmedAt: "",
    projectId,
    rootPathFingerprint: "",
  };
}

export function applyDensityPreference(density: Settings["density"]) {
  if (typeof document === "undefined") return;
  document.documentElement.dataset.density = density;
  try {
    window.localStorage.setItem(DENSITY_STORAGE_KEY, density);
  } catch {
    // Ignore localStorage errors in restricted environments.
  }
}

export function applyFontPreference(fonts: FontPreference) {
  if (typeof document === "undefined") return;
  const root = document.documentElement;
  if (fonts.ui) {
    root.style.setProperty("--font-ui", `${fonts.ui}, ${FONT_FALLBACKS.ui}`);
  } else {
    root.style.removeProperty("--font-ui");
  }
  if (fonts.reading) {
    root.style.setProperty("--font-display", `${fonts.reading}, ${FONT_FALLBACKS.reading}`);
  } else {
    root.style.removeProperty("--font-display");
  }
  if (fonts.code) {
    root.style.setProperty("--font-mono", `${fonts.code}, ${FONT_FALLBACKS.code}`);
  } else {
    root.style.removeProperty("--font-mono");
  }
  try {
    window.localStorage.setItem(FONT_STORAGE_KEY, JSON.stringify(fonts));
  } catch {
    // Ignore localStorage errors in restricted environments.
  }
}

async function applyLanguagePreference(
  language: Settings["language"],
  isCurrent: () => boolean = () => true,
): Promise<boolean> {
  const activated = await activateLocale(language, undefined, undefined, isCurrent);
  if (!activated || !isCurrent()) return false;
  try {
    window.localStorage.setItem(LANGUAGE_STORAGE_KEY, language);
  } catch {
    // Ignore localStorage errors in restricted environments.
  }
  return true;
}

let languagePreferenceEpoch = 0;
let settingsLoadEpoch = 0;
let settingsSaveEpoch = 0;
let globalLoadEpoch = 0;
let globalSaveEpoch = 0;
let globalSaveQueue: Promise<unknown> = Promise.resolve();
let projectSaveQueue: Promise<unknown> = Promise.resolve();
let committedGlobalPreferences: GlobalPreferences | null = null;
const pendingGlobalPatches = new Map<number, GlobalPreferencesPatch>();

function visibleGlobalPreferences(): GlobalPreferences {
  let preferences = committedGlobalPreferences ?? pickGlobalPreferences(defaultSettings);
  for (const patch of pendingGlobalPatches.values()) {
    preferences = mergeGlobalPatch(preferences, patch);
  }
  return preferences;
}

export interface SettingsState {
  settings: Settings;
  chatConvenienceAuthorization: ChatConvenienceAuthorization | null;
  chatConvenienceSaving: boolean;
  loading: boolean;
  saving: boolean;
  globalLoading: boolean;
  globalSaving: boolean;
  globalLoaded: boolean;
  globalError: string | null;
  loadGlobalSettings: () => Promise<Settings>;
  persistGlobalPatch: (patch: GlobalPreferencesPatch) => Promise<Settings>;
  loadedProjectKey: string | null;
  error: string | null;
  loadSettings: (projectId: string, projectRootPath: string) => Promise<Settings>;
  loadChatConvenienceAuthorization: (
    projectId: string,
    projectRootPath: string,
  ) => Promise<ChatConvenienceAuthorization>;
  ensureChatConvenienceAuthorization: (
    projectId: string,
    projectRootPath: string,
  ) => Promise<ChatConvenienceAuthorization>;
  setChatConvenienceAuthorization: (
    projectId: string,
    projectRootPath: string,
    enabled: boolean,
  ) => Promise<ChatConvenienceAuthorization>;
  revokeAllChatConvenienceAuthorizations: () => Promise<void>;
  persistPatch: (
    projectId: string,
    projectRootPath: string,
    patch: Partial<Settings>,
  ) => Promise<Settings>;
  reset: () => void;
}

const chatAuthorizationResource = createProjectResourceController<ChatConvenienceAuthorization>(
  "settings-chat-authorization",
);

export const useSettingsStore = create<SettingsState>((set, get) => ({
  settings: defaultSettings,
  chatConvenienceAuthorization: null,
  chatConvenienceSaving: false,
  loading: false,
  saving: false,
  loadedProjectKey: null,
  error: null,
  globalLoading: false,
  globalSaving: false,
  globalLoaded: false,
  globalError: null,

  loadGlobalSettings: async () => {
    const epoch = ++globalLoadEpoch;
    const saveEpoch = globalSaveEpoch;
    const isCurrent = () => epoch === globalLoadEpoch && saveEpoch === globalSaveEpoch;
    set({ globalLoading: true, globalError: null });
    try {
      // Read after already queued writes so a refresh cannot publish the
      // pre-save file contents after the final pending layer disappears.
      await globalSaveQueue;
      if (!isCurrent()) return get().settings;
      const preferences = hasTauri()
        ? await invoke<GlobalPreferences>("get_global_preferences")
        : pickGlobalPreferences(get().settings);
      if (!isCurrent()) return get().settings;
      committedGlobalPreferences = preferences;
      const visible = visibleGlobalPreferences();
      const languageEpoch = ++languagePreferenceEpoch;
      await applyLanguagePreference(visible.language, () => isCurrent() && languageEpoch === languagePreferenceEpoch);
      if (!isCurrent()) return get().settings;
      applyVisualPreferences(visible);
      set((state) => ({ settings: { ...state.settings, ...visible }, globalLoaded: true }));
      return get().settings;
    } catch (error) {
      if (isCurrent()) set({ globalError: errorMessage(error) });
      return get().settings;
    } finally {
      if (epoch === globalLoadEpoch) set({ globalLoading: false });
    }
  },

  persistGlobalPatch: async (patch) => {
    const epoch = ++globalSaveEpoch;
    globalLoadEpoch += 1;
    if (pendingGlobalPatches.size === 0) committedGlobalPreferences = pickGlobalPreferences(get().settings);
    pendingGlobalPatches.set(epoch, patch);
    const visible = visibleGlobalPreferences();
    const languageEpoch = ++languagePreferenceEpoch;
    set((state) => ({
      settings: { ...state.settings, ...visible },
      globalLoading: false,
      globalSaving: true,
      globalError: null,
    }));
    applyVisualPreferences(visible);
    // Locale loading is presentation work, not a prerequisite to persisting.
    // A slow language chunk must not block a later user choice or disk write.
    const localeReady = applyLanguagePreference(visible.language, () => languageEpoch === languagePreferenceEpoch)
      .catch((error: unknown) => {
        if (languageEpoch === languagePreferenceEpoch) set({ globalError: errorMessage(error) });
        throw error;
      });
    const saving = globalSaveQueue.then(async () => {
      try {
        const saved = hasTauri()
          ? await invoke<GlobalPreferences>("patch_global_preferences", { patch })
          : mergeGlobalPatch(committedGlobalPreferences!, patch);
        committedGlobalPreferences = saved;
        pendingGlobalPatches.delete(epoch);
        const next = visibleGlobalPreferences();
        set((state) => ({
          settings: { ...state.settings, ...next },
          globalSaving: pendingGlobalPatches.size > 0,
          globalLoaded: true,
        }));
        applyVisualPreferences(next);
        return get().settings;
      } catch (error) {
        pendingGlobalPatches.delete(epoch);
        const previousLanguage = get().settings.language;
        const next = visibleGlobalPreferences();
        set((state) => ({
          settings: { ...state.settings, ...next },
          globalSaving: pendingGlobalPatches.size > 0,
          globalError: errorMessage(error),
        }));
        applyVisualPreferences(next);
        if (next.language !== previousLanguage) {
          const rollbackEpoch = ++languagePreferenceEpoch;
          void applyLanguagePreference(next.language, () => rollbackEpoch === languagePreferenceEpoch)
            .catch(() => undefined);
        }
        throw error;
      }
    });
    // A rejected save does not poison the queue. Its caller still receives rejection.
    globalSaveQueue = saving.catch(() => undefined);
    const [saved] = await Promise.all([saving, localeReady]);
    return saved;
  },

  loadSettings: async (projectId, projectRootPath) => {
    if (get().saving) return get().settings;
    const scope = captureProjectScope();
    const loadEpoch = ++settingsLoadEpoch;
    const saveEpochAtStart = settingsSaveEpoch;
    const isCurrentLoad = () =>
      isProjectScopeCurrent(scope)
      && loadEpoch === settingsLoadEpoch
      && saveEpochAtStart === settingsSaveEpoch;
    const projectKey = `${projectId}:${projectRootPath}`;
    set({ loading: true, error: null });
    try {
      const loaded = hasTauri()
        ? await invoke<Settings>("get_settings", {
            request: { projectId, projectRootPath },
          })
        : defaultSettings;
      if (!isCurrentLoad()) return get().settings;
      const globals = get().globalLoaded || get().globalSaving
        ? pickGlobalPreferences(get().settings)
        : pickGlobalPreferences(loaded);
      const settings = { ...defaultSettings, ...loaded, ...globals };
      if (!get().globalSaving) committedGlobalPreferences = globals;
      const globalEpoch = globalSaveEpoch;
      const isCurrentPresentation = () => isCurrentLoad() && globalEpoch === globalSaveEpoch;
      await applyLanguagePreference(settings.language, isCurrentPresentation);
      if (!isCurrentLoad()) return get().settings;
      const next = isCurrentPresentation()
        ? settings
        : { ...settings, ...pickGlobalPreferences(get().settings) };
      applyVisualPreferences(next);
      set({
        settings: next,
        loading: false,
        loadedProjectKey: projectKey,
        globalLoaded: true,
      });
      return next;
    } catch (error) {
      if (!isCurrentLoad()) return get().settings;
      set({ loading: false, error: errorMessage(error) });
      return get().settings;
    }
  },

  loadChatConvenienceAuthorization: async (projectId, projectRootPath) => {
    const scope = captureProjectScope();
    const requestEpoch = chatAuthorizationResource.beginRequest();
    try {
      const authorization = hasTauri()
        ? await invoke<ChatConvenienceAuthorization>("get_chat_convenience_authorization", {
            request: { projectId, projectRootPath },
          })
        : disabledChatConvenienceAuthorization(projectId);
      if (!isProjectScopeCurrent(scope) || !chatAuthorizationResource.isCurrent(requestEpoch)) {
        return disabledChatConvenienceAuthorization(projectId);
      }
      chatAuthorizationResource.markLoaded(projectId, projectRootPath);
      set({
        chatConvenienceAuthorization: authorization,
        error: null,
      });
      return authorization;
    } catch (error) {
      const current = get().chatConvenienceAuthorization;
      const fallback = current?.projectId === projectId
        ? current
        : disabledChatConvenienceAuthorization(projectId);
      if (isProjectScopeCurrent(scope) && chatAuthorizationResource.isCurrent(requestEpoch)) {
        set({ chatConvenienceAuthorization: fallback, error: errorMessage(error) });
      }
      return fallback;
    }
  },

  ensureChatConvenienceAuthorization: (projectId, projectRootPath) => {
    const state = get();
    if (state.chatConvenienceSaving) {
      return Promise.resolve(
        state.chatConvenienceAuthorization ?? disabledChatConvenienceAuthorization(projectId),
      );
    }
    return chatAuthorizationResource.ensure(
      { projectId, rootPath: projectRootPath },
      () => get().loadChatConvenienceAuthorization(projectId, projectRootPath),
      get().chatConvenienceAuthorization ?? disabledChatConvenienceAuthorization(projectId),
    );
  },

  setChatConvenienceAuthorization: async (projectId, projectRootPath, enabled) => {
    const requestEpoch = chatAuthorizationResource.beginRequest();
    const scope = captureProjectScope();
    set({ chatConvenienceSaving: true, error: null });
    try {
      const authorization = hasTauri()
        ? await invoke<ChatConvenienceAuthorization>("set_chat_convenience_authorization", {
            request: { projectId, projectRootPath, enabled },
          })
        : enabled
          ? {
              enabled: true,
              confirmedAt: new Date().toISOString(),
              projectId,
              rootPathFingerprint: "",
            }
          : disabledChatConvenienceAuthorization(projectId);
      if (isProjectScopeCurrent(scope) && chatAuthorizationResource.isCurrent(requestEpoch)) {
        chatAuthorizationResource.markLoaded(projectId, projectRootPath);
        set({
          chatConvenienceAuthorization: authorization,
          error: null,
        });
      }
      return authorization;
    } catch (error) {
      const current = get().chatConvenienceAuthorization;
      const fallback = current?.projectId === projectId
        ? current
        : disabledChatConvenienceAuthorization(projectId);
      if (isProjectScopeCurrent(scope) && chatAuthorizationResource.isCurrent(requestEpoch)) {
        set({ chatConvenienceAuthorization: fallback, error: errorMessage(error) });
      }
      throw error;
    } finally {
      if (isProjectScopeCurrent(scope) && chatAuthorizationResource.isCurrent(requestEpoch)) {
        set({ chatConvenienceSaving: false });
      }
    }
  },

  revokeAllChatConvenienceAuthorizations: async () => {
    try {
      if (hasTauri()) {
        await invoke("revoke_all_chat_convenience_authorizations");
      }
      set({ chatConvenienceAuthorization: null, chatConvenienceSaving: false, error: null });
      chatAuthorizationResource.reset();
    } catch (error) {
      set({ error: errorMessage(error) });
      throw error;
    }
  },

  persistPatch: async (projectId, projectRootPath, patch) => {
    const globalPatch: GlobalPreferencesPatch = {};
    const projectPatch = { ...patch };
    for (const key of globalPreferenceKeys) {
      if (key in patch) {
        Object.assign(globalPatch, { [key]: patch[key] });
        delete projectPatch[key];
      }
    }
    const globalSaving = Object.keys(globalPatch).length > 0
      ? get().persistGlobalPatch(globalPatch)
      : Promise.resolve(get().settings);
    if (Object.keys(projectPatch).length === 0) return globalSaving;

    const scope = captureProjectScope();
    const saveEpoch = ++settingsSaveEpoch;
    const isCurrentRequest = () => isProjectScopeCurrent(scope) && saveEpoch === settingsSaveEpoch;
    set({ loading: false, saving: true, error: null });
    const saving = projectSaveQueue.then(async () => {
      // Read the committed state at dispatch time so queued patches do not
      // carry failed optimistic project values into later saves.
      if (!isProjectScopeCurrent(scope)) return get().settings;
      const next = { ...get().settings, ...projectPatch };
      try {
        const saved = hasTauri()
          ? await invoke<Settings>("save_project_settings", {
              request: { projectId, projectRootPath, settings: next },
            })
          : next;
        if (isProjectScopeCurrent(scope)) {
          set((state) => ({
            settings: { ...saved, ...pickGlobalPreferences(state.settings) },
            saving: !isCurrentRequest(),
            loadedProjectKey: `${projectId}:${projectRootPath}`,
          }));
        }
        return get().settings;
      } catch (error) {
        if (isProjectScopeCurrent(scope)) {
          set({ saving: !isCurrentRequest(), error: errorMessage(error) });
        }
        throw error;
      }
    });
    projectSaveQueue = saving.catch(() => undefined);
    const [saved] = await Promise.all([saving, globalSaving]);
    return saved;
  },

  reset: () => {
    settingsLoadEpoch += 1;
    settingsSaveEpoch += 1;
    chatAuthorizationResource.reset();
    set((state) => ({
      settings: {
        ...defaultSettings,
        ...pickGlobalPreferences(state.settings),
        checkUpdates: state.settings.checkUpdates,
        updateFrequency: state.settings.updateFrequency,
        autoDownloadUpdates: state.settings.autoDownloadUpdates,
        promptChangelogBeforeInstall: state.settings.promptChangelogBeforeInstall,
      },
      loading: false,
      saving: false,
      loadedProjectKey: null,
      chatConvenienceAuthorization: null,
      chatConvenienceSaving: false,
      error: null,
    }));
  },
}));

registerProjectResource("settings-chat-authorization", {
  invalidate: (scope) => {
    const state = useSettingsStore.getState();
    if (!state.chatConvenienceSaving) chatAuthorizationResource.invalidate(scope);
  },
}, ({ projectId, rootPath }) =>
  useSettingsStore.getState().ensureChatConvenienceAuthorization(projectId, rootPath).then(() => undefined));

registerProjectScopeResetHandler("settings", () => useSettingsStore.getState().reset());
