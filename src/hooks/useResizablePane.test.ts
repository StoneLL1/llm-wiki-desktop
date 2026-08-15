import { fireEvent, render, screen } from "@testing-library/react";
import { createElement } from "react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
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
  onCommit: (value: number) => void;
  onPreview: (value: number) => void;
  onReset: () => void;
  value?: number;
}

function ResizeHarness({
  children,
  direction = 1,
  max = 360,
  min = 180,
  onCommit,
  onPreview,
  onReset,
  value = 240,
}: ResizeHarnessProps) {
  const { separatorProps } = useResizablePane({
    value,
    min,
    max,
    direction,
    onCommit,
    onPreview,
    onReset,
  });

  return createElement(
    "div",
    {
      "aria-label": "Resize test pane",
      "aria-valuemax": max,
      "aria-valuemin": min,
      "aria-valuenow": value,
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
  let animationFrames: Map<number, FrameRequestCallback>;
  let nextAnimationFrameId: number;

  beforeEach(() => {
    animationFrames = new Map();
    nextAnimationFrameId = 1;
    vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
      const id = nextAnimationFrameId;
      nextAnimationFrameId += 1;
      animationFrames.set(id, callback);
      return id;
    });
    vi.stubGlobal("cancelAnimationFrame", (id: number) => {
      animationFrames.delete(id);
    });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  const flushAnimationFrame = () => {
    const callbacks = [...animationFrames.values()];
    animationFrames.clear();
    callbacks.forEach((callback) => callback(0));
  };

  it("adjusts pane width with keyboard commands", () => {
    const commits: number[] = [];
    const previews: number[] = [];
    let resetCount = 0;
    render(
      createElement(ResizeHarness, {
        onCommit: (value) => commits.push(value),
        onPreview: (value) => previews.push(value),
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

    expect(commits).toEqual([252, 228, 180, 360]);
    expect(previews).toEqual([]);
    expect(resetCount).toBe(1);
  });

  it("coalesces pointer moves to the final preview in each animation frame", () => {
    const commits: number[] = [];
    const previews: number[] = [];
    render(
      createElement(ResizeHarness, {
        onCommit: (value) => commits.push(value),
        onPreview: (value) => previews.push(value),
        onReset: () => undefined,
      }),
    );

    const separator = screen.getByRole("separator", { name: "Resize test pane" });
    dispatchPointerEvent(separator, "pointerdown", 200);
    for (let clientX = 201; clientX <= 220; clientX += 1) {
      dispatchPointerEvent(document, "pointermove", clientX);
    }

    expect(previews).toEqual([]);
    expect(animationFrames.size).toBe(1);

    flushAnimationFrame();
    expect(previews).toEqual([260]);

    dispatchPointerEvent(document, "pointermove", 230);
    dispatchPointerEvent(document, "pointermove", 240);
    expect(animationFrames.size).toBe(1);
    flushAnimationFrame();

    expect(previews).toEqual([260, 280]);
    expect(commits).toEqual([]);
  });

  it("flushes the final pointer position and commits exactly once on pointerup", () => {
    const commits: number[] = [];
    const previews: number[] = [];
    render(
      createElement(ResizeHarness, {
        direction: -1,
        max: 520,
        min: 280,
        onCommit: (value) => commits.push(value),
        onPreview: (value) => previews.push(value),
        onReset: () => undefined,
        value: 320,
      }),
    );

    const separator = screen.getByRole("separator", { name: "Resize test pane" });
    dispatchPointerEvent(separator, "pointerdown", 200);
    dispatchPointerEvent(document, "pointermove", 170);
    dispatchPointerEvent(document, "pointerup", 260);

    expect(previews).toEqual([280]);
    expect(commits).toEqual([280]);
    expect(animationFrames.size).toBe(0);
    expect(separator).toHaveAttribute("aria-valuenow", "280");
    expect(document.body).not.toHaveClass("is-resizing-pane");
  });

  it("does not commit a pointer interaction whose final value is unchanged", () => {
    const commits: number[] = [];
    const previews: number[] = [];
    render(
      createElement(ResizeHarness, {
        onCommit: (value) => commits.push(value),
        onPreview: (value) => previews.push(value),
        onReset: () => undefined,
      }),
    );

    const separator = screen.getByRole("separator", { name: "Resize test pane" });
    dispatchPointerEvent(separator, "pointerdown", 240);
    dispatchPointerEvent(document, "pointerup", 240);

    expect(previews).toEqual([240]);
    expect(commits).toEqual([]);
  });

  it("rolls back a preview without committing when pointer capture is cancelled", () => {
    const commits: number[] = [];
    const previews: number[] = [];
    render(
      createElement(ResizeHarness, {
        onCommit: (value) => commits.push(value),
        onPreview: (value) => previews.push(value),
        onReset: () => undefined,
      }),
    );

    const separator = screen.getByRole("separator", { name: "Resize test pane" });
    dispatchPointerEvent(separator, "pointerdown", 200);
    dispatchPointerEvent(document, "pointermove", 260);
    flushAnimationFrame();
    dispatchPointerEvent(document, "pointermove", 280);
    dispatchPointerEvent(document, "pointercancel", 280);

    expect(previews).toEqual([300, 240]);
    expect(commits).toEqual([]);
    expect(animationFrames.size).toBe(0);
    expect(separator).toHaveAttribute("aria-valuenow", "240");
  });

  it("cancels a pending preview on unmount without a late commit", () => {
    const commits: number[] = [];
    const previews: number[] = [];
    const view = render(
      createElement(ResizeHarness, {
        onCommit: (value) => commits.push(value),
        onPreview: (value) => previews.push(value),
        onReset: () => undefined,
      }),
    );

    const separator = screen.getByRole("separator", { name: "Resize test pane" });
    dispatchPointerEvent(separator, "pointerdown", 200);
    dispatchPointerEvent(document, "pointermove", 260);
    view.unmount();
    flushAnimationFrame();

    expect(previews).toEqual([240]);
    expect(commits).toEqual([]);
    expect(animationFrames.size).toBe(0);
    expect(document.body).not.toHaveClass("is-resizing-pane");
  });
});
