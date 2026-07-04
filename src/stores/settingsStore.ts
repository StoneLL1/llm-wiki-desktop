import { invoke } from "@tauri-apps/api/core";
import { create } from "zustand";

import { i18next, LANGUAGE_STORAGE_KEY } from "../i18n";
import type { Settings, ThemePreference } from "../types/settings";
import { captureProjectScope, isProjectScopeCurrent } from "./projectScope";

const THEME_STORAGE_KEY = "llm-wiki-desktop.theme";
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
      applyDensityPreference(settings.density);
      applyFontPreference({
        ui: settings.uiFont,
        reading: settings.readingFont,
        code: settings.codeFont,
      });
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
    applyDensityPreference(next.density);
    applyFontPreference({
      ui: next.uiFont,
      reading: next.readingFont,
      code: next.codeFont,
    });
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
      applyDensityPreference(saved.density);
      applyFontPreference({
        ui: saved.uiFont,
        reading: saved.readingFont,
        code: saved.codeFont,
      });
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
      applyDensityPreference(previous.density);
      applyFontPreference({
        ui: previous.uiFont,
        reading: previous.readingFont,
        code: previous.codeFont,
      });
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
