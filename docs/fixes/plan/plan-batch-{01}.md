# Workbench Shell Layout Theme Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement batch 01 from `docs/fixes/01-workbench-shell-layout-theme.md`: resizable workbench panes, collapsible left sidebar, compact project switcher, and preset color themes that also affect Markdown reading surfaces.

**Architecture:** Keep React as the UI owner of interaction state, while filesystem and settings persistence continue through the existing Tauri service boundary. Layout width and sidebar-collapse preferences are personal window preferences stored only in `localStorage`; color theme preset is a global UI setting stored by `SettingsService` in the app config directory, not in project `.app/settings.json`.

**Tech Stack:** React 19, TypeScript, Zustand, Tailwind v4 token CSS in `src/styles.css`, Lucide React, react-i18next, Vitest + Testing Library, Tauri v2/Rust settings DTOs.

---

## Read Context

- Product and architecture: `SPEC/PRD.md`, `SPEC/SPEC.md`, `SPEC/APP_flow.md`, `SPEC/TECH_STACK.md`, `SPEC/BACKEND_STRUCTURE.md`.
- Frontend design constraints: `SPEC/FRONTEND_GUIDELINES.md`, `SPEC/DESIGN.md`, `UI-Frontend-design/dashboard.html`, `UI-Frontend-design/assets/app.css`, `UI-Frontend-design/assets/app.js`.
- Audit and batch spec: `docs/fixes/00-codebase-audit.md`, `docs/fixes/01-workbench-shell-layout-theme.md`.
- Core app code: `src/app/App.tsx`, `src/components/app/AppShell.tsx`, `LeftSidebar.tsx`, `TopBar.tsx`, `RightContextPanel.tsx`, `RightPanelHeader.tsx`, `BottomStatusBar.tsx`, `shellNavigation.ts`, `src/stores/navigationStore.ts`, `src/styles.css`.
- Affected feature code: `src/features/wiki/WikiView.tsx`, `src/features/exports/ExportsView.tsx`, `src/features/lint/LintView.tsx`, `src/features/settings/AppearanceSettings.tsx`, `SettingsView.tsx`.
- Settings and project DTOs: `src/types/settings.ts`, `src/stores/settingsStore.ts`, `src-tauri/src/models/settings.rs`, `src-tauri/src/services/settings_service.rs`, `src/types/project.ts`, `src/stores/projectStore.ts`, `src-tauri/src/models/project.rs`, `src-tauri/src/services/project_service.rs`.

## Clarification Status

No blocking questions remain. The spec defines scope, persistence boundaries, acceptance criteria, and no-new-dependency direction clearly enough to proceed.

## Key Decisions

- Use a first-party splitter instead of adding a pane library. This keeps the dependency graph stable and makes keyboard semantics explicit.
- Store layout preferences under `localStorage["llm-wiki-desktop.layout.v1"]`. Never write pane widths or `sidebarCollapsed` to `.app/settings.json`.
- Preserve `rightPanelOpen` as the existing open/closed state. Add pane sizes and `sidebarCollapsed` to `navigationStore`.
- Keep existing `RecentProject.openedAt`. Do not introduce `lastOpenedAt`; add a backend-derived `missing: boolean` field with serde/default compatibility for old recent-project JSON.
- Store `colorThemePreset` as a global UI preference in `GlobalSettingsFile`. Do not write it to `ProjectSettingsFile`.
- Keep `theme: light | dark | auto` as the brightness mode. `colorThemePreset` supplies a complete semantic token set for both light and dark variants, including reading tokens.
- Do not edit `UI-Frontend-design/`. It remains the authoritative reference, not source code.

## File Structure Map

**Create**

- `src/hooks/useResizablePane.ts`  
  Pointer + keyboard resize logic, clamping, localStorage-safe helpers.
- `src/hooks/useResizablePane.test.ts`  
  Unit tests for clamp, storage parsing, keyboard resizing, and pointer delta handling.
- `src/components/app/ResizableSplitter.tsx`  
  Accessible `role="separator"` handle shared by shell and feature layouts.
- `src/lib/pathDisplay.ts`  
  Pure path compaction helpers for Windows, UNC, POSIX, and CJK paths.
- `src/lib/pathDisplay.test.ts`  
  Tests for drive-letter, UNC, POSIX, CJK, and same-name path display.
- `src/lib/colorThemePresets.ts`  
  Built-in preset metadata, swatches, semantic token maps, fallback lookup, root application helper.
- `src/lib/colorThemePresets.test.ts`  
  Tests for preset fallback, required token coverage, dark/light variant resolution, and no arbitrary CSS variable input.

**Modify**

- `src/stores/navigationStore.ts`  
  Add pane size state, sidebar collapse state, reset/setters, and layout preference persistence.
- `src/components/app/AppShell.tsx`  
  Add shell splitters, CSS variables, collapsed class, and responsive rendering rules.
- `src/components/app/LeftSidebar.tsx`  
  Render expanded/collapsed variants while preserving nav accessibility and recent page behavior.
- `src/components/app/TopBar.tsx`  
  Add sidebar toggle, compact current project button, path compaction, missing recent-project state, keyboardable menu rows.
- `src/features/wiki/WikiView.tsx`  
  Add splitter between `WikiTree` and content; drive tree width from `navigationStore`.
- `src/features/exports/ExportsView.tsx`  
  Add splitter between export list/table and preview pane; keep row preview behavior unchanged.
- `src/features/lint/LintView.tsx`  
  Add splitter between issue list/summary area and `LintIssueDetails`.
- `src/features/settings/AppearanceSettings.tsx`  
  Add preset color theme selector, swatches, Markdown preview, reset-to-codex action.
- `src/features/settings/SettingsView.tsx`  
  Pass `colorThemePreset` and `onChangeColorThemePreset` into `AppearanceSettings`.
- `src/stores/settingsStore.ts`  
  Add default field, apply functions, optimistic persistence rollback, and global setting hydration.
- `src/types/settings.ts`  
  Add `ColorThemePresetId` and `Settings.colorThemePreset`.
- `src-tauri/src/models/settings.rs`  
  Add `ColorThemePresetId`, serde defaults, global file field, round-trip tests.
- `src-tauri/src/services/settings_service.rs`  
  Ensure `color_theme_preset` stays in global settings and never leaks secrets or project-only fields.
- `src/types/project.ts`  
  Add optional/required `missing: boolean` to `RecentProject` after backend support lands.
- `src-tauri/src/models/project.rs`  
  Add serde-default `missing` on `RecentProject`.
- `src-tauri/src/services/project_service.rs`  
  Mark missing recent projects during `list_recent_projects`; write `missing: false` for remembered projects.
- `src/i18n/locales/en.json`, `src/i18n/locales/zh-CN.json`  
  Add splitter labels, sidebar toggle labels, compact project menu labels, theme preset names/descriptions.
- `src/styles.css`  
  Add current width CSS variables, splitter states, collapsed sidebar classes, compact project menu classes, reading theme variables, preset preview classes, responsive splitter hiding.
- Existing tests: `src/app/App.test.tsx`, `src/components/app/appShellActions.test.tsx`, `src/test/ui-css-contracts.test.ts`, `src/features/wiki/wiki.test.tsx`, `src/features/exports/exportsView.test.tsx`, `src/features/lint/lintView.test.tsx`, Rust settings/project tests.

