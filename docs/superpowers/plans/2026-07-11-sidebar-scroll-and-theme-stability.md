# Sidebar Scroll and Theme Stability Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the upper left sidebar scroll as one region, style left/right panel scrollbars consistently, and keep settings save feedback from reflowing appearance content.

**Architecture:** `LeftSidebar` owns one scroll boundary above its fixed footer. A reusable `app-pane-scrollbar` CSS class is attached to left and right panel scroll bodies. `SettingsView` retains its status feedback but CSS positions that status absolutely inside a relative content boundary.

**Tech Stack:** React 19, TypeScript, Tailwind CSS v4 utilities, shared CSS tokens, Vitest, Testing Library.

## Global Constraints

- Do not modify `UI-Frontend-design/`.
- Keep the Agent status row fixed below the left scroll region.
- Limit compact scrollbar styling to the left sidebar and right context panel bodies.
- Keep settings loading/saving feedback accessible with `role="status"`.
- Preserve unrelated working-tree changes and stage only this task's files.

---

### Task 1: Lock the scroll and non-reflow contracts with failing tests

**Files:**
- Modify: `src/components/app/LeftSidebar.test.tsx`
- Modify: `src/test/ui-css-contracts.test.ts`

**Interfaces:**
- Consumes: existing `LeftSidebar` DOM and `src/styles.css` text.
- Produces: regression contracts for `.app-sidebar__scroll-region`, `.app-pane-scrollbar`, and absolutely positioned `.settings-view__status`.

- [ ] **Step 1: Add the sidebar structure test**

Add a test that renders `LeftSidebar`, locates Main Views, Workflow, Favorites, Recent Pages, and the Agent action, then asserts the four sections share the closest `.app-sidebar__scroll-region` while the Agent action is outside it.

- [ ] **Step 2: Add CSS contract tests**

Add assertions equivalent to:

```ts
expect(css).toMatch(/\.app-sidebar__scroll-region\s*\{[^}]*overflow-y:\s*auto/s);
expect(css).toMatch(/\.app-sidebar__recent\s*\{[^}]*flex:\s*none/s);
expect(css).toMatch(/\.app-pane-scrollbar::-webkit-scrollbar\s*\{[^}]*width:\s*4px/s);
expect(css).toMatch(/\.settings-view__content-inner\s*\{[^}]*position:\s*relative/s);
expect(css).toMatch(/\.settings-view__status\s*\{[^}]*position:\s*absolute/s);
```

Also read `RightContextPanel.tsx`, `AgentRightPanel.tsx`, and `ImportRightPanel.tsx` in the CSS contract test and assert their direct `overflow-y-auto` bodies include `app-pane-scrollbar`.

- [ ] **Step 3: Run focused tests and verify RED**

Run: `npm test -- src/components/app/LeftSidebar.test.tsx src/test/ui-css-contracts.test.ts`

Expected: FAIL because the shared scroll-region class, compact scrollbar rules, right-panel class wiring, and absolute status positioning do not exist yet.

---

### Task 2: Implement the minimal layout and scrollbar changes

**Files:**
- Modify: `src/components/app/LeftSidebar.tsx`
- Modify: `src/components/app/RightContextPanel.tsx`
- Modify: `src/features/agent/AgentRightPanel.tsx`
- Modify: `src/features/import/ImportRightPanel.tsx`
- Modify: `src/styles.css`

**Interfaces:**
- Consumes: regression contracts from Task 1 and existing theme tokens such as `--border` and `--border-strong`.
- Produces: one left scroll region, shared left/right scrollbar styling, and non-reflowing settings status feedback.

- [ ] **Step 1: Make the sidebar upper content one scroll region**

Wrap the four upper sections in:

```tsx
<div className="app-sidebar__scroll-region app-pane-scrollbar">
  {/* Main Views, Workflow, Favorites, Recent Pages */}
</div>
```

Remove `min-h-0 flex-1 overflow-y-auto` from the Recent Pages element. Keep the Agent footer as the wrapper's following sibling.

- [ ] **Step 2: Wire the right panel bodies to the shared class**

Add `app-pane-scrollbar` to each direct right-panel body that already owns `overflow-y-auto` in `RightContextPanel`, `AgentRightPanel`, and `ImportRightPanel`. Do not add it to the nested import preview `<pre>`.

- [ ] **Step 3: Add compact scrollbar CSS**

Add rules equivalent to:

```css
.app-sidebar__scroll-region {
  min-height: 0;
  flex: 1;
  overflow-y: auto;
}

.app-sidebar__recent { flex: none; }

.app-pane-scrollbar {
  scrollbar-width: thin;
  scrollbar-color: color-mix(in srgb, var(--border-strong) 35%, transparent) transparent;
}

.app-pane-scrollbar::-webkit-scrollbar { width: 4px; height: 4px; }
.app-pane-scrollbar::-webkit-scrollbar-track { background: transparent; }
.app-pane-scrollbar::-webkit-scrollbar-thumb {
  border-radius: var(--radius-pill);
  background: color-mix(in srgb, var(--border-strong) 35%, transparent);
}
.app-pane-scrollbar:hover::-webkit-scrollbar-thumb,
.app-pane-scrollbar::-webkit-scrollbar-thumb:hover {
  background: color-mix(in srgb, var(--text-muted) 58%, transparent);
}
```

- [ ] **Step 4: Remove save-feedback reflow**

Make `.settings-view__content-inner` the relative positioning boundary and set `.settings-view__status` to `position: absolute` near its upper-right edge without changing the JSX or accessibility role.

- [ ] **Step 5: Run focused tests and verify GREEN**

Run: `npm test -- src/components/app/LeftSidebar.test.tsx src/test/ui-css-contracts.test.ts`

Expected: PASS with no failures.

---

### Task 3: Verify, review, log, and commit

**Files:**
- Modify: `SPEC/progress.txt`
- Modify only if a subtle recurring issue is found: `SPEC/gotchas.txt`

**Interfaces:**
- Consumes: completed implementation and project review requirements.
- Produces: verified change, review findings, milestone log, and scoped Git commit.

- [ ] **Step 1: Run the unified check**

Run: `npm run check`

Expected: exit code 0 for tests, lint, TypeScript/Vite build, console scan, Tauri GUI Rust check, and Rust no-default-features tests.

- [ ] **Step 2: Run two review subagents**

Reviewer A receives shared context and checks design intent, logic, consistency, and docs integration. Reviewer B receives fresh context and checks blind spots, test gaps, and unclear behavior.

- [ ] **Step 3: Fix valid review findings and rerun the unified check**

Apply only in-scope fixes, rerun focused tests as needed, then rerun `npm run check` from the beginning.

- [ ] **Step 4: Add the progress milestone**

Add a newest-first entry using:

```text
[2026-07-11] Sidebar scroll and appearance stability — Unified the upper left sidebar scroll region, refined left/right panel scrollbars, and removed settings save-status reflow — Agent footer remains fixed; save feedback is absolutely positioned.
```

- [ ] **Step 5: Verify the final diff and commit only task files**

Run `git diff --check`, inspect staged paths, and commit with:

```text
fix: stabilize sidebar scrolling and appearance settings
```
