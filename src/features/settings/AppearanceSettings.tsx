import { useId } from "react";
import { useTranslation } from "react-i18next";
import { Check } from "lucide-react";
import type { ColorThemePresetId, ThemePreference } from "../../types/settings";
import { COLOR_THEME_PRESETS } from "../../lib/colorThemePresets";

interface AppearanceSettingsProps {
  theme: ThemePreference;
  colorThemePreset: ColorThemePresetId;
  onChange: (theme: ThemePreference) => void;
  onChangeColorThemePreset: (preset: ColorThemePresetId) => void;
}

export function AppearanceSettings({ theme, colorThemePreset, onChange, onChangeColorThemePreset }: AppearanceSettingsProps) {
  const { t } = useTranslation();
  const group = useId();
  return <section className="settings-section-panel">
    <div id="settings-theme" tabIndex={-1}><h2 className="settings-view__section-title">{t("settings.appearance.title")}</h2><p className="settings-view__section-desc">{t("settings.appearance.description")}</p></div>
    <div className="settings-theme-choices" role="radiogroup" aria-label={t("settings.appearance.mode")}>
      {(["light", "dark", "auto"] as const).map((mode) => <label key={mode} className={`settings-theme-choice${theme === mode ? " is-selected" : ""}`}>
        <input type="radio" name={`${group}-mode`} aria-label={t(`theme.${mode}`)} checked={theme === mode} onChange={() => onChange(mode)} />
        <span className="settings-theme-thumb" data-mode={mode} aria-hidden="true"><span /><i /><i /></span>
        <span className="settings-theme-choice__label">{t(`theme.${mode}`)}{theme === mode ? <Check size={13} aria-hidden="true" /> : null}</span>
      </label>)}
    </div>
    <div id="settings-color-theme" tabIndex={-1} className="settings-color-section"><div><h3>{t("settings.appearance.colorTheme")}</h3><p>{t("settings.appearance.colorDescription")}</p></div>
      <div className="appearance-presets" role="radiogroup" aria-label={t("settings.appearance.colorTheme")}>
        {COLOR_THEME_PRESETS.map((preset) => <label key={preset.id} className={`appearance-preset ${colorThemePreset === preset.id ? "is-selected" : ""}`}>
          <input type="radio" name={`${group}-preset`} aria-label={t(preset.labelKey)} aria-checked={colorThemePreset === preset.id} checked={colorThemePreset === preset.id} onChange={() => onChangeColorThemePreset(preset.id)} />
          <span className="appearance-preset__copy"><span className="appearance-preset__name">{t(preset.labelKey)}</span><span className="appearance-preset__description">{t(preset.descriptionKey)}</span></span>
          <span className="appearance-preset__swatches" aria-hidden="true">{preset.swatches.map((swatch) => <span key={swatch} style={{ background: swatch }} />)}</span>
        </label>)}
      </div>
    </div>
    <div className="appearance-markdown-preview wiki-prose" aria-label={t("settings.appearance.markdownPreview")}><h3>{t("settings.appearance.previewTitle")}</h3><p>{t("settings.appearance.previewParagraph")} <span className="settings-preview-link">{t("settings.appearance.previewLink")}</span></p><pre><code>{'const wiki = "local-first";'}</code></pre></div>
  </section>;
}