---

## Task 1: Layout State Model

**Files:**
- Modify: `src/stores/navigationStore.ts`
- Create: `src/hooks/useResizablePane.ts`
- Create: `src/hooks/useResizablePane.test.ts`

- [ ] **Step 1: Add failing tests for clamp and storage parsing**

Use these test cases in `src/hooks/useResizablePane.test.ts`:

```ts
import { describe, expect, it, beforeEach } from "vitest";
import {
  clampPaneWidth,
  readLayoutPreferenceSnapshot,
  sanitizeLayoutPreferences,
  writeLayoutPreferenceSnapshot,
  DEFAULT_LAYOUT_PREFERENCES,
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
```

Run: `npm run test -- src/hooks/useResizablePane.test.ts`  
Expected: FAIL because the file and helpers do not exist.

- [ ] **Step 2: Implement layout constants and pure helpers**

Add the following public shape in `src/hooks/useResizablePane.ts`:

```ts
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
```

Implement `clampPaneWidth`, `sanitizeLayoutPreferences`, `readLayoutPreferenceSnapshot`, and `writeLayoutPreferenceSnapshot`. `clampPaneWidth(Number.NaN, min, max)` must return the midpoint of `min` and `max` rounded to the nearest integer; this makes bad input deterministic.

Run: `npm run test -- src/hooks/useResizablePane.test.ts`  
Expected: PASS.

- [ ] **Step 3: Extend `navigationStore` with pane and collapse state**

Modify `src/stores/navigationStore.ts` to import the layout helpers and expose:

```ts
export interface NavigationState {
  activeView: AppView;
  rightPanelOpen: boolean;
  sidebarCollapsed: boolean;
  paneSizes: Record<ResizablePaneId, number>;
  setActiveView: (view: AppView) => void;
  setRightPanelOpen: (open: boolean) => void;
  setSidebarCollapsed: (collapsed: boolean) => void;
  toggleSidebarCollapsed: () => void;
  setPaneSize: (pane: ResizablePaneId, width: number) => void;
  resetPaneSize: (pane: ResizablePaneId) => void;
}
```

Setter rule: every layout-changing setter writes a sanitized full snapshot back to `localStorage`. `setActiveView` and `setRightPanelOpen` must not write layout preferences.

Add focused tests to the existing store test area or a new `src/stores/navigationStore.test.ts`:

```ts
import { beforeEach, describe, expect, it } from "vitest";
import { useNavigationStore } from "./navigationStore";

describe("navigationStore layout preferences", () => {
  beforeEach(() => {
    window.localStorage.clear();
    useNavigationStore.setState({
      activeView: "dashboard",
      rightPanelOpen: true,
      sidebarCollapsed: false,
      paneSizes: {
        sidebar: 240,
        rightPanel: 320,
        wikiTree: 260,
        exportsList: 360,
        lintList: 360,
      },
    });
  });

  it("persists sidebar collapse without touching active view", () => {
    useNavigationStore.getState().setActiveView("graph");
    useNavigationStore.getState().toggleSidebarCollapsed();

    expect(useNavigationStore.getState().activeView).toBe("graph");
    expect(useNavigationStore.getState().sidebarCollapsed).toBe(true);
    expect(window.localStorage.getItem("llm-wiki-desktop.layout.v1")).toContain("sidebarCollapsed");
  });

  it("clamps and persists pane size changes", () => {
    useNavigationStore.getState().setPaneSize("rightPanel", 900);
    expect(useNavigationStore.getState().paneSizes.rightPanel).toBe(520);
  });
});
```

Run: `npm run test -- src/stores/navigationStore.test.ts src/hooks/useResizablePane.test.ts`  
Expected: PASS.

---

## Task 2: Accessible Splitter Primitive

**Files:**
- Create: `src/components/app/ResizableSplitter.tsx`
- Modify: `src/hooks/useResizablePane.ts`
- Test: `src/hooks/useResizablePane.test.ts`

- [ ] **Step 1: Add pointer and keyboard resize hook**

Extend `useResizablePane.ts` with:

```ts
export interface UseResizablePaneOptions {
  value: number;
  min: number;
  max: number;
  step?: number;
  direction?: 1 | -1;
  onChange: (value: number) => void;
  onReset: () => void;
}

export interface UseResizablePaneResult {
  separatorProps: {
    onPointerDown: (event: React.PointerEvent<HTMLElement>) => void;
    onDoubleClick: () => void;
    onKeyDown: (event: React.KeyboardEvent<HTMLElement>) => void;
  };
}
```

Rules:
- Pointer movement uses `clientX` deltas.
- `direction: 1` means dragging right increases width.
- `direction: -1` means dragging right decreases width, needed for right-side panels.
- Keyboard `ArrowRight` and `ArrowLeft` adjust by `step`, default `12`.
- `Home` sets `min`, `End` sets `max`, `Enter` resets to default by calling `onReset`.
- Pointer cleanup must run on `pointerup`, `pointercancel`, and component unmount.

Add tests for `ArrowRight`, `ArrowLeft`, `Home`, `End`, and `Enter` through `ResizableSplitter` in the next step.

- [ ] **Step 2: Implement `ResizableSplitter`**

Create `src/components/app/ResizableSplitter.tsx`:

```tsx
import type { ResizablePaneId } from "../../hooks/useResizablePane";
import { useResizablePane } from "../../hooks/useResizablePane";

export interface ResizableSplitterProps {
  paneId: ResizablePaneId;
  label: string;
  min: number;
  max: number;
  value: number;
  direction?: 1 | -1;
  className?: string;
  onChange: (value: number) => void;
  onReset: () => void;
}

export function ResizableSplitter({
  paneId,
  label,
  min,
  max,
  value,
  direction = 1,
  className,
  onChange,
  onReset,
}: ResizableSplitterProps) {
  const { separatorProps } = useResizablePane({
    value,
    min,
    max,
    direction,
    onChange,
    onReset,
  });

  return (
    <div
      {...separatorProps}
      aria-label={label}
      aria-orientation="vertical"
      aria-valuemax={max}
      aria-valuemin={min}
      aria-valuenow={value}
      className={["resize-handle", className].filter(Boolean).join(" ")}
      data-pane-id={paneId}
      role="separator"
      tabIndex={0}
    />
  );
}
```

Add Rendering Library tests through shell/view tests after integration. The primitive itself does not need a separate component test if hook tests cover behavior.

- [ ] **Step 3: Add splitter CSS contract**

In `src/styles.css`, add:

```css
.resize-handle {
  position: relative;
  width: 6px;
  min-width: 6px;
  cursor: col-resize;
  background: transparent;
  outline: none;
  touch-action: none;
}

.resize-handle::before {
  position: absolute;
  inset: 0 2px;
  content: "";
  background: var(--border);
}

.resize-handle:hover::before,
.resize-handle:focus-visible::before,
.resize-handle.is-dragging::before {
  inset: 0 1px;
  background: var(--accent-border);
}

body.is-resizing-pane {
  cursor: col-resize;
  user-select: none;
}
```

Update `src/test/ui-css-contracts.test.ts`:

