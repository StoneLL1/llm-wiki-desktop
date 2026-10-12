import { useId } from "react";
import { useTranslation } from "react-i18next";
import type { AppLanguage } from "../../types/settings";

export function LanguageSettings({ language, onChange }: { language: AppLanguage; onChange: (language: AppLanguage) => void }) {
  const { t } = useTranslation();
  const name = useId();
  return <div id="settings-language" tabIndex={-1} className="settings-info-row">
    <div><strong>{t("settings.language.title")}</strong><p>{t("settings.language.description")}</p></div>
    <div className="settings-segmented" role="radiogroup" aria-label={t("settings.language.title")}>
      {(["en", "zh-CN"] as const).map((option) => <label key={option} className={language === option ? "is-selected" : ""}><input type="radio" name={name} checked={language === option} onChange={() => onChange(option)} /><span>{t(`settings.language.option.${option}`)}</span></label>)}
    </div>
  </div>;
}
