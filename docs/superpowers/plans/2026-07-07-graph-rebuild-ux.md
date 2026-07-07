# Graph Rebuild UX Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Graph rebuilds run as stable in-view background tasks that keep the previous graph visible, show progress, recover without full app reload, and prepare the graph surface for a restrained visual polish pass.

**Architecture:** Keep Graph data in local Markdown-derived JSON cache under `.app/graph-cache.json`. Model build state in the Graph store, render a canvas overlay in `GraphView`, make `ViewErrorBoundary` retry view-locally, and keep backend Graph work behind typed Tauri commands and TaskService.

**Tech Stack:** React 19, TypeScript, Zustand, Tailwind CSS v4 tokens in `src/styles.css`, Lucide React, sigma.js, graphology, Tauri v2 Rust services, Vitest + Testing Library, Rust tests where applicable.

## Global Constraints

- Project content stays Markdown + JSON + local files; do not introduce a database.
- Use the project folder as the source of truth: `raw/`, `wiki/`, `.app/`, `exports/`, `skills/`.
- `raw/sources/` is immutable by default and is not touched by this work.
- React UI must not own filesystem, Git, Agent process, or secret-storage logic.
- Long tasks must be cancellable, logged, progress-visible, and safe to run in the background.
- Search remains local keyword/filter search only; this plan does not add natural-language graph search.
- Match `UI-Frontend-design/dashboard.html` and `UI-Frontend-design/assets/app.css`; do not modify `UI-Frontend-design/`.
- Use `src/styles.css` tokens and absolute px text sizes; do not hardcode component hex colors.
- After implementation run `npm run test`, `npm run lint`, confirm no unintended `console.log`, and verify imports resolve.

---

## File Structure

- Modify `src/stores/graphStore.ts`: add explicit build UI state, single-flight rebuild handling, task progress subscription inputs, and recoverable failure behavior.
- Modify `src/features/graph/GraphView.tsx`: render the rebuild overlay above the existing graph, keep old graph during rebuild, and avoid escalating normal rebuild failures to the view boundary.
- Modify `src/features/graph/GraphControls.tsx`: disable rebuild while active, show compact spinner/status, and expose a log/cancel affordance if supported by existing task commands.
- Modify `src/features/graph/GraphInspector.tsx`: show localized build status and task metadata.
- Modify `src/components/app/ViewErrorBoundary.tsx`: replace full app reload retry with view-local retry.
- Modify `src/components/app/AppShell.tsx` or the local view host if needed: provide a retry key to remount only the failed view.
- Modify `src/i18n/locales/en.json` and `src/i18n/locales/zh-CN.json`: add overlay, retry, cancel, and graph task labels.
- Modify `src/styles.css`: add Graph overlay, dimmed canvas, spinner, and focused visual polish rules using existing tokens.
- Modify `src-tauri/src/commands/graph_commands.rs`: improve GraphBuild progress labels, cancellation boundaries, and stale-cache build semantics if investigation confirms hidden synchronous build.
- Modify `src-tauri/src/services/graph_service.rs`: keep graph service pure and cache-oriented; add helper return values only if needed by commands.
- Test `src/stores/graphStore.test.ts`, `src/features/graph/GraphView.test.tsx`, `src/components/app/ViewErrorBoundary.test.tsx`, `src/test/ui-css-contracts.test.ts`, and Rust graph command/service tests if the backend contract changes.

---

### Task 1: Reproduce And Lock The Failure Contract

**Files:**
- Create or modify: `src/components/app/ViewErrorBoundary.test.tsx`
- Create or modify: `src/features/graph/GraphView.test.tsx`
- Modify only if needed: test setup files already used by the project

**Interfaces:**
- Consumes: existing `ViewErrorBoundary` retry button behavior.
- Produces: failing tests that prove retry must not reload the whole app and Graph rebuild must render an overlay with old data.

- [ ] **Step 1: Add a failing error-boundary retry test**

