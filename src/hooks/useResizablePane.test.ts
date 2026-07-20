import { fireEvent, render, screen } from "@testing-library/react";
import { createElement } from "react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it } from "vitest";
import {
  clampPaneWidth,
  DEFAULT_LAYOUT_PREFERENCES,
  PANE_WIDTH_LIMITS,
  readLayoutPreferenceSnapshot,
  sanitizeLayoutPreferences,
  SIDEBAR_COLLAPSE_THRESHOLD,
  useResizablePane,
  writeLayoutPreferenceSnapshot,
} from "./useResizablePane";

describe("clampPaneWidth", () => {
  it("clamps invalid and out-of-range widths", () => {
    expect(clampPaneWidth(48, 56, 360)).toBe(56);
    expect(clampPaneWidth(420, 180, 360)).toBe(360);
    expect(clampPaneWidth(Number.NaN, 56, 360)).toBe(240);
    expect(clampPaneWidth(-20, 220, 480)).toBe(220);
  });
});

describe("sidebar resize constants", () => {
  it("allows the sidebar splitter to reach the icon rail threshold", () => {
    expect(PANE_WIDTH_LIMITS.sidebar.min).toBe(56);
    expect(SIDEBAR_COLLAPSE_THRESHOLD).toBe(96);
    expect(PANE_WIDTH_LIMITS.sidebar.min).toBeLessThan(SIDEBAR_COLLAPSE_THRESHOLD);
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
        lintDetails: -4,
      },
    });

    expect(snapshot.sidebarCollapsed).toBe(true);
    expect(snapshot.paneSizes.sidebar).toBe(56);
    expect(snapshot.paneSizes.rightPanel).toBe(280);
    expect(snapshot.paneSizes.wikiTree).toBe(260);
    expect(snapshot.paneSizes.exportsList).toBe(360);
    expect(snapshot.paneSizes.lintDetails).toBe(280);
  });

  it("derives sidebar collapse from the sanitized width", () => {
    const snapshot = sanitizeLayoutPreferences({
      sidebarCollapsed: false,
      paneSizes: {
        sidebar: 80,
      },
    });

    expect(snapshot.sidebarCollapsed).toBe(true);
    expect(snapshot.paneSizes.sidebar).toBe(80);
  });

  it("round-trips a valid snapshot", () => {
    writeLayoutPreferenceSnapshot({
      sidebarCollapsed: true,
      paneSizes: {
        sidebar: 300,
        rightPanel: 420,
        wikiTree: 320,
        exportsList: 400,
        lintDetails: 300,
      },
    });

    expect(readLayoutPreferenceSnapshot().paneSizes.rightPanel).toBe(420);
    expect(readLayoutPreferenceSnapshot().sidebarCollapsed).toBe(true);
    expect(readLayoutPreferenceSnapshot().paneSizes.sidebar).toBe(56);
  });
});

interface ResizeHarnessProps {
  children?: ReactNode;
  direction?: 1 | -1;
  max?: number;
  min?: number;
  onChange: (value: number) => void;
  onReset: () => void;
  value?: number;
}

function ResizeHarness({
  children,
  direction = 1,
  max = 360,
  min = 180,
  onChange,
  onReset,
  value = 240,
}: ResizeHarnessProps) {
  const { separatorProps } = useResizablePane({
    value,
    min,
    max,
    direction,
    onChange,
    onReset,
  });

  return createElement(
    "div",
    {
      "aria-label": "Resize test pane",
      role: "separator",
      tabIndex: 0,
      ...separatorProps,
    },
    children,
  );
}

function dispatchPointerEvent(target: Document | Element | Window, type: string, clientX: number) {
  const event = new Event(type, { bubbles: true, cancelable: true });
  Object.defineProperty(event, "clientX", { value: clientX });
  Object.defineProperty(event, "pointerId", { value: 1 });
  fireEvent(target, event);
}

describe("useResizablePane", () => {
  it("adjusts pane width with keyboard commands", () => {
    const changes: number[] = [];
    let resetCount = 0;
    render(
      createElement(ResizeHarness, {
        onChange: (value) => changes.push(value),
        onReset: () => {
          resetCount += 1;
        },
      }),
    );

    const separator = screen.getByRole("separator", { name: "Resize test pane" });
    fireEvent.keyDown(separator, { key: "ArrowRight" });
    fireEvent.keyDown(separator, { key: "ArrowLeft" });
    fireEvent.keyDown(separator, { key: "Home" });
    fireEvent.keyDown(separator, { key: "End" });
    fireEvent.keyDown(separator, { key: "Enter" });

    expect(changes).toEqual([252, 228, 180, 360]);
    expect(resetCount).toBe(1);
  });

  it("uses pointer delta and direction when dragging", () => {
    const changes: number[] = [];
    render(
      createElement(ResizeHarness, {
        direction: -1,
        max: 520,
        min: 280,
        onChange: (value) => changes.push(value),
        onReset: () => undefined,
        value: 320,
      }),
    );

    const separator = screen.getByRole("separator", { name: "Resize test pane" });
    dispatchPointerEvent(separator, "pointerdown", 200);
    dispatchPointerEvent(document, "pointermove", 170);
    dispatchPointerEvent(document, "pointermove", 260);
    dispatchPointerEvent(document, "pointerup", 260);

    expect(changes).toEqual([350, 280]);
    expect(document.body).not.toHaveClass("is-resizing-pane");
  });
});