```ts
it("defines keyboard-visible resizable pane handles", () => {
  expect(styles).toContain(".resize-handle");
  expect(styles).toMatch(/\.resize-handle:focus-visible::before/s);
  expect(styles).toContain("body.is-resizing-pane");
});
```

Run: `npm run test -- src/hooks/useResizablePane.test.ts src/test/ui-css-contracts.test.ts`  
Expected: PASS.

---

## Task 3: Shell Splitters and Collapsible Sidebar

**Files:**
- Modify: `src/components/app/AppShell.tsx`
- Modify: `src/components/app/TopBar.tsx`
- Modify: `src/components/app/LeftSidebar.tsx`
- Modify: `src/styles.css`
- Modify: `src/i18n/locales/en.json`
- Modify: `src/i18n/locales/zh-CN.json`
- Test: `src/app/App.test.tsx`, `src/components/app/appShellActions.test.tsx`, `src/test/ui-css-contracts.test.ts`

- [ ] **Step 1: Add i18n keys**

Add English keys:

```json
{
  "shell.sidebar.collapse": "Collapse sidebar",
  "shell.sidebar.expand": "Expand sidebar",
  "shell.splitter.sidebar": "Resize sidebar",
  "shell.splitter.rightPanel": "Resize context panel"
}
```

Add Chinese keys:

```json
{
  "shell.sidebar.collapse": "收起侧边栏",
  "shell.sidebar.expand": "展开侧边栏",
  "shell.splitter.sidebar": "调整侧边栏宽度",
  "shell.splitter.rightPanel": "调整上下文面板宽度"
}
```

- [ ] **Step 2: Wire shell CSS variables in `AppShell`**

In `AppShell`, read `sidebarCollapsed`, `paneSizes`, `setPaneSize`, and `resetPaneSize` from `navigationStore`. Build a style object:

```tsx
const shellStyle = {
  "--sidebar-w-current": `${sidebarCollapsed ? 56 : paneSizes.sidebar}px`,
  "--rightpanel-w-current": `${paneSizes.rightPanel}px`,
} as React.CSSProperties;
```

Render class names:

```tsx
<div
  className={[
    "app-shell",
    rightPanelOpen ? "is-right-open" : "is-right-collapsed",
    sidebarCollapsed ? "is-sidebar-collapsed" : "",
  ].filter(Boolean).join(" ")}
  style={shellStyle}
>
```

Render the shell splitters only when they are visible:

```tsx
<LeftSidebar />
{!sidebarCollapsed ? (
  <ResizableSplitter
    paneId="sidebar"
    label={t("shell.splitter.sidebar")}
    min={PANE_WIDTH_LIMITS.sidebar.min}
    max={PANE_WIDTH_LIMITS.sidebar.max}
    value={paneSizes.sidebar}
    onChange={(value) => setPaneSize("sidebar", value)}
    onReset={() => resetPaneSize("sidebar")}
  />
) : null}
<main className="app-shell__main" id="main-content">
```

For right panel:

```tsx
{rightPanelOpen ? (
  <ResizableSplitter
    paneId="rightPanel"
    label={t("shell.splitter.rightPanel")}
    min={PANE_WIDTH_LIMITS.rightPanel.min}
    max={PANE_WIDTH_LIMITS.rightPanel.max}
    value={paneSizes.rightPanel}
    direction={-1}
    onChange={(value) => setPaneSize("rightPanel", value)}
    onReset={() => resetPaneSize("rightPanel")}
  />
) : null}
{rightPanelOpen ? <RightContextPanel /> : null}
```

- [ ] **Step 3: Update shell grid CSS**

Replace shell columns:

```css
.app-shell {
  --sidebar-w-current: var(--sidebar-w);
  --rightpanel-w-current: var(--rightpanel-w);
}

.app-shell__workbench {
  grid-template-columns: var(--sidebar-w-current) 6px minmax(0, 1fr) 6px var(--rightpanel-w-current);
}

.app-shell.is-right-collapsed .app-shell__workbench {
  grid-template-columns: var(--sidebar-w-current) 6px minmax(0, 1fr);
}

.app-shell.is-sidebar-collapsed .app-sidebar {
  width: var(--sidebar-collapsed-w);
}

.right-panel {
  width: var(--rightpanel-w-current);
}
```

In `@media (max-width: 1180px)`, hide the right-panel splitter and use the existing drawer behavior:

```css
@media (max-width: 1180px) {
  .resize-handle[data-pane-id="rightPanel"] {
    display: none;
  }

  .app-shell__workbench,
  .app-shell.is-right-collapsed .app-shell__workbench {
    grid-template-columns: var(--sidebar-w-current) 6px minmax(0, 1fr);
  }
}
```

In `@media (max-width: 820px)`, align collapsed behavior with the existing responsive rule:

```css
@media (max-width: 820px) {
  .app-shell__workbench,
  .app-shell.is-right-collapsed .app-shell__workbench {
    grid-template-columns: var(--sidebar-collapsed-w) minmax(0, 1fr);
  }

  .resize-handle[data-pane-id="sidebar"] {
    display: none;
  }
}
```

- [ ] **Step 4: Add topbar sidebar toggle**

Import `PanelLeftClose` and `PanelLeftOpen` from `lucide-react`. Read `sidebarCollapsed` and `toggleSidebarCollapsed`. Place an icon button before the project switcher:

```tsx
<button
  aria-label={sidebarCollapsed ? t("shell.sidebar.expand") : t("shell.sidebar.collapse")}
  className="icon-button"
  onClick={toggleSidebarCollapsed}
  title={sidebarCollapsed ? t("shell.sidebar.expand") : t("shell.sidebar.collapse")}
  type="button"
>
  {sidebarCollapsed ? <PanelLeftOpen aria-hidden="true" size={16} /> : <PanelLeftClose aria-hidden="true" size={16} />}
</button>
```

- [ ] **Step 5: Render collapsed sidebar cleanly**

In `LeftSidebar`, read `sidebarCollapsed`. Rules:

- Expanded: current behavior remains, including main views, workflow views, recent pages, lint count, and Agent foot.
- Collapsed: keep only icon buttons for `mainViews` and `workflowViews`, keep active `aria-current`, hide counts and labels visually, preserve `title`.
- Collapsed Agent foot: render a single icon/status button with accessible name `shell.agentTooltip`.

Use one shared nav rendering function that receives `collapsed`.

- [ ] **Step 6: Add shell tests**

Add tests to `src/app/App.test.tsx`:

```ts
it("collapses and restores the left sidebar from the top bar", () => {
  render(<App />);

  fireEvent.click(screen.getByRole("button", { name: "Collapse sidebar" }));
  expect(document.querySelector(".app-shell")).toHaveClass("is-sidebar-collapsed");
  expect(screen.getByRole("button", { name: "Dashboard" })).toHaveAttribute("aria-current", "page");

  fireEvent.click(screen.getByRole("button", { name: "Expand sidebar" }));
  expect(document.querySelector(".app-shell")).not.toHaveClass("is-sidebar-collapsed");
});

it("renders accessible shell splitters while panels are visible", () => {
  render(<App />);

  expect(screen.getByRole("separator", { name: "Resize sidebar" })).toHaveAttribute("aria-valuemin", "180");
  expect(screen.getByRole("separator", { name: "Resize context panel" })).toHaveAttribute("aria-valuemax", "520");
});
```