Create or extend `src/components/app/ViewErrorBoundary.test.tsx` with:

```tsx
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ViewErrorBoundary } from "./ViewErrorBoundary";

function ThrowingView() {
  throw new Error("boom");
}

describe("ViewErrorBoundary", () => {
  it("retries locally without reloading the whole app", () => {
    const reload = vi.fn();
    Object.defineProperty(window, "location", {
      value: { ...window.location, reload },
      writable: true,
    });

    render(
      <ViewErrorBoundary>
        <ThrowingView />
      </ViewErrorBoundary>,
    );

    fireEvent.click(screen.getByRole("button", { name: /retry/i }));
    expect(reload).not.toHaveBeenCalled();
  });
});
```

- [ ] **Step 2: Add a failing Graph rebuild overlay test**

Create or extend `src/features/graph/GraphView.test.tsx` with a mocked graph store state containing existing data and `status: "rebuilding"`:

```tsx
it("keeps the previous graph visible while showing the rebuild overlay", () => {
  seedGraphStore({
    status: "rebuilding",
    data: graphPayloadWithTwoNodes(),
    buildUi: {
      phase: "rebuilding",
      taskId: "task-graph-1",
      progress: 0.5,
      label: "Building graph",
      error: null,
    },
  });

  render(<GraphView projectId="project-1" rootPath="D:\\wiki" />);

  expect(screen.getByRole("status", { name: /rebuilding graph/i })).toBeInTheDocument();
  expect(screen.getByTestId("graph-canvas-surface")).toBeInTheDocument();
  expect(screen.getByText(/50%/)).toBeInTheDocument();
});
```

If the project does not currently expose `seedGraphStore`, add a local test helper in the test file that calls `useGraphStore.setState(...)` with the required fields.

- [ ] **Step 3: Run the focused tests and confirm failure**

Run:

```powershell
npm run test -- src/components/app/ViewErrorBoundary.test.tsx src/features/graph/GraphView.test.tsx
```

Expected: FAIL. The error-boundary test fails because retry calls `window.location.reload()`. The Graph overlay test fails because the build UI state and accessible overlay do not exist yet.

---

### Task 2: Add Graph Build UI State And Single-Flight Rebuilds

**Files:**
- Modify: `src/stores/graphStore.ts`
- Create or modify: `src/stores/graphStore.test.ts`

**Interfaces:**
- Produces: `GraphBuildUiState`.
- Produces: `activeBuildTaskId` or equivalent task identity.
- Produces: rebuild behavior that keeps existing `data` on task failure.

- [ ] **Step 1: Add failing store tests**

In `src/stores/graphStore.test.ts`, add:

```ts
it("keeps existing graph data visible when rebuild fails", async () => {
  const previous = graphPayloadWithTwoNodes();
  mockInvokeBuildGraph({ taskId: "task-1" });
  mockWaitForTaskTerminal({ status: "failed", error: "build failed" });

  useGraphStore.setState({ status: "ready", data: previous, error: null });

  await useGraphStore.getState().rebuild("project-1", "D:\\wiki");

  const state = useGraphStore.getState();
  expect(state.data).toBe(previous);
  expect(state.status).toBe("ready");
  expect(state.buildUi.phase).toBe("failed");
  expect(state.buildUi.error).toContain("build failed");
});

it("does not start a second rebuild while one is active", async () => {
  mockInvokeBuildGraph({ taskId: "task-1" });
  mockWaitForTaskTerminalPending();

  const first = useGraphStore.getState().rebuild("project-1", "D:\\wiki");
  const second = useGraphStore.getState().rebuild("project-1", "D:\\wiki");

  expect(invokeBuildGraphCallCount()).toBe(1);
  await resolvePendingTaskAsSucceeded();
  await Promise.all([first, second]);
});
```

- [ ] **Step 2: Run the focused store tests and confirm failure**

Run:

```powershell
npm run test -- src/stores/graphStore.test.ts
```

