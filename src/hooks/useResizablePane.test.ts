import { beforeEach, describe, expect, it } from "vitest";
import {
  clampPaneWidth,
  DEFAULT_LAYOUT_PREFERENCES,
  readLayoutPreferenceSnapshot,
  sanitizeLayoutPreferences,
  writeLayoutPreferenceSnapshot,
} from "./useResizablePane";

describe("clampPaneWidth", () => {
  it("clamps invalid and out-of-range widths", () => {
    expect(clampPaneWidth(100, 180, 360)).toBe(180);
    expect(clampPaneWidth(420, 180, 360)).toBe(360);
    expect(clampPaneWidth(Number.NaN, 180, 360)).toBe(240);
    expect(clampPaneWidth(-20, 220, 480)).toBe(220);
  });
});

describe("layout preference storage", () => {
  beforeEach(() => window.localStorage.clear());

  it("falls back to defaults for corrupt snapshots", () => {
    window.localStorage.setItem("llm-wiki-desktop.layout.v1", "{broken");
    expect(readLayoutPreferenceSnapshot()).toEqual(DEFAULT_LAYOUT_PREFERENCES);
  });

  it("sanitizes every pane width against its limit", () => {
    const snapshot = sanitizeLayoutPreferences({
      sidebarCollapsed: true,
      paneSizes: {
        sidebar: 999,
        rightPanel: 100,
        wikiTree: Number.NaN,
        exportsList: 360,
        lintList: -4,
      },
    });

    expect(snapshot.sidebarCollapsed).toBe(true);
    expect(snapshot.paneSizes.sidebar).toBe(360);
    expect(snapshot.paneSizes.rightPanel).toBe(280);
    expect(snapshot.paneSizes.wikiTree).toBe(260);
    expect(snapshot.paneSizes.exportsList).toBe(360);
    expect(snapshot.paneSizes.lintList).toBe(220);
  });

  it("round-trips a valid snapshot", () => {
    writeLayoutPreferenceSnapshot({
      sidebarCollapsed: true,
      paneSizes: {
        sidebar: 300,
        rightPanel: 420,
        wikiTree: 320,
        exportsList: 400,
        lintList: 280,
      },
    });

    expect(readLayoutPreferenceSnapshot().paneSizes.rightPanel).toBe(420);
    expect(readLayoutPreferenceSnapshot().sidebarCollapsed).toBe(true);
  });
});
