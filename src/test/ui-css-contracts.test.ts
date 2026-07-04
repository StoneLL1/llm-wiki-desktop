/// <reference types="node" />

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

const stylesPath = join(process.cwd(), "src", "styles.css");
const styles = readFileSync(stylesPath, "utf8");

describe("responsive UI CSS contracts", () => {
  it("defines the desktop drawer and collapsed-sidebar breakpoints", () => {
    expect(styles).toContain("@media (max-width: 1180px)");
    expect(styles).toContain("@media (max-width: 820px)");
    expect(styles).toContain("@media (max-width: 760px)");
    expect(styles).toContain(".right-panel__backdrop");
  });

  it("allows dense toolbars to wrap translated labels", () => {
    expect(styles).toMatch(/\.view-toolbar\s*\{[^}]*flex-wrap:\s*wrap/s);
    expect(styles).toMatch(/\.toolbar-actions\s*\{[^}]*flex-wrap:\s*wrap/s);
    expect(styles).toMatch(/\.wiki-editor__toolbar\s*\{[^}]*flex-wrap:\s*wrap/s);
  });

  it("keeps composite card copy wrappable while compact controls stay single-line", () => {
    expect(styles).not.toMatch(/(?:^|\n)button\s*\{[^}]*white-space:\s*nowrap/s);
    expect(styles).toMatch(/\.btn\s*\{[^}]*white-space:\s*nowrap/s);
  });

  it("bounds the task drawer to the viewport and layers it above context drawers", () => {
    expect(styles).toMatch(/\.task-drawer\s*\{[^}]*z-index:\s*70[^}]*width:\s*min\(420px, 100vw\)/s);
    expect(styles).toContain(".task-drawer.is-expanded { width: min(760px, 100vw); }");
    expect(styles).toContain(".task-drawer:not(.is-expanded) .task-drawer__body { flex-direction: column; }");
    expect(styles).toMatch(/\[role="dialog"\]\[aria-modal="true"\]\s*\{\s*z-index:\s*100/s);
  });

  it("respects reduced-motion preferences", () => {
    expect(styles).toContain("@media (prefers-reduced-motion: reduce)");
  });

  it("defines keyboard-visible resizable pane handles", () => {
    expect(styles).toContain(".resize-handle");
    expect(styles).toMatch(/\.resize-handle:focus-visible::before/s);
    expect(styles).toContain("body.is-resizing-pane");
  });

  it("wires shell grid columns to persisted pane width variables", () => {
    expect(styles).toContain("--sidebar-w-current");
    expect(styles).toContain("--rightpanel-w-current");
    expect(styles).toMatch(/grid-template-columns:\s*var\(--sidebar-w-current\) 6px minmax\(0, 1fr\) 6px var\(--rightpanel-w-current\)/s);
    expect(styles).toContain(".app-shell.is-sidebar-collapsed .app-sidebar");
    expect(styles).toContain(".app-shell.is-sidebar-collapsed .app-sidebar button");
    expect(styles).not.toContain(".app-sidebar nav button");
    expect(styles).toMatch(/@media \(max-width: 820px\)[\s\S]*\.resize-handle\[data-pane-id="sidebar"\]/s);
  });

  it("wires internal view splitters to pane width variables", () => {
    expect(styles).toContain("--wiki-tree-w-current");
    expect(styles).toContain("--exports-list-w-current");
    expect(styles).toContain("--lint-list-w-current");
    expect(styles).toMatch(/\.exports-view-layout\s*\{[^}]*grid-template-columns:\s*var\(--exports-list-w-current, 360px\) 6px minmax\(320px, 1fr\)/s);
    expect(styles).toMatch(/\.lint-view-layout\s*\{[^}]*grid-template-columns:\s*var\(--lint-list-w-current, 360px\) 6px minmax\(320px, 1fr\)/s);
    expect(styles).toMatch(/@media \(max-width: 980px\)[\s\S]*\.resize-handle\[data-pane-id="exportsList"\][\s\S]*\.resize-handle\[data-pane-id="lintList"\]/s);
  });

  it("defines focused export preview and source-mode affordances", () => {
    expect(styles).toContain(".app-shell.is-workspace-focused .app-shell__workbench");
    expect(styles).toContain(".exports-view-layout.is-preview-focused");
    expect(styles).toContain('.exports-view-layout.is-preview-focused > .resize-handle[data-pane-id="exportsList"]');
    expect(styles).toContain(".export-file-cell");
    expect(styles).toContain(".export-row-actions");
    expect(styles).toContain(".segmented-control button[aria-pressed=\"true\"]");
    expect(styles).toMatch(/\.html-preview__source\s*\{[^}]*font-family:\s*var\(--font-mono\)[^}]*white-space:\s*pre-wrap/s);
  });

  it("defines compact project switcher rows", () => {
    expect(styles).toContain(".app-topbar__project-text");
    expect(styles).toContain(".app-topbar__project-menu-row");
    expect(styles).toContain(".app-topbar__project-menu-row.is-missing");
  });
});
