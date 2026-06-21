import i18next from "i18next";
import { initReactI18next } from "react-i18next";

import type { Settings } from "../types/settings";
import en from "./locales/en.json";
import zhCN from "./locales/zh-CN.json";

export const LANGUAGE_STORAGE_KEY = "llm-wiki-desktop.language";

async function detectInitialLanguage(): Promise<Settings["language"]> {
  try {
    const language = window.localStorage.getItem(LANGUAGE_STORAGE_KEY);
    return language === "zh-CN" ? "zh-CN" : "en";
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
