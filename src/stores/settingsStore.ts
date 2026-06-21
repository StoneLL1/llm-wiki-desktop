import { invoke } from "@tauri-apps/api/core";
import { create } from "zustand";

import { i18next, LANGUAGE_STORAGE_KEY } from "../i18n";
import type { Settings, ThemePreference } from "../types/settings";
import { captureProjectScope, isProjectScopeCurrent } from "./projectScope";

const THEME_STORAGE_KEY = "llm-wiki-desktop.theme";

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
  closeBehavior: "minimize_to_tray",
  contextWindow: 32_000,
  checkUpdates: true,
  agentDefault: null,
  llmProviders: [],
  template: "general",
};

export function applyThemePreference(theme: ThemePreference) {
  if (typeof document === "undefined") return;
  document.documentElement.dataset.theme = theme;
  try {
    window.localStorage.setItem(THEME_STORAGE_KEY, theme);
  } catch {
    // Ignore localStorage errors in restricted environments.
  }
}

async function applyLanguagePreference(language: Settings["language"]) {
  try {
    window.localStorage.setItem(LANGUAGE_STORAGE_KEY, language);
  } catch {
    // Ignore localStorage errors in restricted environments.
  }
  await i18next.changeLanguage(language);
}

export interface SettingsState {
  settings: Settings;
  loading: boolean;
  saving: boolean;
  loadedProjectKey: string | null;
  error: string | null;
  loadSettings: (projectId: string, projectRootPath: string) => Promise<Settings>;
  persistPatch: (
    projectId: string,
    projectRootPath: string,
    patch: Partial<Settings>,
  ) => Promise<Settings>;
  reset: () => void;
}

export const useSettingsStore = create<SettingsState>((set, get) => ({
  settings: defaultSettings,
  loading: false,
  saving: false,
  loadedProjectKey: null,
  error: null,

  loadSettings: async (projectId, projectRootPath) => {
    const scope = captureProjectScope();
    const projectKey = `${projectId}:${projectRootPath}`;
    set({ loading: true, error: null });
    try {
      const settings = hasTauri()
        ? await invoke<Settings>("get_settings", {
            request: { projectId, projectRootPath },
          })
        : defaultSettings;
      if (!isProjectScopeCurrent(scope)) return get().settings;
      applyThemePreference(settings.theme);
      await applyLanguagePreference(settings.language);
      if (!isProjectScopeCurrent(scope)) return get().settings;
      set({
        settings,
        loading: false,
        loadedProjectKey: projectKey,
      });
      return settings;
    } catch (error) {
      if (!isProjectScopeCurrent(scope)) return get().settings;
      set({ loading: false, error: errorMessage(error) });
      return get().settings;
    }
  },

  persistPatch: async (projectId, projectRootPath, patch) => {
    const scope = captureProjectScope();
    const previous = get().settings;
    const next = { ...previous, ...patch };
    set({ settings: next, saving: true, error: null });
    applyThemePreference(next.theme);
    await applyLanguagePreference(next.language);

    if (!hasTauri()) {
      set({ saving: false });
      return next;
    }

    try {
      const saved = await invoke<Settings>("save_settings", {
        request: { projectId, projectRootPath, settings: next },
      });
      if (!isProjectScopeCurrent(scope)) return get().settings;
      applyThemePreference(saved.theme);
      await applyLanguagePreference(saved.language);
      if (!isProjectScopeCurrent(scope)) return get().settings;
      set({
        settings: saved,
        saving: false,
        loadedProjectKey: `${projectId}:${projectRootPath}`,
      });
      return saved;
    } catch (error) {
      if (!isProjectScopeCurrent(scope)) return get().settings;
      set({ settings: previous, saving: false, error: errorMessage(error) });
      applyThemePreference(previous.theme);
      await applyLanguagePreference(previous.language);
      return previous;
    }
  },

  reset: () =>
    set((state) => ({
      settings: {
        ...state.settings,
        agentDefault: null,
        llmProviders: [],
        template: "general",
      },
      loading: false,
      saving: false,
      loadedProjectKey: null,
      error: null,
    })),
}));