Run: `npm run test -- src/app/App.test.tsx src/components/app/appShellActions.test.tsx src/test/ui-css-contracts.test.ts`  
Expected: PASS.

---

## Task 4: Wiki, Exports, and Lint Internal Splitters

**Files:**
- Modify: `src/features/wiki/WikiView.tsx`
- Modify: `src/features/exports/ExportsView.tsx`
- Modify: `src/features/lint/LintView.tsx`
- Modify: `src/styles.css`
- Modify: `src/i18n/locales/en.json`
- Modify: `src/i18n/locales/zh-CN.json`
- Test: `src/features/wiki/wiki.test.tsx`, `src/features/exports/exportsView.test.tsx`, `src/features/lint/lintView.test.tsx`

- [ ] **Step 1: Add i18n keys**

English:

```json
{
  "shell.splitter.wikiTree": "Resize wiki tree",
  "shell.splitter.exportsList": "Resize export list",
  "shell.splitter.lintList": "Resize lint issue list"
}
```

Chinese:

```json
{
  "shell.splitter.wikiTree": "调整 Wiki 文件树宽度",
  "shell.splitter.exportsList": "调整导出列表宽度",
  "shell.splitter.lintList": "调整 Lint 问题列表宽度"
}
```

- [ ] **Step 2: Wire Wiki tree width**

In `WikiView`, read `paneSizes`, `setPaneSize`, `resetPaneSize`. Add layout style:

```tsx
const layoutStyle = {
  "--wiki-tree-w-current": `${paneSizes.wikiTree}px`,
} as React.CSSProperties;
```

Apply to root:

```tsx
<div className="wiki-view-layout" style={layoutStyle}>
```

Place splitter after `WikiTree` or the empty tree placeholder:

```tsx
<ResizableSplitter
  paneId="wikiTree"
  label={t("shell.splitter.wikiTree")}
  min={PANE_WIDTH_LIMITS.wikiTree.min}
  max={PANE_WIDTH_LIMITS.wikiTree.max}
  value={paneSizes.wikiTree}
  onChange={(value) => setPaneSize("wikiTree", value)}
  onReset={() => resetPaneSize("wikiTree")}
/>
```

CSS:

```css
.wiki-tree {
  width: var(--wiki-tree-w-current, 260px);
}
```

- [ ] **Step 3: Wire Exports list width**

In `ExportsView`, add:

```tsx
const layoutStyle = {
  "--exports-list-w-current": `${paneSizes.exportsList}px`,
} as React.CSSProperties;
```

Render:

```tsx
<div className="exports-view-layout" style={layoutStyle}>
  <div className="exports-view__list-pane">
    ...
  </div>
  <ResizableSplitter ... paneId="exportsList" label={t("shell.splitter.exportsList")} />
  <aside className="exports-view__preview-pane">
```

CSS:

```css
.exports-view-layout {
  grid-template-columns: var(--exports-list-w-current, 360px) 6px minmax(320px, 1fr);
}

.exports-view__list-pane,
.exports-view__preview-pane {
  min-width: 0;
  min-height: 0;
}
```

Keep the existing table, preview, clear-preview behavior, export generation dialog, and task handling unchanged.

- [ ] **Step 4: Wire Lint issue list width**

In `LintView`, add:

```tsx
const layoutStyle = {
  "--lint-list-w-current": `${paneSizes.lintList}px`,
} as React.CSSProperties;
```

Render:

```tsx
<div className="lint-view-layout" style={layoutStyle}>
  <div className="lint-view__list-pane">
    ...
  </div>
  <ResizableSplitter ... paneId="lintList" label={t("shell.splitter.lintList")} />
  <LintIssueDetails ... />
</div>
```

CSS:

```css
.lint-view-layout {
  grid-template-columns: var(--lint-list-w-current, 360px) 6px minmax(320px, 1fr);
}

.lint-view__list-pane {
  display: flex;
  min-width: 0;
  min-height: 0;
  flex-direction: column;
  border-right: 1px solid var(--border);
}
```

- [ ] **Step 5: Preserve stacked responsive layouts**

In `@media (max-width: 980px)`, keep Exports and Lint stacked and hide their splitters:

```css
@media (max-width: 980px) {
  .exports-view-layout,
  .lint-view-layout {
    grid-template-columns: minmax(0, 1fr);
    grid-template-rows: minmax(280px, 1fr) minmax(260px, 1fr);
    overflow-y: auto;
  }

  .resize-handle[data-pane-id="exportsList"],
  .resize-handle[data-pane-id="lintList"] {
    display: none;
  }
}
```

- [ ] **Step 6: Add view tests**

Add tests that only assert accessible splitter presence and pane-specific aria limits, not pixel layout:

```ts
expect(screen.getByRole("separator", { name: "Resize wiki tree" })).toHaveAttribute("aria-valuemin", "220");
expect(screen.getByRole("separator", { name: "Resize export list" })).toHaveAttribute("aria-valuemax", "480");
expect(screen.getByRole("separator", { name: "Resize lint issue list" })).toHaveAttribute("aria-valuenow", "360");
```

Run:

```powershell
npm run test -- src/features/wiki/wiki.test.tsx src/features/exports/exportsView.test.tsx src/features/lint/lintView.test.tsx src/test/ui-css-contracts.test.ts
```

Expected: PASS.

---

## Task 5: Compact Project Switcher

**Files:**
- Create: `src/lib/pathDisplay.ts`
- Create: `src/lib/pathDisplay.test.ts`
- Modify: `src/components/app/TopBar.tsx`
- Modify: `src/types/project.ts`
- Modify: `src-tauri/src/models/project.rs`
- Modify: `src-tauri/src/services/project_service.rs`
- Modify: `src/styles.css`
- Modify: `src/i18n/locales/en.json`
- Modify: `src/i18n/locales/zh-CN.json`
- Test: `src/app/App.test.tsx`, Rust project model/service tests

- [ ] **Step 1: Implement `compactPath` tests**

Create `src/lib/pathDisplay.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { compactPath } from "./pathDisplay";

describe("compactPath", () => {
  it("keeps short paths unchanged", () => {
    expect(compactPath("D:/wiki")).toBe("D:/wiki");
  });

  it("compacts Windows drive paths", () => {
    expect(compactPath("D:/Users/Aletta/Documents/wiki/agent-llm")).toBe("D:/.../wiki/agent-llm");
  });

  it("compacts UNC paths without losing server and share", () => {
    expect(compactPath("//server/share/team/wiki/project")).toBe("//server/share/.../wiki/project");
  });

  it("compacts POSIX paths", () => {
    expect(compactPath("/Users/aletta/Documents/wiki/agent-llm")).toBe("/.../wiki/agent-llm");
  });

  it("preserves CJK leaf names", () => {
    expect(compactPath("D:/知识库/研究/智能体项目")).toBe("D:/知识库/研究/智能体项目");
  });
});
```

Run: `npm run test -- src/lib/pathDisplay.test.ts`  
Expected: FAIL until helper exists.

- [ ] **Step 2: Add path display helper**

Create `src/lib/pathDisplay.ts` with deterministic separators:

