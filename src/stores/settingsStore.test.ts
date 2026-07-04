import { beforeEach, describe, expect, it } from "vitest";

import { useSettingsStore } from "./settingsStore";

describe("settingsStore color theme preset", () => {
  beforeEach(() => {
    window.localStorage.clear();
    document.documentElement.removeAttribute("data-color-theme-preset");
    document.documentElement.removeAttribute("data-theme");
    document.documentElement.style.cssText = "";
    useSettingsStore.getState().reset();
  });

  it("applies the default color theme preset to the document root", async () => {
    await useSettingsStore.getState().loadSettings("project-1", "D:/wiki");

    expect(document.documentElement.dataset.colorThemePreset).toBe("codex");
  });
});
