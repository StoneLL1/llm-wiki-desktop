import { invoke } from "@tauri-apps/api/core";
import { X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import { activateLocale, i18next, LANGUAGE_STORAGE_KEY } from "../../i18n";
import { useModalDialog } from "../../hooks/useModalDialog";
import { applyThemePreference } from "../../stores/settingsStore";
import type { ThemePreference } from "../../types/settings";

interface GlobalUiPreferences {
  language: "en" | "zh-CN";
  theme: ThemePreference;
}

const hasTauri = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/**
 * Project-scoped secrets and execution settings deliberately stay unavailable
 * here. These two global preferences remain useful before any project opens.
 */
export function NoProjectSettingsDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const { t } = useTranslation();
  const closeButtonRef = useRef<HTMLButtonElement>(null);
  const dialogRef = useModalDialog<HTMLDivElement>({ open, onClose, initialFocusRef: closeButtonRef });
  const [preferences, setPreferences] = useState<GlobalUiPreferences>({
    language: (i18next.resolvedLanguage ?? i18next.language) === "zh-CN" ? "zh-CN" : "en",
    theme: (document.documentElement.dataset.theme as ThemePreference | undefined) ?? "auto",
  });
  const [error, setError] = useState<string | null>(null);
  const saveEpochRef = useRef(0);

  useEffect(() => {
    if (!open || !hasTauri()) return;
    let active = true;
    void invoke<GlobalUiPreferences>("get_global_ui_preferences").then(
      (saved) => {
        if (active) setPreferences(saved);
      },
      (reason) => {
        if (active) setError(errorMessage(reason));
      },
    );
    return () => {
      active = false;
    };
  }, [open]);

  const save = async (next: GlobalUiPreferences) => {
    const saveEpoch = ++saveEpochRef.current;
    const isCurrent = () => saveEpoch === saveEpochRef.current;
    setError(null);
    try {
      const activated = await activateLocale(next.language, undefined, undefined, isCurrent);
      if (!activated || !isCurrent()) return;
      setPreferences(next);
      applyThemePreference(next.theme);
      try {
        window.localStorage.setItem(LANGUAGE_STORAGE_KEY, next.language);
      } catch {
        // The backend preference remains the source of truth in desktop builds.
      }
      if (!hasTauri()) return;
      const saved = await invoke<GlobalUiPreferences>("save_global_ui_preferences", { preferences: next });
      if (!isCurrent()) return;
      const savedLanguageActivated = await activateLocale(
        saved.language,
        undefined,
        undefined,
        isCurrent,
      );
      if (!savedLanguageActivated || !isCurrent()) return;
      setPreferences(saved);
      applyThemePreference(saved.theme);
      try {
        window.localStorage.setItem(LANGUAGE_STORAGE_KEY, saved.language);
      } catch {
        // The backend preference remains the source of truth in desktop builds.
      }
    } catch (reason) {
      if (isCurrent()) setError(errorMessage(reason));
    }
  };

  if (!open) return null;

  return (
    <div
      ref={dialogRef}
      aria-labelledby="no-project-settings-title"
      aria-modal="true"
      className="dialog-overlay settings-dialog__overlay"
      onClick={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
      role="dialog"
      tabIndex={-1}
    >
      <section className="dialog dialog--settings settings-dialog">
        <header className="dialog__head">
          <div className="min-w-0">
            <h2 id="no-project-settings-title" className="settings-dialog__title">{t("nav.settings")}</h2>
            <p className="settings-dialog__subtitle">{t("settings.general.scopeCopy")}</p>
          </div>
          <button
            ref={closeButtonRef}
            aria-label={t("settings.dialog.close")}
            className="icon-button ml-auto shrink-0"
            onClick={onClose}
            title={t("settings.dialog.close")}
            type="button"
          >
            <X aria-hidden="true" size={16} />
          </button>
        </header>
        <div className="settings-dialog__body p-4">
          <section className="space-y-3" aria-labelledby="no-project-language-title">
            <div>
              <h3 id="no-project-language-title" className="m-0 text-[13px] font-medium">{t("settings.language.title")}</h3>
              <p className="m-0 mt-1 text-[12px] text-[var(--text-muted)]">{t("settings.language.description")}</p>
            </div>
            <div className="seg w-fit" role="group" aria-label={t("settings.language.title")}>
              <button className={preferences.language === "en" ? "is-active" : ""} onClick={() => void save({ ...preferences, language: "en" })} type="button">{t("settings.language.option.en")}</button>
              <button className={preferences.language === "zh-CN" ? "is-active" : ""} onClick={() => void save({ ...preferences, language: "zh-CN" })} type="button">{t("settings.language.option.zh-CN")}</button>
            </div>
          </section>
          <section className="mt-6 space-y-3" aria-labelledby="no-project-appearance-title">
            <div>
              <h3 id="no-project-appearance-title" className="m-0 text-[13px] font-medium">{t("settings.appearance.title")}</h3>
              <p className="m-0 mt-1 text-[12px] text-[var(--text-muted)]">{t("settings.appearance.description")}</p>
            </div>
            <div className="seg w-fit" role="group" aria-label={t("settings.appearance.title")}>
              {(["light", "dark", "auto"] as const).map((theme) => (
                <button className={preferences.theme === theme ? "is-active" : ""} key={theme} onClick={() => void save({ ...preferences, theme })} type="button">
                  {t(`settings.appearance.${theme}`)}
                </button>
              ))}
            </div>
          </section>
          {error ? <p className="mt-5 text-[12px] text-[var(--danger)]" role="alert">{error}</p> : null}
        </div>
      </section>
    </div>
  );
}
