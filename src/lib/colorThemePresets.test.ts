import { describe, expect, it } from "vitest";
import {
  COLOR_THEME_PRESETS,
  getColorThemePreset,
  requiredThemeVars,
  resolveColorThemeVariant,
} from "./colorThemePresets";

describe("color theme presets", () => {
  it("falls back to codex for unknown ids", () => {
    expect(getColorThemePreset("unknown").id).toBe("codex");
  });

  it("ships at least four complete presets", () => {
    expect(COLOR_THEME_PRESETS.length).toBeGreaterThanOrEqual(4);
    for (const preset of COLOR_THEME_PRESETS) {
      for (const token of requiredThemeVars) {
        expect(preset.variants.light.cssVars[token], `${preset.id} light ${token}`).toBeTruthy();
        expect(preset.variants.dark.cssVars[token], `${preset.id} dark ${token}`).toBeTruthy();
      }
    }
  });

  it("uses redesigned reference-inspired accent families with clear separation", () => {
    expect(COLOR_THEME_PRESETS.map((preset) => preset.id)).toEqual([
      "codex",
      "paper",
      "graphite",
      "mint",
      "night",
      "highContrast",
    ]);
    expect(getColorThemePreset("codex").variants.light.cssVars["--accent"]).toBe("#0f766e");
    expect(getColorThemePreset("paper").variants.light.cssVars["--accent"]).toBe("#c2410c");
    expect(getColorThemePreset("graphite").variants.light.cssVars["--accent"]).toBe("#5b5bd6");
    expect(getColorThemePreset("mint").variants.light.cssVars["--accent"]).toBe("#059669");
    expect(getColorThemePreset("night").variants.dark.cssVars["--accent"]).toBe("#8b5cf6");
    expect(getColorThemePreset("highContrast").variants.dark.cssVars["--accent"]).toBe("#7dd3fc");
  });

  it("resolves auto mode from system preference", () => {
    const preset = getColorThemePreset("codex");
    expect(resolveColorThemeVariant(preset, "auto", false).mode).toBe("light");
    expect(resolveColorThemeVariant(preset, "auto", true).mode).toBe("dark");
  });

  it("keeps brightness mode authoritative for every preset", () => {
    const preset = getColorThemePreset("night");
    expect(resolveColorThemeVariant(preset, "light", true).mode).toBe("light");
    expect(resolveColorThemeVariant(preset, "dark", false).mode).toBe("dark");
  });
});
