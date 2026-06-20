import { invoke } from "@tauri-apps/api/core";
import i18next from "i18next";
import { initReactI18next } from "react-i18next";

import { defaultProject } from "../stores/projectStore";
import type { Settings } from "../types/settings";
import en from "./locales/en.json";
import zhCN from "./locales/zh-CN.json";

const hasTauri = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function detectInitialLanguage(): Promise<Settings["language"]> {
  if (!hasTauri()) {
    return "en";
  }
  try {
    const settings = await invoke<Settings>("get_settings", {
      request: {
        projectId: defaultProject.projectId,
        projectRootPath: defaultProject.rootPath,
      },
    });
    return settings.language;
  } catch {
    return "en";
  }
}

export const i18nReady = (async () => {
  const language = await detectInitialLanguage();
  await i18next.use(initReactI18next).init({
    resources: {
      en: { translation: en },
      "zh-CN": { translation: zhCN },
    },
    lng: language,
    fallbackLng: "en",
    interpolation: {
      escapeValue: false,
    },
  });
})();

export { i18next };
