import { useCallback, useEffect, useRef } from "react";
import type {
  KeyboardEvent as ReactKeyboardEvent,
  PointerEvent as ReactPointerEvent,
} from "react";

export type ResizablePaneId =
  | "sidebar"
  | "rightPanel"
  | "wikiTree"
  | "exportsList"
  | "lintDetails";

export interface PaneWidthLimit {
  min: number;
  max: number;
  defaultValue: number;
}

export const LAYOUT_STORAGE_KEY = "llm-wiki-desktop.layout.v1";
export const SIDEBAR_COLLAPSE_THRESHOLD = 96;

export const PANE_WIDTH_LIMITS: Record<ResizablePaneId, PaneWidthLimit> = {
  sidebar: { min: 56, max: 360, defaultValue: 240 },
  rightPanel: { min: 280, max: 520, defaultValue: 320 },
  wikiTree: { min: 220, max: 480, defaultValue: 260 },
  exportsList: { min: 220, max: 480, defaultValue: 360 },
  lintDetails: { min: 280, max: 520, defaultValue: 320 },
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
    lintDetails: PANE_WIDTH_LIMITS.lintDetails.defaultValue,
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

  const sanitizedPaneSizes = PANE_IDS.reduce(
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
  );

  if (candidate.sidebarCollapsed === true) {
    sanitizedPaneSizes.sidebar = PANE_WIDTH_LIMITS.sidebar.min;
  }

  return {
    sidebarCollapsed: sanitizedPaneSizes.sidebar <= SIDEBAR_COLLAPSE_THRESHOLD,
    paneSizes: sanitizedPaneSizes,
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

export interface UseResizablePaneOptions {
  value: number;
  min: number;
  max: number;
  step?: number;
  direction?: 1 | -1;
  onPreview: (value: number) => void;
  onCommit: (value: number) => void;
  onReset: () => void;
}

export interface UseResizablePaneResult {
  separatorProps: {
    onPointerDown: (event: ReactPointerEvent<HTMLElement>) => void;
    onLostPointerCapture: () => void;
    onDoubleClick: () => void;
    onKeyDown: (event: ReactKeyboardEvent<HTMLElement>) => void;
  };
}

interface DragSnapshot {
  element: HTMLElement;
  pointerId: number;
  startValue: number;
  startX: number;
  lastValue: number;
  animationFrameId: number | null;
}

export function useResizablePane({
  value,
  min,
  max,
  step = 12,
  direction = 1,
  onPreview,
  onCommit,
  onReset,
}: UseResizablePaneOptions): UseResizablePaneResult {
  const dragSnapshotRef = useRef<DragSnapshot | null>(null);
  const valueRef = useRef(value);
  const minRef = useRef(min);
  const maxRef = useRef(max);
  const stepRef = useRef(step);
  const directionRef = useRef(direction);
  const onPreviewRef = useRef(onPreview);
  const onCommitRef = useRef(onCommit);
  const onResetRef = useRef(onReset);

  valueRef.current = value;
  minRef.current = min;
  maxRef.current = max;
  stepRef.current = step;
  directionRef.current = direction;
  onPreviewRef.current = onPreview;
  onCommitRef.current = onCommit;
  onResetRef.current = onReset;

  const cancelAnimationFrameFor = useCallback((snapshot: DragSnapshot) => {
    if (snapshot.animationFrameId !== null) {
      window.cancelAnimationFrame(snapshot.animationFrameId);
      snapshot.animationFrameId = null;
    }
  }, []);

  const previewValue = useCallback((snapshot: DragSnapshot, nextValue: number) => {
    onPreviewRef.current(nextValue);
    snapshot.element.setAttribute("aria-valuenow", String(nextValue));
  }, []);

  const releaseDrag = useCallback((snapshot: DragSnapshot) => {
    dragSnapshotRef.current = null;
    snapshot.element.classList.remove("is-dragging");
    if (snapshot.element.hasPointerCapture?.(snapshot.pointerId)) {
      snapshot.element.releasePointerCapture(snapshot.pointerId);
    }

    document.body.classList.remove("is-resizing-pane");
  }, []);

  const cancelDrag = useCallback(() => {
    const snapshot = dragSnapshotRef.current;
    if (!snapshot) {
      return;
    }

    cancelAnimationFrameFor(snapshot);
    previewValue(snapshot, valueRef.current);
    releaseDrag(snapshot);
  }, [cancelAnimationFrameFor, previewValue, releaseDrag]);

  useEffect(() => {
    const handlePointerMove = (event: PointerEvent) => {
      const snapshot = dragSnapshotRef.current;
      if (!snapshot || event.pointerId !== snapshot.pointerId) {
        return;
      }

      const nextValue =
        snapshot.startValue + (event.clientX - snapshot.startX) * directionRef.current;
      snapshot.lastValue = clampPaneWidth(nextValue, minRef.current, maxRef.current);
      if (snapshot.animationFrameId === null) {
        snapshot.animationFrameId = window.requestAnimationFrame(() => {
          const activeSnapshot = dragSnapshotRef.current;
          if (activeSnapshot !== snapshot) {
            return;
          }

          snapshot.animationFrameId = null;
          previewValue(snapshot, snapshot.lastValue);
        });
      }
    };

    const handlePointerUp = (event: PointerEvent) => {
      const snapshot = dragSnapshotRef.current;
      if (!snapshot || event.pointerId !== snapshot.pointerId) {
        return;
      }

      const nextValue =
        snapshot.startValue + (event.clientX - snapshot.startX) * directionRef.current;
      snapshot.lastValue = clampPaneWidth(nextValue, minRef.current, maxRef.current);
      cancelAnimationFrameFor(snapshot);
      previewValue(snapshot, snapshot.lastValue);
      if (snapshot.lastValue !== valueRef.current) {
        onCommitRef.current(snapshot.lastValue);
      }
      releaseDrag(snapshot);
    };

    const handlePointerCancel = (event: PointerEvent) => {
      const snapshot = dragSnapshotRef.current;
      if (snapshot && event.pointerId === snapshot.pointerId) {
        cancelDrag();
      }
    };

    document.addEventListener("pointermove", handlePointerMove);
    document.addEventListener("pointerup", handlePointerUp);
    document.addEventListener("pointercancel", handlePointerCancel);

    return () => {
      document.removeEventListener("pointermove", handlePointerMove);
      document.removeEventListener("pointerup", handlePointerUp);
      document.removeEventListener("pointercancel", handlePointerCancel);
      cancelDrag();
    };
  }, [cancelAnimationFrameFor, cancelDrag, previewValue, releaseDrag]);

  const onPointerDown = useCallback((event: ReactPointerEvent<HTMLElement>) => {
    cancelDrag();
    event.preventDefault();
    event.currentTarget.setPointerCapture?.(event.pointerId);
    event.currentTarget.classList.add("is-dragging");
    document.body.classList.add("is-resizing-pane");
    dragSnapshotRef.current = {
      element: event.currentTarget,
      pointerId: event.pointerId,
      startValue: valueRef.current,
      startX: event.clientX,
      lastValue: valueRef.current,
      animationFrameId: null,
    };
  }, [cancelDrag]);

  const onLostPointerCapture = useCallback(() => {
    cancelDrag();
  }, [cancelDrag]);

  const onDoubleClick = useCallback(() => {
    onResetRef.current();
  }, []);

  const onKeyDown = useCallback((event: ReactKeyboardEvent<HTMLElement>) => {
    const keyDelta: Record<string, number> = {
      ArrowRight: stepRef.current * directionRef.current,
      ArrowLeft: -stepRef.current * directionRef.current,
    };

    if (event.key in keyDelta) {
      event.preventDefault();
      onCommitRef.current(
        clampPaneWidth(valueRef.current + keyDelta[event.key], minRef.current, maxRef.current),
      );
      return;
    }

    if (event.key === "Home") {
      event.preventDefault();
      onCommitRef.current(minRef.current);
      return;
    }

    if (event.key === "End") {
      event.preventDefault();
      onCommitRef.current(maxRef.current);
      return;
    }

    if (event.key === "Enter") {
      event.preventDefault();
      onResetRef.current();
    }
  }, []);

  return {
    separatorProps: {
      onPointerDown,
      onLostPointerCapture,
      onDoubleClick,
      onKeyDown,
    },
  };
}