```ts
export function compactPath(path: string, maxSegments = 3): string {
  const normalized = path.replaceAll("\\", "/").replace(/\/+/g, "/");
  if (!normalized) return "";

  const unc = normalized.startsWith("//");
  const parts = normalized.split("/").filter(Boolean);
  if (unc && parts.length <= maxSegments + 2) return `//${parts.join("/")}`;
  if (!unc && parts.length <= maxSegments) return normalized;

  const drive = /^[A-Za-z]:$/.test(parts[0] ?? "") ? parts[0] : null;
  if (unc) {
    const [server, share, ...rest] = parts;
    const tail = rest.slice(-Math.max(1, maxSegments - 1));
    return `//${server}/${share}/.../${tail.join("/")}`;
  }
  if (drive) {
    const tail = parts.slice(1).slice(-Math.max(1, maxSegments - 1));
    return `${drive}/.../${tail.join("/")}`;
  }
  const tail = parts.slice(-Math.max(1, maxSegments - 1));
  return normalized.startsWith("/") ? `/.../${tail.join("/")}` : `.../${tail.join("/")}`;
}
```

Run: `npm run test -- src/lib/pathDisplay.test.ts`  
Expected: PASS.

- [ ] **Step 3: Add recent-project missing state in Rust**

Modify `src-tauri/src/models/project.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RecentProject {
    pub project_id: String,
    pub name: String,
    pub root_path: String,
    pub template: ProjectTemplate,
    pub opened_at: String,
    #[serde(default)]
    pub missing: bool,
}
```

In `ProjectService::list_recent_projects`, after loading the file:

```rust
Ok(file
    .projects
    .into_iter()
    .map(|mut project| {
        project.missing = !Path::new(&project.root_path).exists();
        project
    })
    .collect())
```

In every `RecentProject` construction, set `missing: false`.

Add Rust tests:

```rust
#[test]
fn recent_project_missing_defaults_to_false_for_legacy_json() {
    let raw = serde_json::json!({
        "projectId": "p",
        "name": "Project",
        "rootPath": "D:/missing",
        "template": "general",
        "openedAt": "2026-07-04T00:00:00Z"
    });
    let project: RecentProject = serde_json::from_value(raw).unwrap();
    assert!(!project.missing);
}
```

Run: `cargo test --manifest-path src-tauri/Cargo.toml project::tests::recent_project_missing_defaults_to_false_for_legacy_json`  
Expected: PASS if local Rust test runner is healthy. If Windows returns loader `0xc0000139`, run `cargo check --manifest-path src-tauri/Cargo.toml --lib --tests` and record the known environment gotcha from `SPEC/gotchas.txt`.

- [ ] **Step 4: Update TypeScript DTO**

Modify `src/types/project.ts`:

```ts
export interface RecentProject {
  projectId: string;
  name: string;
  rootPath: string;
  template: ProjectTemplate;
  openedAt: string;
  missing: boolean;
}
```

Update test fixtures that construct `RecentProject` to include `missing: false`.

- [ ] **Step 5: Update TopBar project button and menu**

In `TopBar.tsx`, import `compactPath`. Render current project as:

```tsx
<span className="app-topbar__project-text">
  <span className="app-topbar__project-name">{currentProject.name}</span>
  <span className="app-topbar__project-path" title={currentProject.rootPath}>
    {compactPath(currentProject.rootPath)}
  </span>
</span>
```

Recent menu row shape:

```tsx
<button
  key={rp.projectId}
  aria-disabled={rp.missing ? "true" : undefined}
  role="menuitem"
  className={`app-topbar__project-menu-row ${rp.missing ? "is-missing" : ""}`}
  onClick={() => {
    if (rp.missing) return;
    setMenuOpen(false);
    void openProject(rp.rootPath);
  }}
  type="button"
>
  <FolderOpen aria-hidden="true" size={14} />
  <span className="app-topbar__project-menu-copy">
    <span className="app-topbar__project-menu-name">{rp.name}</span>
    <span className="app-topbar__project-menu-path" title={rp.rootPath}>
      {compactPath(rp.rootPath)}
    </span>
  </span>
  <span className="app-topbar__project-menu-meta">
    {rp.missing ? t("shell.projectMenu.missing") : formatOpenedAt(rp.openedAt)}
  </span>
</button>
```

Add `formatOpenedAt(iso: string): string` in `TopBar.tsx` using `new Date(iso).toLocaleDateString()` and fallback to `iso` for invalid dates.

- [ ] **Step 6: Add keyboard menu behavior**

Keep current click-outside behavior. Add:

- `Escape` closes the menu and returns focus to the project button.
- `ArrowDown` on the project button opens menu and focuses first enabled row.
- `ArrowUp`/`ArrowDown` inside menu moves between enabled rows.
- `Enter`/`Space` activates the focused row.

Implement with refs to the project button and menu item buttons. Missing rows stay focusable only if they provide a remove action in a later batch; in this batch they are skipped by arrow navigation.

- [ ] **Step 7: Add CSS**

```css
.app-topbar__project-button {
  width: min(300px, 28vw);
}

.app-topbar__project-text,
.app-topbar__project-menu-copy {
  display: grid;
  min-width: 0;
}

.app-topbar__project-name,
.app-topbar__project-menu-name {
  overflow: hidden;
  color: var(--text-primary);
  font-size: 13px;
  font-weight: 500;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.app-topbar__project-menu-row {
  display: grid;
  grid-template-columns: 16px minmax(0, 1fr) auto;
  width: 100%;
  align-items: center;
  gap: var(--sp-2);
  border-radius: var(--radius-sm);
  padding: 6px var(--sp-2);
  text-align: left;
}

.app-topbar__project-menu-row:hover,
.app-topbar__project-menu-row:focus-visible {
  background: var(--surface-muted);
}

.app-topbar__project-menu-row.is-missing {
  color: var(--text-disabled);
}
```

- [ ] **Step 8: Add tests**

Update `src/app/App.test.tsx`:

```ts
it("shows compact project paths in the topbar and full path in title", () => {
  render(<App />);
  const switcher = screen.getByRole("button", { name: "Switch project" });
  expect(within(switcher).getByText("D:/.../wiki/agent-llm")).toHaveAttribute(
    "title",
    "D:/Users/Aletta/Documents/wiki/agent-llm",
  );
});
```

Run:

```powershell
npm run test -- src/lib/pathDisplay.test.ts src/app/App.test.tsx
cargo check --manifest-path src-tauri/Cargo.toml --lib --tests
```

Expected: PASS, except known Windows loader limitations apply only to `cargo test`, not `cargo check`.

---

## Task 6: Color Theme Presets and Settings Contract

**Files:**
- Create: `src/lib/colorThemePresets.ts`
- Create: `src/lib/colorThemePresets.test.ts`
- Modify: `src/types/settings.ts`
- Modify: `src/stores/settingsStore.ts`
- Modify: `src-tauri/src/models/settings.rs`
- Modify: `src-tauri/src/services/settings_service.rs`
- Test: TS preset tests, Rust settings tests

- [ ] **Step 1: Add TypeScript preset tests**

Create `src/lib/colorThemePresets.test.ts`:

```ts
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

  it("resolves auto mode from system preference", () => {
    const preset = getColorThemePreset("codex");
    expect(resolveColorThemeVariant(preset, "auto", false).mode).toBe("light");
    expect(resolveColorThemeVariant(preset, "auto", true).mode).toBe("dark");
  });
});
```

Run: `npm run test -- src/lib/colorThemePresets.test.ts`  
Expected: FAIL until module exists.

- [ ] **Step 2: Add TypeScript settings type**

Modify `src/types/settings.ts`:

```ts
export type ColorThemePresetId =
  | "codex"
  | "paper"
  | "graphite"
  | "mint"
  | "night"
  | "highContrast";