Expected: FAIL because `buildUi` and single-flight rebuild behavior do not exist.

- [ ] **Step 3: Extend the store state**

In `graphStore.ts`, add:

```ts
export type GraphBuildPhase = "idle" | "loading" | "rebuilding" | "succeeded" | "failed" | "canceled";

export type GraphBuildUiState = {
  phase: GraphBuildPhase;
  taskId: string | null;
  progress: number | null;
  label: string | null;
  error: string | null;
};

const idleBuildUi: GraphBuildUiState = {
  phase: "idle",
  taskId: null,
  progress: null,
  label: null,
  error: null,
};
```

Add `buildUi: GraphBuildUiState` and `activeBuildPromise: Promise<void> | null` or an equivalent private in-flight guard to the store implementation.

- [ ] **Step 4: Update `rebuild` to preserve old data**

Implement this behavior:

```ts
if (get().buildUi.phase === "rebuilding" || get().buildUi.phase === "loading") {
  return get().activeBuildPromise ?? Promise.resolve();
}

const hasData = Boolean(get().data);
set({
  status: hasData ? "rebuilding" : "loading",
  error: null,
  buildUi: {
    phase: hasData ? "rebuilding" : "loading",
    taskId: null,
    progress: 0,
    label: "graph.status.rebuilding",
    error: null,
  },
});
```

On terminal failure, set `status` back to `"ready"` if old data exists, otherwise `"error"`. Do not clear `data`.

- [ ] **Step 5: Verify store tests**

Run:

```powershell
npm run test -- src/stores/graphStore.test.ts
```

Expected: PASS.

---

### Task 3: Render The In-View Rebuild Overlay

**Files:**
- Modify: `src/features/graph/GraphView.tsx`
- Modify: `src/features/graph/GraphControls.tsx`
- Modify: `src/features/graph/GraphInspector.tsx`
- Modify: `src/styles.css`
- Modify: `src/i18n/locales/en.json`
- Modify: `src/i18n/locales/zh-CN.json`
- Test: `src/features/graph/GraphView.test.tsx`
- Test: `src/test/ui-css-contracts.test.ts`

**Interfaces:**
- Consumes: `GraphBuildUiState` from `graphStore`.
- Produces: accessible overlay with `role="status"` and localized copy.

- [ ] **Step 1: Add i18n keys**

Add these keys in English and Chinese:

```json
{
  "graph.rebuildOverlay.title": "Rebuilding graph",
  "graph.rebuildOverlay.viewLog": "View log",
  "graph.rebuildOverlay.cancel": "Cancel rebuild",
  "graph.rebuildOverlay.failed": "Graph rebuild failed. The previous graph is still shown."
}
```

Use the existing locale file structure rather than adding a nested shape if the project keeps Graph strings flat.

- [ ] **Step 2: Add overlay markup**

In `GraphView.tsx`, render the overlay inside the canvas shell when `buildUi.phase` is `"loading"` or `"rebuilding"`:

```tsx
{isGraphBuildActive ? (
  <div className="graph-rebuild-overlay" role="status" aria-label={t("graph.rebuildOverlay.title")}>
    <RefreshCw className="graph-rebuild-overlay__spinner" aria-hidden="true" />
    <div className="graph-rebuild-overlay__title">{t("graph.rebuildOverlay.title")}</div>
    {buildUi.label ? <div className="graph-rebuild-overlay__label">{buildUi.label}</div> : null}
    {typeof buildUi.progress === "number" ? (
      <div className="graph-rebuild-overlay__progress">{Math.round(buildUi.progress * 100)}%</div>
    ) : null}
  </div>
) : null}
```

Add `data-testid="graph-canvas-surface"` to the stable canvas container used by tests.

- [ ] **Step 3: Disable duplicate rebuild controls**

In `GraphControls.tsx`, derive:

```ts
const rebuildDisabled = disabled || status === "loading" || status === "rebuilding";
```

Use the disabled state on the rebuild button and show a small spinning `RefreshCw` icon when active.

