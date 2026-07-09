import type { ColorThemePresetId, ThemePreference } from "../../types/settings";
import { useTranslation } from "react-i18next";

import { COLOR_THEME_PRESETS } from "../../lib/colorThemePresets";

interface AppearanceSettingsProps {
  theme: ThemePreference;
  colorThemePreset: ColorThemePresetId;
  onChange: (theme: ThemePreference) => void;
  onChangeColorThemePreset: (preset: ColorThemePresetId) => void;
}

const previews: Array<{
  value: ThemePreference;
  tone: string;
  surface: string;
  text: string;
}> = [
  { value: "light", tone: "#ffffff", surface: "#f5f5f5", text: "#0d0d0d" },
  { value: "dark", tone: "#0f1113", surface: "#171a1d", text: "#f5f5f5" },
  { value: "auto", tone: "linear-gradient(135deg, #ffffff 0%, #ffffff 50%, #0f1113 50%, #0f1113 100%)", surface: "#d7d7d7", text: "#3c3c3c" },
];

export function AppearanceSettings({
  theme,
  colorThemePreset,
  onChange,
  onChangeColorThemePreset,
}: AppearanceSettingsProps) {
  const { t } = useTranslation();

  return (
    <section className="settings-section-panel">
      <div>
        <h2 className="m-0 text-[16px] font-semibold">{t("settings.appearance.title")}</h2>
        <p className="mt-1 text-[12px] text-[var(--text-muted)]">{t("settings.appearance.description")}</p>
      </div>

      <div className="settings-choice-grid">
        {previews.map((preview) => {
          const selected = theme === preview.value;
          return (
            <button
              key={preview.value}
              type="button"
              onClick={() => onChange(preview.value)}
              className={`settings-choice-card grid gap-3${selected ? " is-selected" : ""}`}
            >
              <div
                className="h-[120px] rounded-[var(--radius-md)] border border-[var(--border-subtle)] p-3"
                style={{ background: preview.tone as string }}
              >
                <div className="h-full rounded-[10px] p-3" style={{ background: preview.surface }}>
                  <div className="h-3 w-16 rounded-full" style={{ background: preview.text, opacity: 0.9 }} />
                  <div className="mt-3 h-2.5 w-full rounded-full" style={{ background: preview.text, opacity: 0.18 }} />
                  <div className="mt-2 h-2.5 w-4/5 rounded-full" style={{ background: preview.text, opacity: 0.18 }} />
                </div>
              </div>
              <div>
                <div className="text-[13px] font-medium">{t(`theme.${preview.value}`)}</div>
                <div className="mt-1 text-[11px] text-[var(--text-muted)]">{t(`settings.appearance.${preview.value}`)}</div>
              </div>
            </button>
          );
        })}
      </div>

      <div className="grid gap-2">
        <div className="text-[12px] font-medium text-[var(--text-secondary)]">{t("settings.appearance.colorTheme")}</div>
        <div className="appearance-presets" role="radiogroup" aria-label={t("settings.appearance.colorTheme")}>
          {COLOR_THEME_PRESETS.map((preset) => {
            const selected = colorThemePreset === preset.id;
            return (
              <button
                key={preset.id}
                aria-checked={selected}
                className={`appearance-preset ${selected ? "is-selected" : ""}`}
                onClick={() => onChangeColorThemePreset(preset.id)}
                role="radio"
                type="button"
              >
                <span className="appearance-preset__copy">
                  <span className="appearance-preset__name">{t(preset.labelKey)}</span>
                  <span className="appearance-preset__description">{t(preset.descriptionKey)}</span>
                </span>
                <span className="appearance-preset__swatches" aria-hidden="true">
                  {preset.swatches.map((swatch) => (
                    <span key={swatch} style={{ background: swatch }} />
                  ))}
                </span>
              </button>
            );
          })}
        </div>
      </div>

      <div className="appearance-markdown-preview wiki-prose" aria-label={t("settings.appearance.markdownPreview")}>
        <h1>{t("settings.appearance.previewTitle")}</h1>
        <p>{t("settings.appearance.previewParagraph")}</p>
        <p>
          <a href="#preview">{t("settings.appearance.previewLink")}</a>
        </p>
        <pre>
          <code>{`const wiki = "local-first";`}</code>
        </pre>
      </div>
    </section>
  );
}