export interface Settings {
  language: AppLanguage;
  theme: ThemePreference;
  colorThemePreset: ColorThemePresetId;
  density: DensityPreference;
  ...
}
```

Add `colorThemePreset: "codex"` to `defaultSettings`.

- [ ] **Step 3: Implement preset metadata and root application helper**

Create `src/lib/colorThemePresets.ts` with:

```ts
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
```

Preset requirements:

- `codex`: near-monochrome with teal accent matching current `:root`.
- `paper`: white/soft paper reading surface, neutral shell, low saturation.
- `graphite`: high-neutral gray shell, calm reading surface.
- `mint`: restrained teal-tinted accent, not a one-hue UI.
- `night`: dark neutral surface with teal accent.
- `highContrast`: high contrast text/borders while preserving compact Codex-like density.

Helper functions:

```ts
export function getColorThemePreset(id: string): ColorThemePreset {
  return COLOR_THEME_PRESETS.find((preset) => preset.id === id) ?? COLOR_THEME_PRESETS[0];
}

export function resolveColorThemeVariant(
  preset: ColorThemePreset,
  theme: ThemePreference,
  prefersDark: boolean,
): ColorThemeVariant {
  if (preset.id === "night") return preset.variants.dark;
  if (theme === "dark") return preset.variants.dark;
  if (theme === "auto" && prefersDark) return preset.variants.dark;
  return preset.variants.light;
}

export function applyColorThemePresetToRoot(
  presetId: string,
  theme: ThemePreference,
  root = document.documentElement,
  prefersDark = typeof window !== "undefined" &&
    typeof window.matchMedia === "function" &&
    window.matchMedia("(prefers-color-scheme: dark)").matches,
) {
  const preset = getColorThemePreset(presetId);
  const variant = resolveColorThemeVariant(preset, theme, prefersDark);
  root.dataset.colorThemePreset = preset.id;
  for (const [name, value] of Object.entries(variant.cssVars)) {
    root.style.setProperty(name, value);
  }
  root.style.colorScheme = variant.mode;
}
```

Run: `npm run test -- src/lib/colorThemePresets.test.ts`  
Expected: PASS.

- [ ] **Step 4: Apply preset from settings store**

In `src/stores/settingsStore.ts`:

- Import `applyColorThemePresetToRoot`.
- Add `COLOR_THEME_STORAGE_KEY = "llm-wiki-desktop.colorThemePreset"`.
- Add `applyColorThemePresetPreference(preset, theme)` that applies root vars and writes localStorage.
- Call it in `loadSettings`, `persistPatch` optimistic branch, saved branch, and rollback branch.
- When `theme` changes, reapply the current `colorThemePreset`.

Add a store test or extend an existing settings store test:

```ts
expect(document.documentElement.dataset.colorThemePreset).toBe("codex");
```

- [ ] **Step 5: Extend Rust settings DTOs**

In `src-tauri/src/models/settings.rs`, add:

```rust
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ColorThemePresetId {
    #[default]
    Codex,
    Paper,
    Graphite,
    Mint,
    Night,
    HighContrast,
}
```

Add field to `Settings` and `GlobalSettingsFile`:

```rust
#[serde(default)]
pub color_theme_preset: ColorThemePresetId,
```

Set default to `ColorThemePresetId::Codex`. Add it to `apply_global` and `to_global_file`. Do not add it to `ProjectSettingsFile`.

Add tests:

```rust
#[test]
fn color_theme_preset_is_global_and_legacy_safe() {
    let legacy = serde_json::json!({
        "language": "en",
        "theme": "auto",
        "contextWindow": 32000
    });

    let settings: Settings = serde_json::from_value(legacy).unwrap();
    assert_eq!(
        serde_json::to_value(settings.color_theme_preset).unwrap(),
        serde_json::json!("codex")
    );

    let global = settings.to_global_file();
    let project = settings.to_project_file();
    let global_value = serde_json::to_value(global).unwrap();
    let project_value = serde_json::to_value(project).unwrap();
    assert_eq!(global_value["colorThemePreset"], serde_json::json!("codex"));
    assert!(project_value.get("colorThemePreset").is_none());
}
```

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml settings::tests::color_theme_preset_is_global_and_legacy_safe
cargo check --manifest-path src-tauri/Cargo.toml --lib --tests
```

Expected: `cargo check` PASS. If `cargo test` hits the known Windows loader issue, record it and continue with `cargo check` as the validation proxy.

---

## Task 7: Appearance UI and Reading Tokens

**Files:**
- Modify: `src/features/settings/AppearanceSettings.tsx`
- Modify: `src/features/settings/SettingsView.tsx`
- Modify: `src/styles.css`
- Modify: `src/i18n/locales/en.json`
- Modify: `src/i18n/locales/zh-CN.json`
- Test: add `src/features/settings/AppearanceSettings.test.tsx`

- [ ] **Step 1: Expand `AppearanceSettings` props**

Use:

```ts
interface AppearanceSettingsProps {
  theme: ThemePreference;
  colorThemePreset: ColorThemePresetId;
  onChange: (theme: ThemePreference) => void;
  onChangeColorThemePreset: (preset: ColorThemePresetId) => void;
}
```

Update `SettingsView`:

```tsx
<AppearanceSettings
  theme={settings.theme}
  colorThemePreset={settings.colorThemePreset}
  onChange={(theme) => void savePatch({ theme })}
  onChangeColorThemePreset={(colorThemePreset) => void savePatch({ colorThemePreset })}
/>
```

- [ ] **Step 2: Add preset grid UI**

Render a compact preset section below the existing light/dark/auto controls:

```tsx
<div className="appearance-presets" role="radiogroup" aria-label={t("settings.appearance.colorTheme")}>
  {COLOR_THEME_PRESETS.map((preset) => {
    const selected = colorThemePreset === preset.id;
    return (
      <button
        key={preset.id}
        aria-checked={selected}
        className={`appearance-preset ${selected ? "is-selected" : ""}`}
        onClick={() => onChangeColorThemePreset(preset.id)}
        role="radio"
        type="button"
      >
        <span className="appearance-preset__copy">
          <span className="appearance-preset__name">{t(preset.labelKey)}</span>
          <span className="appearance-preset__description">{t(preset.descriptionKey)}</span>
        </span>
        <span className="appearance-preset__swatches" aria-hidden="true">
          {preset.swatches.map((swatch) => (
            <span key={swatch} style={{ background: swatch }} />
          ))}
        </span>
      </button>
    );
  })}
</div>
```

Add a Markdown preview:

```tsx
<div className="appearance-markdown-preview wiki-prose" aria-label={t("settings.appearance.markdownPreview")}>
  <h1>{t("settings.appearance.previewTitle")}</h1>
  <p>{t("settings.appearance.previewParagraph")}</p>
  <p><a href="#preview">{t("settings.appearance.previewLink")}</a></p>
  <pre><code>{`const wiki = "local-first";`}</code></pre>
</div>
```

