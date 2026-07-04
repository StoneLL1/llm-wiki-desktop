export type ResizablePaneId =
  | "sidebar"
  | "rightPanel"
  | "wikiTree"
  | "exportsList"
  | "lintList";

export interface PaneWidthLimit {
  min: number;
  max: number;
  defaultValue: number;
}

export const LAYOUT_STORAGE_KEY = "llm-wiki-desktop.layout.v1";

export const PANE_WIDTH_LIMITS: Record<ResizablePaneId, PaneWidthLimit> = {
  sidebar: { min: 180, max: 360, defaultValue: 240 },
  rightPanel: { min: 280, max: 520, defaultValue: 320 },
  wikiTree: { min: 220, max: 480, defaultValue: 260 },
  exportsList: { min: 220, max: 480, defaultValue: 360 },
  lintList: { min: 220, max: 480, defaultValue: 360 },
};

export interface LayoutPreferences {
  sidebarCollapsed: boolean;
  paneSizes: Record<ResizablePaneId, number>;
}

export const DEFAULT_LAYOUT_PREFERENCES: LayoutPreferences = {
  sidebarCollapsed: false,
  paneSizes: {
    sidebar: PANE_WIDTH_LIMITS.sidebar.defaultValue,
    rightPanel: PANE_WIDTH_LIMITS.rightPanel.defaultValue,
    wikiTree: PANE_WIDTH_LIMITS.wikiTree.defaultValue,
    exportsList: PANE_WIDTH_LIMITS.exportsList.defaultValue,
    lintList: PANE_WIDTH_LIMITS.lintList.defaultValue,
  },
};

const PANE_IDS = Object.keys(PANE_WIDTH_LIMITS) as ResizablePaneId[];

function cloneDefaultLayoutPreferences(): LayoutPreferences {
  return {
    sidebarCollapsed: DEFAULT_LAYOUT_PREFERENCES.sidebarCollapsed,
    paneSizes: { ...DEFAULT_LAYOUT_PREFERENCES.paneSizes },
  };
}

export function clampPaneWidth(value: number, min: number, max: number): number {
  if (!Number.isFinite(value)) {
    const matchingLimit = PANE_IDS.map((paneId) => PANE_WIDTH_LIMITS[paneId]).find(
      (limit) => limit.min === min && limit.max === max,
    );
    if (matchingLimit) {
      return matchingLimit.defaultValue;
    }

    return Math.round((min + max) / 2);
  }

  return Math.min(Math.max(value, min), max);
}

export function sanitizeLayoutPreferences(snapshot: unknown): LayoutPreferences {
  if (!snapshot || typeof snapshot !== "object") {
    return cloneDefaultLayoutPreferences();
  }

  const candidate = snapshot as Partial<LayoutPreferences>;
  const paneSizes =
    candidate.paneSizes && typeof candidate.paneSizes === "object"
      ? (candidate.paneSizes as Partial<Record<ResizablePaneId, number>>)
      : {};

  return {
    sidebarCollapsed: candidate.sidebarCollapsed === true,
    paneSizes: PANE_IDS.reduce(
      (sizes, paneId) => {
        const limit = PANE_WIDTH_LIMITS[paneId];
        sizes[paneId] = clampPaneWidth(
          paneSizes[paneId] ?? limit.defaultValue,
          limit.min,
          limit.max,
        );
        return sizes;
      },
      {} as Record<ResizablePaneId, number>,
    ),
  };
}

export function readLayoutPreferenceSnapshot(): LayoutPreferences {
  if (typeof window === "undefined") {
    return cloneDefaultLayoutPreferences();
  }

  try {
    const rawSnapshot = window.localStorage.getItem(LAYOUT_STORAGE_KEY);
    if (!rawSnapshot) {
      return cloneDefaultLayoutPreferences();
    }

    return sanitizeLayoutPreferences(JSON.parse(rawSnapshot));
  } catch {
    return cloneDefaultLayoutPreferences();
  }
}

export function writeLayoutPreferenceSnapshot(snapshot: LayoutPreferences): void {
  if (typeof window === "undefined") {
    return;
  }

  window.localStorage.setItem(
    LAYOUT_STORAGE_KEY,
    JSON.stringify(sanitizeLayoutPreferences(snapshot)),
  );
}
