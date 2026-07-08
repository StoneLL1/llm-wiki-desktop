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
  "--background": "#fbfbfa",
  "--foreground": "#111111",
  "--surface": "#f6f7f6",
  "--surface-raised": "#ffffff",
  "--surface-muted": "#eef0ef",
  "--surface-hover": "#f2f4f3",
  "--border": "#dedfdd",
  "--border-subtle": "#ececea",
  "--text-primary": "#111111",
  "--text-secondary": "#3b3d3c",
  "--text-muted": "#6f7471",
  "--accent": "#0f766e",
  "--accent-hover": "#0b5f59",
  "--accent-soft": "#e3f3f0",
  "--accent-border": "#9fd4cc",
  "--reading-background": "#ffffff",
  "--reading-text": "#3b3d3c",
  "--reading-muted": "#6f7471",
  "--reading-link": "#0b5f59",
  "--reading-code-bg": "#eef0ef",
  "--reading-border": "#ececea",
};

const codexDark: Record<ThemeCssVar, string> = {
  "--background": "#101312",
  "--foreground": "#f3f6f4",
  "--surface": "#151918",
  "--surface-raised": "#1a1f1e",
  "--surface-muted": "#202625",
  "--surface-hover": "#252c2a",
  "--border": "#2b3331",
  "--border-subtle": "#222a28",
  "--text-primary": "#f3f6f4",
  "--text-secondary": "#d3dbd8",
  "--text-muted": "#99a4a0",
  "--accent": "#2dd4bf",
  "--accent-hover": "#5eead4",
  "--accent-soft": "rgba(45, 212, 191, 0.16)",
  "--accent-border": "rgba(45, 212, 191, 0.34)",
  "--reading-background": "#101312",
  "--reading-text": "#d3dbd8",
  "--reading-muted": "#99a4a0",
  "--reading-link": "#5eead4",
  "--reading-code-bg": "#202625",
  "--reading-border": "#222a28",
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
  preset("codex", ["#fbfbfa", "#eef0ef", "#111111", "#0f766e"], {}, {}),
  preset(
    "paper",
    ["#fffaf2", "#f0e4d4", "#2b2118", "#c2410c"],
    {
      "--background": "#fffaf2",
      "--foreground": "#2b2118",
      "--surface": "#fbf1e4",
      "--surface-raised": "#fffdfa",
      "--surface-muted": "#f0e4d4",
      "--surface-hover": "#f6eadb",
      "--border": "#dccbb8",
      "--border-subtle": "#eadbca",
      "--text-primary": "#2b2118",
      "--text-secondary": "#574536",
      "--text-muted": "#83705f",
      "--accent": "#c2410c",
      "--accent-hover": "#9a3412",
      "--accent-soft": "#ffedd5",
      "--accent-border": "#fdba74",
      "--reading-background": "#fffdfa",
      "--reading-text": "#574536",
      "--reading-muted": "#83705f",
      "--reading-link": "#9a3412",
      "--reading-code-bg": "#f0e4d4",
      "--reading-border": "#eadbca",
    },
    {
      "--background": "#18120e",
      "--surface": "#211813",
      "--surface-raised": "#2a1f18",
      "--surface-muted": "#33261d",
      "--surface-hover": "#3c2d22",
      "--border": "#4b392b",
      "--border-subtle": "#3a2b21",
      "--text-primary": "#fff7ed",
      "--text-secondary": "#efd6bf",
      "--text-muted": "#c6a98f",
      "--accent": "#fb923c",
      "--accent-hover": "#fdba74",
      "--accent-soft": "rgba(251, 146, 60, 0.18)",
      "--accent-border": "rgba(251, 146, 60, 0.38)",
      "--reading-background": "#18120e",
      "--reading-text": "#efd6bf",
      "--reading-muted": "#c6a98f",
      "--reading-link": "#fdba74",
      "--reading-code-bg": "#33261d",
      "--reading-border": "#3a2b21",
    },
  ),
  preset(
    "graphite",
    ["#f7f7fb", "#e0e1eb", "#171821", "#5b5bd6"],
    {
      "--background": "#fbfbfd",
      "--foreground": "#171821",
      "--surface": "#f4f5fa",
      "--surface-raised": "#ffffff",
      "--surface-muted": "#e9ebf2",
      "--surface-hover": "#eff1f7",
      "--border": "#d9dce7",
      "--border-subtle": "#e6e8ef",
      "--text-primary": "#171821",
      "--text-secondary": "#3d4052",
      "--text-muted": "#71768a",
      "--accent": "#5b5bd6",
      "--accent-hover": "#4341b8",
      "--accent-soft": "#ececff",
      "--accent-border": "#c7d2fe",
      "--reading-background": "#ffffff",
      "--reading-text": "#3d4052",
      "--reading-muted": "#71768a",
      "--reading-link": "#4341b8",
      "--reading-code-bg": "#e9ebf2",
      "--reading-border": "#e6e8ef",
    },
    {
      "--background": "#111218",
      "--surface": "#171922",
      "--surface-raised": "#1d202b",
      "--surface-muted": "#252836",
      "--surface-hover": "#2b2f3e",
      "--border": "#34394a",
      "--border-subtle": "#282c3a",
      "--text-primary": "#f5f6fb",
      "--text-secondary": "#d4d8e6",
      "--text-muted": "#9da4ba",
      "--accent": "#a5b4fc",
      "--accent-hover": "#c4b5fd",
      "--accent-soft": "rgba(165, 180, 252, 0.16)",
      "--accent-border": "rgba(165, 180, 252, 0.36)",
      "--reading-background": "#111218",
      "--reading-text": "#d4d8e6",
      "--reading-muted": "#9da4ba",
      "--reading-link": "#c4b5fd",
      "--reading-code-bg": "#252836",
      "--reading-border": "#282c3a",
    },
  ),
  preset(
    "mint",
    ["#f7fffb", "#dff7ec", "#10231a", "#059669"],
    {
      "--background": "#f7fffb",
      "--foreground": "#10231a",
      "--surface": "#effbf5",
      "--surface-raised": "#ffffff",
      "--surface-muted": "#dff7ec",
      "--surface-hover": "#e9fbf2",
      "--border": "#c6e8d6",
      "--border-subtle": "#d8f0e3",
      "--text-primary": "#10231a",
      "--text-secondary": "#294c3b",
      "--text-muted": "#5f7d6d",
      "--accent": "#059669",
      "--accent-hover": "#047857",
      "--accent-soft": "#d1fae5",
      "--accent-border": "#86efac",
      "--reading-background": "#ffffff",
      "--reading-text": "#294c3b",
      "--reading-muted": "#5f7d6d",
      "--reading-link": "#047857",
      "--reading-code-bg": "#dff7ec",
      "--reading-border": "#d8f0e3",
    },
    {
      "--background": "#071510",
      "--surface": "#0d1c16",
      "--surface-raised": "#12261e",
      "--surface-muted": "#183126",
      "--surface-hover": "#1e3c2f",
      "--border": "#275242",
      "--border-subtle": "#1f4034",
      "--text-primary": "#ecfdf5",
      "--text-secondary": "#c7f0db",
      "--text-muted": "#8dc5aa",
      "--accent": "#34d399",
      "--accent-hover": "#6ee7b7",
      "--accent-soft": "rgba(52, 211, 153, 0.16)",
      "--accent-border": "rgba(52, 211, 153, 0.34)",
      "--reading-background": "#071510",
      "--reading-text": "#c7f0db",
      "--reading-muted": "#8dc5aa",
      "--reading-link": "#6ee7b7",
      "--reading-code-bg": "#183126",
      "--reading-border": "#1f4034",
    },
  ),
  preset(
    "night",
    ["#090b1a", "#1d2140", "#f5f3ff", "#8b5cf6"],
    {
      "--background": "#fbfaff",
      "--foreground": "#171326",
      "--surface": "#f3f0ff",
      "--surface-raised": "#ffffff",
      "--surface-muted": "#ebe7ff",
      "--surface-hover": "#f0edff",
      "--border": "#d8d1f0",
      "--border-subtle": "#e6e1f5",
      "--text-primary": "#171326",
      "--text-secondary": "#40375e",
      "--text-muted": "#776d96",
      "--accent": "#7c3aed",
      "--accent-hover": "#6d28d9",
      "--accent-soft": "#ede9fe",
      "--accent-border": "#c4b5fd",
      "--reading-background": "#ffffff",
      "--reading-text": "#40375e",
      "--reading-muted": "#776d96",
      "--reading-link": "#6d28d9",
      "--reading-code-bg": "#ebe7ff",
      "--reading-border": "#e6e1f5",
    },
    {
      "--background": "#090b1a",
      "--foreground": "#f5f3ff",
      "--surface": "#10142a",
      "--surface-raised": "#171b34",
      "--surface-muted": "#1d2140",
      "--surface-hover": "#252a4e",
      "--border": "#31365f",
      "--border-subtle": "#272b4e",
      "--text-primary": "#f5f3ff",
      "--text-secondary": "#ddd6fe",
      "--text-muted": "#aaa2d6",
      "--accent": "#8b5cf6",
      "--accent-hover": "#a78bfa",
      "--accent-soft": "rgba(139, 92, 246, 0.18)",
      "--accent-border": "rgba(139, 92, 246, 0.4)",
      "--reading-background": "#090b1a",
      "--reading-text": "#ddd6fe",
      "--reading-muted": "#aaa2d6",
      "--reading-link": "#a78bfa",
      "--reading-code-bg": "#1d2140",
      "--reading-border": "#272b4e",
    },
  ),
  preset(
    "highContrast",
    ["#ffffff", "#050505", "#003f8c", "#7dd3fc"],
    {
      "--foreground": "#000000",
      "--border": "#000000",
      "--border-subtle": "#4b5563",
      "--text-primary": "#000000",
      "--text-secondary": "#111827",
      "--text-muted": "#1f2937",
      "--accent": "#0047b3",
      "--accent-hover": "#002f7a",
      "--accent-soft": "#e6f0ff",
      "--accent-border": "#0047b3",
      "--reading-text": "#111827",
      "--reading-muted": "#1f2937",
      "--reading-link": "#002f7a",
      "--reading-border": "#4b5563",
    },
    {
      "--background": "#050505",
      "--foreground": "#ffffff",
      "--surface": "#0b0b0b",
      "--surface-raised": "#111111",
      "--surface-muted": "#1a1a1a",
      "--surface-hover": "#242424",
      "--border": "#ffffff",
      "--border-subtle": "#8a8a8a",
      "--text-primary": "#ffffff",
      "--text-secondary": "#f2f2f2",
      "--text-muted": "#d8d8d8",
      "--accent": "#7dd3fc",
      "--accent-hover": "#bae6fd",
      "--accent-soft": "rgba(125, 211, 252, 0.22)",
      "--accent-border": "#7dd3fc",
      "--reading-background": "#050505",
      "--reading-text": "#f2f2f2",
      "--reading-muted": "#d8d8d8",
      "--reading-link": "#bae6fd",
      "--reading-code-bg": "#1a1a1a",
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