- [ ] **Step 4: Add CSS using tokens**

In `src/styles.css`:

```css
.graph-canvas-shell.is-rebuilding .sigma-container {
  opacity: 0.45;
}

.graph-rebuild-overlay {
  position: absolute;
  inset: 0;
  display: grid;
  place-content: center;
  gap: var(--sp-2);
  background: color-mix(in srgb, var(--surface) 62%, transparent);
  color: var(--text);
  text-align: center;
  pointer-events: none;
}

.graph-rebuild-overlay__spinner {
  width: 18px;
  height: 18px;
  margin: 0 auto;
  color: var(--accent);
  animation: graph-spin 900ms linear infinite;
}

@keyframes graph-spin {
  to {
    transform: rotate(360deg);
  }
}
```

Use the project's actual canvas class names if they differ; keep the same token-based behavior.

- [ ] **Step 5: Verify overlay tests**

Run:

```powershell
npm run test -- src/features/graph/GraphView.test.tsx src/test/ui-css-contracts.test.ts
```

Expected: PASS.

---

### Task 4: Make View Retry Local Instead Of Full App Reload

**Files:**
- Modify: `src/components/app/ViewErrorBoundary.tsx`
- Modify if needed: `src/components/app/AppShell.tsx`
- Test: `src/components/app/ViewErrorBoundary.test.tsx`

**Interfaces:**
- Produces: retry behavior that resets the boundary state without calling `window.location.reload()`.

- [ ] **Step 1: Implement local retry**

In `ViewErrorBoundary.tsx`, replace the reload handler with:

```tsx
private handleRetry = () => {
  this.setState((state) => ({
    error: null,
    retryKey: state.retryKey + 1,
  }));
};
```

Render children through a keyed fragment:

```tsx
return <Fragment key={this.state.retryKey}>{this.props.children}</Fragment>;
```

Add `retryKey: number` to boundary state.

- [ ] **Step 2: Keep copy generic**

Keep the existing localized "view failed to load" copy, but ensure the primary button means "retry this view", not "reload app".

- [ ] **Step 3: Verify boundary tests**

Run:

```powershell
npm run test -- src/components/app/ViewErrorBoundary.test.tsx
```

Expected: PASS.

---

### Task 5: Audit Backend Graph Build Task Semantics

**Files:**
- Modify if needed: `src-tauri/src/commands/graph_commands.rs`
- Modify if needed: `src-tauri/src/services/graph_service.rs`
- Test if existing harness supports it: `src-tauri/tests/mvp_flow.rs` or a focused graph command/service test

**Interfaces:**
- Consumes: existing `get_graph` and `build_graph` Tauri commands.
- Produces: progress-visible, cancellable GraphBuild behavior for explicit rebuilds and expensive cache-miss paths.

- [ ] **Step 1: Inspect current stale-cache behavior**

Confirm whether `get_graph` synchronously rebuilds stale/missing graph cache by reading `GraphService::resolve`.

Expected finding from prior review: stale or missing cache can be repaired inside `resolve`, which hides potentially expensive work from the task system.

- [ ] **Step 2: Add or update backend tests before changing behavior**

Add a test that calls `get_graph` with missing cache and enough wiki pages to require a build-required result if the command already has such an error contract.

If the current public contract intentionally allows synchronous repair, write the test around explicit `build_graph` progress and leave cache-miss behavior as a follow-up documented issue.

- [ ] **Step 3: Improve explicit `build_graph` progress**

In `run_graph_build`, emit labels at these boundaries:

```rust
"Scanning wiki pages"
"Building graph nodes and links"
"Writing graph cache"
"Graph cache ready"
```

Check cancellation after scan, after graph construction, and before cache write.

- [ ] **Step 4: Keep frontend compatible**

Do not change command names or DTO shapes unless the frontend tests and TypeScript types are updated in the same task.

- [ ] **Step 5: Verify backend tests**

