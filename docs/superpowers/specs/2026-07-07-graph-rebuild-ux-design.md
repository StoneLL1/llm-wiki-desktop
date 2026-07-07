# Graph Rebuild UX Design

## Goal

Repair the Graph rebuild flow so clicking "Rebuild graph" keeps the user in the Graph view, shows an in-place rebuilding overlay on top of the previous graph, and replaces the graph in place after the build succeeds. The same work should prepare the Graph surface for a restrained visual polish pass that remains aligned with the Codex-like desktop design.

## Approved User Experience

The approved rebuild behavior is:

- Keep the previous graph visible while rebuilding.
- Place a loading overlay above the canvas with a spinner, "Rebuilding graph" copy, and task progress when available.
- Dim or soften the old graph while the overlay is active.
- Disable repeated rebuild clicks while a Graph build task is already active.
- On success, fetch the new graph data and replace the rendered graph in place.
- On failure, keep the old graph visible and show a recoverable error banner.
- Do not reload the whole app, reopen the project, or navigate back to the dashboard.

## Root Cause Findings

The current user-visible failure has two related causes:

1. `ViewErrorBoundary` treats a view-level failure as an application-level recovery action. Its retry button calls `window.location.reload()`, which clears the in-memory project state, briefly shows the project start view, and then lets the recent-project bootstrap reopen the project.
2. Graph rebuild state is only partially modeled in the Graph view. The store has `loading` and `rebuilding` states, but the UI does not expose the build task as a first-class in-view operation with progress, cancellation, repeated-click protection, and stable failure recovery.

The backend already exposes `build_graph` as a task, but graph cache miss and stale-cache flows still need to be checked so expensive Graph work does not silently run through a normal `get_graph` load path without the progress and cancellation semantics required by the product docs.

## Product Constraints

- Project content remains Markdown, JSON, and local files. No database is introduced.
- Graph cache remains `.app/graph-cache.json`.
- React does not own filesystem, Git, Agent process, or secret-storage logic.
- Long Graph work must be progress-visible, cancellable, logged, and safe to run in the background.
- `UI-Frontend-design/` is a read-only design reference.
- The Graph UI stays compact and workbench-like: panes, toolbar controls, inspector rows, and status overlays rather than landing-page or decorative styling.

## Frontend Architecture

Graph rebuild should be represented as a project-scoped task owned by `graphStore` and rendered by `GraphView`.

The store should expose:

```ts
type GraphBuildPhase = "idle" | "loading" | "rebuilding" | "succeeded" | "failed";

type GraphBuildUiState = {
  phase: GraphBuildPhase;
  taskId: string | null;
  progress: number | null;
  label: string | null;
  error: string | null;
};
```

`GraphView` should keep the current `GraphPayload` rendered while `phase === "rebuilding"` and `data` exists. A new overlay component can render the rebuild state without touching Sigma renderer internals.

`GraphControls` should derive button state from the same build state. Rebuild is disabled while a Graph build is active. The existing task drawer can remain available through an explicit "View log" action, but rebuilding should no longer force the user out of the Graph surface.

`GraphInspector` should show localized build status and task metadata when the active view is Graph. It should not become the only place where progress is visible; the canvas overlay is the primary feedback.

## Error Recovery Design

`ViewErrorBoundary` should support view-local retry. Retrying a failed Graph view should reset only the failed view boundary and allow the Graph load/rebuild path to run again.

It must not call `window.location.reload()` for ordinary view failures. Full app reload may remain as a last-resort diagnostic action if intentionally exposed elsewhere, but it should not be the default button behind "Retry" in the workbench.

Graph-specific rebuild failures should not reach the global view error boundary. They should be represented as recoverable graph errors in `graphStore`, rendered as a banner over the still-visible previous graph when one exists.

## Backend Architecture

`build_graph` should remain the explicit rebuild entry point. It should create or reuse a project-scoped GraphBuild task, emit clear progress labels, check cancellation at meaningful boundaries, and finish by writing `.app/graph-cache.json`.

`get_graph` should be audited for stale or missing cache behavior. If a cache build could be expensive, the frontend should receive a typed "build required" result and start the task path rather than doing heavy work as a hidden synchronous load.

The command and service boundaries stay:

- Tauri command resolves project context and DTOs.
- Graph service scans wiki pages, builds graph payload, and reads/writes cache.
- Task service owns progress, logs, cancellation, and terminal state.

## Visual Polish Direction

The first polish pass should improve readability without changing the graph stack.

Reference projects:

- Sigma.js and Graphology for reducer-based rendering, hover behavior, and typed graph data modeling.
- Cytoscape.js for stylesheet-like visual mapping across node types and selection states.
- Cosmos for smooth large-graph interaction, motion, and density-aware labels.
- Gephi and ForceAtlas2 practice for community colors, spatial legibility, and low-emphasis edges.

Recommended first pass:

- Size nodes by degree or weighted connectivity.
- Use restrained type/community color mapping with a sparse teal accent for active states.
- Render edges with low opacity and increased opacity only for selected neighborhoods.
- Show labels only for selected nodes, hovered nodes, search hits, and important high-degree nodes.
- Dim unrelated nodes during focus-neighbor mode instead of removing context entirely.
- Keep the canvas shell consistent with `UI-Frontend-design/assets/app.css`: compact controls, hairline borders, 13px UI text, and no decorative gradients.

## Failure Modes

- Graph task fails before cache write: old graph remains visible, overlay closes, error banner appears.
- Graph task succeeds but follow-up `get_graph` fails: old graph remains visible, error banner explains that cache refresh failed.
- User clicks rebuild repeatedly: subsequent clicks are ignored or focus the existing task state.
- User cancels rebuild: old graph remains visible, task status becomes canceled, and the rebuild button is enabled again.
- View renderer throws unexpectedly: boundary retry resets only the Graph view, not the whole app.

## Testing Strategy

- Store tests for successful rebuild, failed rebuild with existing data, failed rebuild without data, repeated rebuild clicks, and task progress updates.
- Component tests for overlay rendering, disabled rebuild button, recoverable error banner, and no project/dashboard navigation during retry.
- Error-boundary tests proving retry does not call `window.location.reload()`.
- Backend tests or integration checks for GraphBuild task progress, cancellation, cache write, and stale-cache behavior.
- Manual/E2E verification in the Tauri app for the exact reported flow.
