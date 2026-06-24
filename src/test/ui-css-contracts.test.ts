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
});
