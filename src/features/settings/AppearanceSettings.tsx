import type { ColorThemePresetId, ThemePreference } from "../../types/settings";
import { useTranslation } from "react-i18next";

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
  colorThemePreset: _colorThemePreset,
  onChange,
  onChangeColorThemePreset: _onChangeColorThemePreset,
}: AppearanceSettingsProps) {
  const { t } = useTranslation();

  return (
    <section className="grid gap-4">
      <div>
        <h2 className="m-0 text-[16px] font-semibold">{t("settings.appearance.title")}</h2>
        <p className="mt-1 text-[12px] text-[var(--text-muted)]">{t("settings.appearance.description")}</p>
      </div>

      <div className="grid gap-3 md:grid-cols-3">
        {previews.map((preview) => {
          const selected = theme === preview.value;
          return (
            <button
              key={preview.value}
              type="button"
              onClick={() => onChange(preview.value)}
              className={`grid gap-3 rounded-[var(--radius-lg)] border p-3 text-left ${
                selected ? "border-[var(--accent)] shadow-[0_0_0_1px_var(--accent)]" : "border-[var(--border)]"
              }`}
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
    </section>
  );
}
