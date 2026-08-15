import i18next from "i18next";
import { initReactI18next } from "react-i18next";

import type { Settings } from "../types/settings";

export const LANGUAGE_STORAGE_KEY = "llm-wiki-desktop.language";

type AppLanguage = Settings["language"];
type LocaleResources = Record<string, unknown>;
type LocaleModule = { default: LocaleResources };
type LocaleImporters = Record<AppLanguage, () => Promise<LocaleModule>>;

const localeImporters: LocaleImporters = {
  en: () => import("./locales/en.json"),
  "zh-CN": () => import("./locales/zh-CN.json"),
};

const localeBackend = {
  type: "backend" as const,
  init: () => undefined,
  read: (
    language: string,
    _namespace: string,
    callback: (error: Error | null, resources?: LocaleResources) => void,
  ) => {
    const normalized = language === "zh-CN" ? "zh-CN" : language === "en" ? "en" : null;
    if (!normalized) {
      callback(new Error(`Unsupported locale: ${language}`));
      return;
    }
    void localeImporters[normalized]().then(
      ({ default: resources }) => callback(null, resources),
      (error: unknown) => callback(error instanceof Error ? error : new Error(String(error))),
    );
  },
};

interface LocaleResourceTarget {
  addResourceBundle: (
    language: AppLanguage,
    namespace: string,
    resources: LocaleResources,
    deep: boolean,
    overwrite: boolean,
  ) => void;
}

interface LanguageTarget {
  changeLanguage: (language: AppLanguage) => Promise<unknown>;
}

export function createLocaleResourceLoader(
  importers: LocaleImporters,
  target: LocaleResourceTarget,
) {
  const loaded = new Map<AppLanguage, Promise<void>>();

  return (language: AppLanguage): Promise<void> => {
    const existing = loaded.get(language);
    if (existing) return existing;

    const request = importers[language]().then(({ default: resources }) => {
      target.addResourceBundle(language, "translation", resources, true, true);
    });
    loaded.set(language, request);
    request.catch(() => loaded.delete(language));
    return request;
  };
}

async function detectInitialLanguage(): Promise<AppLanguage> {
  try {
    const language = window.localStorage.getItem(LANGUAGE_STORAGE_KEY);
    return language === "zh-CN" ? "zh-CN" : "en";
  } catch {
    return "en";
  }
}

const loadLocaleResource = createLocaleResourceLoader(localeImporters, i18next);

let localeActivationEpoch = 0;

export async function activateLocale(
  language: AppLanguage,
  loadLocale: (language: AppLanguage) => Promise<void> = loadLocaleResource,
  target: LanguageTarget = i18next,
  isCurrent: () => boolean = () => true,
): Promise<boolean> {
  const activationEpoch = ++localeActivationEpoch;
  await loadLocale(language);
  if (activationEpoch !== localeActivationEpoch || !isCurrent()) return false;
  await target.changeLanguage(language);
  return activationEpoch === localeActivationEpoch && isCurrent();
}

export const i18nReady = (async () => {
  const language = await detectInitialLanguage();
  await i18next.use(localeBackend).use(initReactI18next).init({
    lng: language,
    fallbackLng: false,
    interpolation: {
      escapeValue: false,
    },
  });
})();

export { i18next };
