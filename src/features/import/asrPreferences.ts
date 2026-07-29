import type { ImportAsrProfile } from "../../types/importV2";

export interface AsrPreference {
  profile: ImportAsrProfile;
  language: string | null;
}

const STORAGE_KEY = "llm-wiki-desktop.import.asr-preference.v1";

function isProfile(value: unknown): value is ImportAsrProfile {
  return value === "fast" || value === "balanced" || value === "accurate";
}

export function readAsrPreference(): AsrPreference | null {
  try {
    const value = JSON.parse(window.localStorage.getItem(STORAGE_KEY) ?? "null") as Partial<AsrPreference> | null;
    if (!value || !isProfile(value.profile)) return null;
    const language = typeof value.language === "string" && /^[a-z]{2,3}(?:-[a-z0-9]{2,8})?$/i.test(value.language)
      ? value.language
      : null;
    return { profile: value.profile, language };
  } catch {
    return null;
  }
}

export function writeAsrPreference(value: AsrPreference | null): void {
  try {
    if (value) window.localStorage.setItem(STORAGE_KEY, JSON.stringify(value));
    else window.localStorage.removeItem(STORAGE_KEY);
  } catch {
    // Preference persistence is best-effort; recognition authorization is backend-owned.
  }
}
