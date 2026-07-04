import type { ColorThemePresetId, ThemePreference } from "../types/settings";

export const requiredThemeVars = [
  "--background",
  "--foreground",
  "--surface",
  "--surface-raised",
  "--surface-muted",
  "--surface-hover",
  "--border",
  "--border-subtle",
  "--text-primary",
  "--text-secondary",
  "--text-muted",
  "--accent",
  "--accent-hover",
  "--accent-soft",
  "--accent-border",
  "--reading-background",
  "--reading-text",
  "--reading-muted",
  "--reading-link",
  "--reading-code-bg",
  "--reading-border",
] as const;

export type ThemeCssVar = (typeof requiredThemeVars)[number];

export interface ColorThemeVariant {
  mode: "light" | "dark";
  cssVars: Record<ThemeCssVar, string>;
}

export interface ColorThemePreset {
  id: ColorThemePresetId;
  labelKey: string;
  descriptionKey: string;
  swatches: string[];
  variants: {
    light: ColorThemeVariant;
    dark: ColorThemeVariant;
  };
}

const codexLight: Record<ThemeCssVar, string> = {
  "--background": "#ffffff",
  "--foreground": "#0d0d0d",
  "--surface": "#fafafa",
  "--surface-raised": "#ffffff",
  "--surface-muted": "#f5f5f5",
  "--surface-hover": "#f7f7f7",
  "--border": "#e5e5e5",
  "--border-subtle": "#ededed",
  "--text-primary": "#0d0d0d",
  "--text-secondary": "#3c3c3c",
  "--text-muted": "#6e6e6e",
  "--accent": "#10a37f",
  "--accent-hover": "#0a7a5e",
  "--accent-soft": "#e8f5f0",
  "--accent-border": "#b8dccd",
  "--reading-background": "#ffffff",
  "--reading-text": "#3c3c3c",
  "--reading-muted": "#6e6e6e",
  "--reading-link": "#0a7a5e",
  "--reading-code-bg": "#f5f5f5",
  "--reading-border": "#ededed",
};

const codexDark: Record<ThemeCssVar, string> = {
  "--background": "#111315",
  "--foreground": "#f5f7f8",
  "--surface": "#15181b",
  "--surface-raised": "#181c20",
  "--surface-muted": "#20252a",
  "--surface-hover": "#252a30",
  "--border": "#2d3339",
  "--border-subtle": "#252b31",
  "--text-primary": "#f5f7f8",
  "--text-secondary": "#d3d7dc",
  "--text-muted": "#a0a8b0",
  "--accent": "#10a37f",
  "--accent-hover": "#2dd4aa",
  "--accent-soft": "rgba(16, 163, 127, 0.18)",
  "--accent-border": "rgba(16, 163, 127, 0.36)",
  "--reading-background": "#111315",
  "--reading-text": "#d3d7dc",
  "--reading-muted": "#a0a8b0",
  "--reading-link": "#2dd4aa",
  "--reading-code-bg": "#20252a",
  "--reading-border": "#252b31",
};

function variant(mode: "light" | "dark", cssVars: Record<ThemeCssVar, string>): ColorThemeVariant {
  return { mode, cssVars };
}

function preset(
  id: ColorThemePresetId,
  swatches: string[],
  light: Partial<Record<ThemeCssVar, string>>,
  dark: Partial<Record<ThemeCssVar, string>>,
): ColorThemePreset {
  return {
    id,
    labelKey: `themePreset.${id}.name`,
    descriptionKey: `themePreset.${id}.description`,
    swatches,
    variants: {
      light: variant("light", { ...codexLight, ...light }),
      dark: variant("dark", { ...codexDark, ...dark }),
    },
  };
}

