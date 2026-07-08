/// <reference types="node" />

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

const stylesPath = join(process.cwd(), "src", "styles.css");
const styles = readFileSync(stylesPath, "utf8");

/** Re-read styles.css so a test observes the current file contents even when
 *  other tests in the same run mutated the cached module-level `styles`. */
const readStyles = (): string => readFileSync(stylesPath, "utf8");

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
    expect(styles).not.toContain('.resize-handle[data-pane-id="sidebar"] { display: none; }');
    expect(styles).toMatch(/@media \(max-width: 820px\)[\s\S]*grid-template-columns:\s*var\(--sidebar-w-current\) 6px minmax\(0, 1fr\)/s);
  });

  it("uses the splitter as the single visual boundary next to the sidebar", () => {
    expect(styles).not.toMatch(/\.app-sidebar\s*\{[^}]*border-right/s);
    expect(styles).not.toMatch(/(?:^|\n)\.right-panel\s*\{[^}]*border-left/s);
    expect(styles).toMatch(/@media \(max-width: 1180px\)[\s\S]*\.right-panel\s*\{[^}]*border-left:\s*1px solid var\(--border\)/s);
    expect(styles).toMatch(/\.resize-handle::before\s*\{[^}]*width:\s*1px/s);
    expect(styles).not.toMatch(/\.resize-handle:hover::before,[\s\S]*inset:\s*0 1px/s);
  });

  it("routes accent focus and ambient states through theme tokens", () => {
    expect(styles).toContain("--accent-ring");
    expect(styles).toContain("--accent-ring-soft");
    expect(styles).toContain("--accent-ambient");
    expect(styles).not.toMatch(/rgba\(16,\s*163,\s*127/s);
    expect(styles).not.toContain("#10a37f");
    expect(styles).not.toContain("#0a7a5e");
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
    expect(styles).toContain(".col-export-actions");
    expect(styles).toContain(".export-row-actions");
    expect(styles).toContain(".segmented-control button[aria-pressed=\"true\"]");
    expect(styles).toMatch(/\.html-preview__source\s*\{[^}]*font-family:\s*var\(--font-mono\)[^}]*white-space:\s*pre-wrap/s);
  });

  it("defines compact project switcher rows", () => {
    expect(styles).toContain(".app-topbar__project-text");
    expect(styles).toContain(".app-topbar__project-menu-row");
    expect(styles).toContain(".app-topbar__project-menu-row.is-missing");
  });

  it("defines launch metadata and generated project path styles", () => {
    expect(styles).toContain(".project-path-preview");
    expect(styles).toContain(".projcard__meta");
    expect(styles).toContain(".quickaction");
  });

  it("defines chat scroll containers with shrink-safe overflow boundaries", () => {
    const css = readStyles();
    expect(css).toContain(".chat-stream-wrap");
    expect(css).toMatch(/\.chat-stream-wrap\s*\{[^}]*overflow:\s*hidden/s);
    expect(css).toMatch(/\.chat-scroll-region\s*\{[^}]*overflow-y:\s*auto/s);
    expect(css).toMatch(/\.chat-conversation\s*\{[^}]*min-height:\s*0/s);
  });

  it("defines graph rebuild overlay and spinner affordances", () => {
    const css = styles;
    expect(css).toContain(".graph-canvas.is-rebuilding");
    expect(css).toMatch(/\.graph-rebuild-overlay\s*\{[^}]*position:\s*absolute[^}]*place-content:\s*center/s);
    expect(css).toMatch(/\.graph-rebuild-overlay__spinner\s*\{[^}]*animation:\s*graph-spin/s);
    expect(css).toMatch(/\.graph-toolbar-spin\s*\{[^}]*animation:\s*graph-spin/s);
  });

  it("keeps the graph canvas visually quiet with a light grid and raised overlays", () => {
    const css = styles;
    expect(css).toMatch(/\.graph-canvas\s*\{[^}]*background-color:\s*var\(--background\)/s);
    expect(css).toMatch(/\.graph-canvas\s*\{[^}]*background-size:\s*40px 40px/s);
    expect(css).toMatch(/\.graph-info\s*\{[^}]*background:\s*color-mix/s);
    expect(css).toMatch(/\.graph-legend\s*\{[^}]*box-shadow:\s*0 8px 24px color-mix/s);
    expect(css).not.toContain("--shadow-soft");
  });
});
