import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { applyColorThemePresetPreference, useSettingsStore } from "./settingsStore";

let darkModeListener: ((event: MediaQueryListEvent) => void) | null = null;

describe("settingsStore color theme preset", () => {
  beforeEach(() => {
    window.localStorage.clear();
    document.documentElement.removeAttribute("data-color-theme-preset");
    document.documentElement.removeAttribute("data-theme");
    document.documentElement.style.cssText = "";
    darkModeListener = null;
    vi.stubGlobal(
      "matchMedia",
      vi.fn(
        () =>
          ({
            matches: false,
            media: "(prefers-color-scheme: dark)",
            addEventListener: vi.fn((event: string, listener: (event: MediaQueryListEvent) => void) => {
              if (event === "change") darkModeListener = listener;
            }),
            removeEventListener: vi.fn(),
          }) as unknown as MediaQueryList,
      ),
    );
    useSettingsStore.getState().reset();
  });

  afterEach(() => {
    applyColorThemePresetPreference("codex", "light");
    vi.unstubAllGlobals();
  });

  it("applies the default color theme preset to the document root", async () => {
    await useSettingsStore.getState().loadSettings("project-1", "D:/wiki");

    expect(document.documentElement.dataset.colorThemePreset).toBe("codex");
  });

  it("reapplies auto color theme tokens when the OS dark preference changes", async () => {
    await useSettingsStore.getState().loadSettings("project-1", "D:/wiki");

    expect(document.documentElement.style.getPropertyValue("--background")).toBe("#ffffff");

    darkModeListener?.({ matches: true } as MediaQueryListEvent);

    expect(document.documentElement.style.getPropertyValue("--background")).toBe("#111315");
  });
});