- [ ] **Step 3: Add reading theme CSS variables**

In `:root`, add defaults:

```css
--reading-background: var(--background);
--reading-text: var(--text-secondary);
--reading-muted: var(--text-muted);
--reading-link: var(--accent-hover);
--reading-code-bg: var(--surface-muted);
--reading-border: var(--border-subtle);
```

Update reading surfaces:

```css
.wiki-prose,
.chat-prose {
  color: var(--reading-text);
}

.wiki-prose p,
.wiki-prose li,
.chat-prose blockquote {
  color: var(--reading-text);
}

.wiki-prose a,
.chat-prose a {
  color: var(--reading-link);
}

.wiki-prose code,
.chat-prose code,
.wiki-prose pre,
.chat-prose pre {
  border-color: var(--reading-border);
  background: var(--reading-code-bg);
}

.appearance-markdown-preview {
  border: 1px solid var(--reading-border);
  border-radius: var(--radius-lg);
  background: var(--reading-background);
  padding: var(--sp-4);
}
```

Keep `.html-preview__iframe` content unchanged because it is sandboxed generated HTML. Only update surrounding chrome:

```css
.html-preview,
.html-preview__frame-wrap {
  background: var(--reading-background);
}
```

- [ ] **Step 4: Add i18n keys**

English:

```json
{
  "settings.appearance.colorTheme": "Color theme",
  "settings.appearance.markdownPreview": "Markdown theme preview",
  "settings.appearance.previewTitle": "Knowledge note",
  "settings.appearance.previewParagraph": "A calm reading surface keeps Markdown, wikilinks, and code comfortable across long sessions.",
  "settings.appearance.previewLink": "Linked concept",
  "themePreset.codex.name": "Codex",
  "themePreset.codex.description": "Near-monochrome shell with restrained teal.",
  "themePreset.paper.name": "Paper",
  "themePreset.paper.description": "Soft reading tones for long Markdown sessions.",
  "themePreset.graphite.name": "Graphite",
  "themePreset.graphite.description": "Neutral gray workspace with crisp contrast.",
  "themePreset.mint.name": "Mint",
  "themePreset.mint.description": "A light teal accent without decorative color.",
  "themePreset.night.name": "Night",
  "themePreset.night.description": "Low-glare dark workspace.",
  "themePreset.highContrast.name": "High contrast",
  "themePreset.highContrast.description": "Stronger text and borders for clarity."
}
```

Chinese:

```json
{
  "settings.appearance.colorTheme": "配色主题",
  "settings.appearance.markdownPreview": "Markdown 主题预览",
  "settings.appearance.previewTitle": "知识笔记",
  "settings.appearance.previewParagraph": "安静的阅读表面让 Markdown、双链和代码在长时间阅读中保持舒适。",
  "settings.appearance.previewLink": "关联概念",
  "themePreset.codex.name": "Codex",
  "themePreset.codex.description": "近单色工作台，保留克制的 teal 强调。",
  "themePreset.paper.name": "Paper",
  "themePreset.paper.description": "适合长时间阅读 Markdown 的柔和纸感。",
  "themePreset.graphite.name": "Graphite",
  "themePreset.graphite.description": "中性灰工作区，文字对比更清晰。",
  "themePreset.mint.name": "Mint",
  "themePreset.mint.description": "轻量 teal 强调，不引入装饰性色块。",
  "themePreset.night.name": "Night",
  "themePreset.night.description": "低眩光深色工作台。",
  "themePreset.highContrast.name": "高对比",
  "themePreset.highContrast.description": "更强的文字和边框对比，提升辨识度。"
}
```

- [ ] **Step 5: Test Appearance UI**

Create `src/features/settings/AppearanceSettings.test.tsx`:

```tsx
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import "../../i18n";
import { AppearanceSettings } from "./AppearanceSettings";

describe("AppearanceSettings", () => {
  it("renders built-in color presets without arbitrary color inputs", () => {
    render(
      <AppearanceSettings
        theme="auto"
        colorThemePreset="codex"
        onChange={vi.fn()}
        onChangeColorThemePreset={vi.fn()}
      />,
    );

    expect(screen.getByRole("radio", { name: /Codex/i })).toHaveAttribute("aria-checked", "true");
    expect(screen.getAllByRole("radio").length).toBeGreaterThanOrEqual(4);
    expect(screen.queryByLabelText(/hex|rgb|hsl|css variable/i)).not.toBeInTheDocument();
  });

  it("notifies when a preset is selected", () => {
    const onChangeColorThemePreset = vi.fn();
    render(
      <AppearanceSettings
        theme="auto"
        colorThemePreset="codex"
        onChange={vi.fn()}
        onChangeColorThemePreset={onChangeColorThemePreset}
      />,
    );

    fireEvent.click(screen.getByRole("radio", { name: /Paper/i }));
    expect(onChangeColorThemePreset).toHaveBeenCalledWith("paper");
  });
});
```

Run: `npm run test -- src/features/settings/AppearanceSettings.test.tsx src/lib/colorThemePresets.test.ts`  
Expected: PASS.

---

## Task 8: Regression Coverage and Quality Gates

**Files:**
- Modify tests listed in prior tasks
- Modify: `SPEC/progress.txt`
- Modify: `SPEC/gotchas.txt` only if a subtle or recurring issue is hit

- [ ] **Step 1: Run focused frontend tests**

Run:

```powershell
npm run test -- src/hooks/useResizablePane.test.ts src/stores/navigationStore.test.ts src/lib/pathDisplay.test.ts src/lib/colorThemePresets.test.ts src/features/settings/AppearanceSettings.test.tsx src/app/App.test.tsx src/components/app/appShellActions.test.tsx src/test/ui-css-contracts.test.ts
```

Expected: PASS.

- [ ] **Step 2: Run all frontend checks**

Run:

```powershell
npm run test
npm run lint
```

Expected: PASS.

- [ ] **Step 3: Verify import paths and TypeScript build**

Run:

```powershell
npm run build
```

Expected: PASS. This validates TypeScript import paths and Vite module resolution.

- [ ] **Step 4: Confirm no unintended console logging remains**

Because `rg.exe` is blocked on this machine, use PowerShell:

```powershell
Get-ChildItem -LiteralPath src -Recurse -File | Select-String -Pattern 'console\.log'
```

Expected: no matches. `console.warn` remains acceptable only where intentionally used for fallback diagnostics and already documented in code review.

- [ ] **Step 5: Run Rust validation**

Run:

```powershell
cargo check --manifest-path src-tauri/Cargo.toml --lib --tests
```

Expected: PASS.  
Optional: run targeted `cargo test` commands for settings/project tests. If `cargo test` fails with Windows loader `0xc0000139`, cite `SPEC/gotchas.txt` and do not treat it as a code regression.

- [ ] **Step 6: Review workflow**

After implementation:

- Launch Subagent A with shared context to review design intent, logic, consistency, and integration with docs.
- Launch Subagent B with fresh context to review blind spots, missing tests, unclear behavior, and regression risk.
- If subagents are unavailable, perform both reviews manually and label them "shared-context review" and "fresh-context review".
- Fix valid issues.
- Rerun `npm run test`, `npm run lint`, `npm run build`, and `cargo check --manifest-path src-tauri/Cargo.toml --lib --tests`.