export const COLOR_THEME_PRESETS: ColorThemePreset[] = [
  preset("codex", ["#ffffff", "#f5f5f5", "#0d0d0d", "#10a37f"], {}, {}),
  preset(
    "paper",
    ["#fffdf8", "#f3efe4", "#27231d", "#3f7f6b"],
    {
      "--background": "#fffdf8",
      "--surface": "#fbf7ee",
      "--surface-muted": "#f3efe4",
      "--surface-hover": "#f7f1e6",
      "--border": "#ded6c8",
      "--border-subtle": "#ebe3d5",
      "--text-primary": "#27231d",
      "--text-secondary": "#4b4438",
      "--accent": "#3f7f6b",
      "--accent-hover": "#2f6454",
      "--accent-soft": "#e8f1ed",
      "--accent-border": "#b8d2c7",
      "--reading-background": "#fffdf8",
      "--reading-text": "#4b4438",
      "--reading-code-bg": "#f3efe4",
      "--reading-border": "#ebe3d5",
    },
    {},
  ),
  preset(
    "graphite",
    ["#f7f8f8", "#d8dcdf", "#151719", "#4f7f8f"],
    {
      "--surface": "#f6f7f7",
      "--surface-muted": "#eef0f1",
      "--border": "#d8dcdf",
      "--text-primary": "#151719",
      "--accent": "#4f7f8f",
      "--accent-hover": "#38606d",
      "--accent-soft": "#e6eef1",
      "--accent-border": "#b8cbd2",
    },
    {
      "--background": "#121416",
      "--surface": "#181b1e",
      "--surface-raised": "#1c2023",
      "--accent": "#6aa6b8",
      "--accent-hover": "#8bc2d2",
      "--accent-soft": "rgba(106, 166, 184, 0.17)",
      "--accent-border": "rgba(106, 166, 184, 0.36)",
    },
  ),
  preset(
    "mint",
    ["#ffffff", "#edf7f2", "#10201b", "#0f8a6b"],
    {
      "--surface": "#fbfdfb",
      "--surface-muted": "#edf7f2",
      "--surface-hover": "#f2faf6",
      "--accent": "#0f8a6b",
      "--accent-hover": "#0a6d55",
      "--accent-soft": "#e6f5ee",
      "--accent-border": "#b4dcca",
    },
    {
      "--accent": "#29b48e",
      "--accent-hover": "#5bd1b1",
      "--accent-soft": "rgba(41, 180, 142, 0.16)",
      "--accent-border": "rgba(41, 180, 142, 0.34)",
    },
  ),
  preset(
    "night",
    ["#0d0f12", "#171b20", "#f3f6f7", "#28b49a"],
    {},
    {
      "--background": "#0d0f12",
      "--surface": "#12161a",
      "--surface-raised": "#171b20",
      "--surface-muted": "#1f252b",
      "--surface-hover": "#262d34",
      "--accent": "#28b49a",
      "--accent-hover": "#57d8c0",
      "--reading-background": "#0d0f12",
      "--reading-text": "#d8dee3",
    },
  ),
  preset(
    "highContrast",
    ["#ffffff", "#000000", "#005fcc", "#00a878"],
    {
      "--foreground": "#000000",
      "--border": "#000000",
      "--border-subtle": "#6b7280",
      "--text-primary": "#000000",
      "--text-secondary": "#111827",
      "--text-muted": "#374151",
      "--accent": "#005fcc",
      "--accent-hover": "#003f8c",
      "--accent-soft": "#e6f0ff",
      "--accent-border": "#005fcc",
      "--reading-text": "#111827",
      "--reading-link": "#003f8c",
      "--reading-border": "#6b7280",
    },
    {
      "--background": "#000000",
      "--foreground": "#ffffff",
      "--surface": "#050505",
      "--surface-raised": "#0a0a0a",
      "--surface-muted": "#151515",
      "--surface-hover": "#202020",
      "--border": "#ffffff",
      "--border-subtle": "#8a8a8a",
      "--text-primary": "#ffffff",
      "--text-secondary": "#f2f2f2",
      "--text-muted": "#d0d0d0",
      "--accent": "#64b5ff",
      "--accent-hover": "#9ed0ff",
      "--accent-soft": "rgba(100, 181, 255, 0.22)",
      "--accent-border": "#64b5ff",
      "--reading-background": "#000000",
      "--reading-text": "#f2f2f2",
      "--reading-muted": "#d0d0d0",
      "--reading-link": "#9ed0ff",
      "--reading-code-bg": "#151515",
      "--reading-border": "#8a8a8a",
    },
  ),
];

export function getColorThemePreset(id: string): ColorThemePreset {
  return COLOR_THEME_PRESETS.find((themePreset) => themePreset.id === id) ?? COLOR_THEME_PRESETS[0];
}

export function resolveColorThemeVariant(
  themePreset: ColorThemePreset,
  theme: ThemePreference,
  prefersDark: boolean,
): ColorThemeVariant {
  if (themePreset.id === "night") return themePreset.variants.dark;
  if (theme === "dark") return themePreset.variants.dark;
  if (theme === "auto" && prefersDark) return themePreset.variants.dark;
  return themePreset.variants.light;
}

export function applyColorThemePresetToRoot(
  presetId: string,
  theme: ThemePreference,
  root = document.documentElement,
  prefersDark =
    typeof window !== "undefined" &&
    typeof window.matchMedia === "function" &&
    window.matchMedia("(prefers-color-scheme: dark)").matches,
) {
  const themePreset = getColorThemePreset(presetId);
  const resolved = resolveColorThemeVariant(themePreset, theme, prefersDark);
  root.dataset.colorThemePreset = themePreset.id;
  for (const [name, value] of Object.entries(resolved.cssVars)) {
    root.style.setProperty(name, value);
  }
  root.style.colorScheme = resolved.mode;
}
