import type { AppLanguage } from "../../types/settings";
import { useTranslation } from "react-i18next";

interface LanguageSettingsProps {
  language: AppLanguage;
  onChange: (language: AppLanguage) => void;
}

const options: AppLanguage[] = ["en", "zh-CN"];

export function LanguageSettings({ language, onChange }: LanguageSettingsProps) {
  const { t } = useTranslation();

  return (
    <section className="grid gap-4">
      <div>
        <h2 className="m-0 text-[16px] font-semibold">{t("settings.language.title")}</h2>
        <p className="mt-1 text-[12px] text-[var(--text-muted)]">{t("settings.language.description")}</p>
      </div>

      <div className="grid gap-3 md:grid-cols-2">
        {options.map((option) => {
          const selected = language === option;
          return (
            <button
              key={option}
              type="button"
              onClick={() => onChange(option)}
              className={`rounded-[var(--radius-lg)] border p-4 text-left ${
                selected ? "border-[var(--accent)] bg-[var(--accent-soft)]" : "border-[var(--border)] bg-[var(--surface-raised)]"
              }`}
            >
              <div className="text-[13px] font-medium">{t(`settings.language.option.${option}`)}</div>
              <div className="mt-1 text-[11px] text-[var(--text-muted)]">{t(`settings.language.help.${option}`)}</div>
            </button>
          );
        })}
      </div>
    </section>
  );
}