- [ ] **Step 7: Progress logging**

Append a new record to `SPEC/progress.txt` after the implementation lands:

```text
[2026-07-04] Workbench Shell/Layout/Theme — Implemented resizable panes, collapsible sidebar, compact project switcher, and preset reading themes — Layout preferences stay in localStorage; color theme preset is a global app setting.
```

Only add `SPEC/gotchas.txt` entries for subtle or recurring errors.

---

## Acceptance Criteria

### Resizable Panes

- WHEN the user drags the sidebar splitter to the right THEN the system SHALL increase the expanded sidebar width in real time until it reaches 360px.
- WHEN the user drags the sidebar splitter to the left THEN the system SHALL decrease the expanded sidebar width in real time until it reaches 180px.
- WHEN the user drags the right context splitter to the left THEN the system SHALL increase the right context panel width in real time until it reaches 520px.
- WHEN the user drags the right context splitter to the right THEN the system SHALL decrease the right context panel width in real time until it reaches 280px.
- WHEN the user refreshes the app after resizing shell panes THEN the system SHALL restore the last sidebar and right-panel widths from `localStorage`.
- WHEN the user double-clicks any splitter THEN the system SHALL reset that pane to its default width.
- WHEN a splitter has keyboard focus and the user presses `ArrowLeft` or `ArrowRight` THEN the system SHALL adjust that pane by 12px and clamp it to its allowed range.
- WHEN a splitter has keyboard focus and the user presses `Home` THEN the system SHALL set that pane to its minimum width.
- WHEN a splitter has keyboard focus and the user presses `End` THEN the system SHALL set that pane to its maximum width.
- WHEN a splitter has keyboard focus and the user presses `Enter` THEN the system SHALL reset that pane to its default width.
- WHEN the right context panel is closed THEN the system SHALL not render an invisible right-panel splitter.
- WHEN the viewport is below 1180px and the right panel behaves as a drawer THEN the system SHALL hide the right-panel splitter and keep drawer behavior intact.
- WHEN the user resizes the Wiki tree splitter THEN the system SHALL update only the Wiki tree width and leave shell sidebar/right-panel widths unchanged.
- WHEN the user resizes the Export list splitter THEN the system SHALL update only the Export records list width and leave preview controls intact.
- WHEN the user resizes the Lint list splitter THEN the system SHALL update only the Lint issue list width and leave issue details usable.

### Collapsible Sidebar

- WHEN the user clicks the topbar sidebar collapse button THEN the system SHALL collapse the left sidebar to a 56px icon rail.
- WHEN the sidebar is collapsed THEN the system SHALL keep Dashboard, Wiki, Chat, Graph, Agent, Import, Lint, and Exports reachable by mouse and keyboard.
- WHEN the sidebar is collapsed THEN the system SHALL hide recent pages, section labels, nav text, and numeric badges without causing horizontal scrolling.
- WHEN the sidebar is collapsed THEN the system SHALL provide accessible names and tooltips for icon-only controls.
- WHEN the user clicks the topbar sidebar expand button THEN the system SHALL restore full sidebar navigation, recent pages, and Agent foot display.
- WHEN the user switches language, switches project, or refreshes the app THEN the system SHALL preserve the sidebar collapsed state from `localStorage`.
- WHEN the viewport is below 820px THEN the system SHALL use the 56px icon rail and SHALL not render a sidebar resize handle.

### Compact Project Switcher

- WHEN the current project path is longer than the topbar project button can comfortably show THEN the system SHALL show a compact middle-elided path and preserve the full path in a tooltip/title.
- WHEN the current project name is visible in the topbar THEN the system SHALL prioritize the project name over the path.
- WHEN the user opens the project switcher THEN the system SHALL show recent projects with name, compact path, opened date, current badge, and missing status when applicable.
- WHEN the user clicks an available recent project THEN the system SHALL call `openProject(rootPath)` and close the menu.
- WHEN a recent project path no longer exists THEN the system SHALL mark that row as missing and SHALL not silently delete it from recent-project history.
- WHEN the project switcher menu is open and the user presses `Escape` THEN the system SHALL close the menu and return focus to the project button.
- WHEN the project switcher menu is open and the user presses arrow keys THEN the system SHALL move focus between enabled recent-project rows.
- WHEN the app is 1120px wide THEN the system SHALL keep search, task, language, settings, and project controls visible without text overlap.
- WHEN project names or paths contain CJK characters THEN the system SHALL keep text inside its container and provide the full path through tooltip/title.

### Color Theme Presets

- WHEN the user opens Appearance settings THEN the system SHALL show at least four built-in preset color themes with swatches and descriptions.
- WHEN the user selects a preset theme THEN the system SHALL apply its semantic tokens immediately to the app shell, settings preview, Wiki Markdown, Chat Markdown, and HTML preview chrome.
- WHEN the user selects a preset theme THEN the system SHALL persist `colorThemePreset` through the existing settings flow.
- WHEN the user refreshes or reopens a project THEN the system SHALL restore the saved color theme preset.
- WHEN the saved preset id is unknown or absent THEN the system SHALL fall back to `codex`.
- WHEN `theme` is `auto` and the OS dark preference changes THEN the system SHALL apply the dark or light variant for the active preset.
- WHEN a preset is active THEN the system SHALL not expose any manual hex, RGB, HSL, or arbitrary CSS-variable input.
- WHEN the preset is `codex` THEN the system SHALL remain aligned with the Codex-like near-monochrome palette and restrained teal accent.
- WHEN theme settings are saved THEN the system SHALL keep provider secrets in OS credential storage and SHALL not write secret values to settings files.
- WHEN theme settings are saved THEN the system SHALL write `colorThemePreset` to global settings and SHALL not write it to project `.app/settings.json`.

### General Safety and Quality

- WHEN implementation changes frontend code THEN the system SHALL pass `npm run test` and `npm run lint`.
- WHEN implementation changes imports, DTOs, or CSS modules THEN the system SHALL pass `npm run build`.
- WHEN implementation changes Rust DTOs or services THEN the system SHALL pass `cargo check --manifest-path src-tauri/Cargo.toml --lib --tests`.
- WHEN checks complete THEN the system SHALL verify that no unintended `console.log` remains under `src/`.
- WHEN a subtle or recurring issue is discovered THEN the system SHALL add one entry to `SPEC/gotchas.txt` using the required symptom/root-cause/avoidance format.
- WHEN the feature lands THEN the system SHALL add a progress entry to `SPEC/progress.txt` using the required date/module/summary/decision format.

## Out Of Scope For Batch 01

- Graph visual redesign and graph cache recovery.
- Chat retrieval quality and page-scoped chat.
- Lint report history persistence.
- HTML export preview maximization and browser preview.
- Project start page redesign and folder picker improvements.
- User-authored arbitrary color editors.
- Any database for wiki content.

## Execution Recommendation

Use Subagent-Driven execution for this batch:

1. Task 1-2: layout primitives and store.
2. Task 3-4: shell and internal pane integration.
3. Task 5: project switcher and backend recent-project missing state.
4. Task 6-7: settings DTO, presets, and Appearance UI.
5. Task 8: full verification and review merge.

This keeps each change reviewable and avoids making `AppShell.tsx` harder to reason about than it already is.