Run the available Rust test command used by the project. If no focused Rust graph tests exist, run:

```powershell
cd src-tauri
cargo test
```

Expected: PASS.

---

### Task 6: First Visual Polish Pass

**Files:**
- Modify: `src/features/graph/graphRenderModel.ts`
- Modify: `src/features/graph/graphRenderStyle.ts`
- Modify: `src/features/graph/GraphView.tsx`
- Modify: `src/styles.css`
- Test: existing Graph render/model tests or add focused tests if absent

**Interfaces:**
- Consumes: existing graph payload nodes, edges, types, degree, selected node, focus-neighbor mode, and search state.
- Produces: clearer node hierarchy, restrained edge opacity, selective labels, and consistent selected/neighbor styles.

- [ ] **Step 1: Add render-style tests**

Add tests for:

```ts
expect(sizeForDegree(0)).toBeLessThan(sizeForDegree(8));
expect(edgeOpacity({ selected: false, neighbor: false })).toBeLessThan(edgeOpacity({ selected: true, neighbor: true }));
expect(shouldShowLabel({ selected: true })).toBe(true);
expect(shouldShowLabel({ hovered: true })).toBe(true);
expect(shouldShowLabel({ searchHit: true })).toBe(true);
```

Use the actual exported helper names in `graphRenderStyle.ts`. If helpers are not exported, extract small pure functions with tests before wiring them into Sigma reducers.

- [ ] **Step 2: Apply degree-based node sizing**

Map node degree to a bounded size range, for example 3px to 11px. Keep bounds token-free because these are canvas drawing units, not DOM spacing.

- [ ] **Step 3: Apply restrained color mapping**

Use existing CSS variables for selected/accent states where DOM styles are involved. For canvas colors, centralize the palette in `graphRenderStyle.ts` and avoid scattering hex values through components.

- [ ] **Step 4: Improve label strategy**

Show labels for selected nodes, hovered nodes, search matches, and high-degree nodes above a density-aware threshold. Hide low-value labels in dense views.

- [ ] **Step 5: Verify graph tests**

Run:

```powershell
npm run test -- src/features/graph
```

Expected: PASS.

---

### Task 7: Full Verification And Review

**Files:**
- Modify: `progress.txt` if present or required by project process
- Modify: `gotchas.txt` only if a subtle or recurring issue was discovered

**Interfaces:**
- Produces: final verified implementation with no unintended console logging and no import failures.

- [ ] **Step 1: Run all frontend tests**

Run:

```powershell
npm run test
```

Expected: PASS.

- [ ] **Step 2: Run lint**

Run:

```powershell
npm run lint
```

Expected: PASS.

- [ ] **Step 3: Check unintended console logs**

Run:

```powershell
Select-String -Path src\**\*.ts,src\**\*.tsx -Pattern "console\.log" -SimpleMatch
```

Expected: no unintended `console.log` matches.

- [ ] **Step 4: Verify imports resolve**

Run:

```powershell
npm run test
```

Expected: PASS. The TypeScript/Vitest import graph should fail here if imports are broken.

- [ ] **Step 5: Perform required review workflow**

Launch two review subagents if available:

- Subagent A with shared context: design intent, graph rebuild logic, SPEC alignment.
- Subagent B with fresh context: blind spots, missing tests, unclear behavior.

If subagents are unavailable, perform both reviews manually and record that in the final report.

- [ ] **Step 6: Rerun all checks after fixes**

Run again from Step 1 after any review fix.

Expected: all checks PASS.

---

## Self-Review

- Spec coverage: The plan covers stable rebuild UX, local error-boundary retry, frontend task state, backend GraphBuild semantics, first visual polish, and required verification.
- Placeholder scan: No task uses forbidden placeholder wording.
- Type consistency: `GraphBuildUiState` and `GraphBuildPhase` are defined before later tasks consume them.
- Scope check: This is one cohesive Graph rebuild and polish plan. Deep graph architecture redesign is intentionally left out.
