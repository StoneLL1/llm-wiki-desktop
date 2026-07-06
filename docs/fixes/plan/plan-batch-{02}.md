# Graph Dashboard Visuals Reliability Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver batch 02 by making the graph view visually clearer, making graph cache/loading behavior reliable across reopen/project switch/corrupt cache cases, and adding a compact dashboard graph overview aligned with the Codex-like desktop design.

**Architecture:** Treat wiki Markdown files as the source of truth, graph cache as a recoverable acceleration layer, and all graph visual decisions as deterministic pure functions. Backend `get_graph` resolves live wiki state through `GraphService::resolve`; frontend `graphStore` models load/rebuild/empty/error states without clearing usable graph data during background work; `GraphView`, `GraphLegend`, `GraphInspector`, and dashboard preview consume typed DTOs and shared helpers instead of duplicating ad hoc render rules.

**Tech Stack:** React 19, TypeScript, Vite, Tailwind CSS v4 token classes, Zustand, sigma.js, graphology, graphology-layout-forceatlas2, graphology-communities-louvain, Tauri v2, Rust services, Vitest, Testing Library, Cargo tests/checks.

---

## Read Context

This plan is based on the following files and code paths:

- `docs/fixes/00-codebase-audit.md`
- `docs/fixes/02-graph-dashboard-visuals-reliability.md`
- `SPEC/PRD.md`, `SPEC/SPEC.md`, `SPEC/APP_flow.md`, `SPEC/TECH_STACK.md`, `SPEC/BACKEND_STRUCTURE.md`, `SPEC/FRONTEND_GUIDELINES.md`, `SPEC/DESIGN.md`
- `UI-Frontend-design/dashboard.html`
- `UI-Frontend-design/graph.html`
- `UI-Frontend-design/assets/app.css`
- `UI-Frontend-design/assets/app.js`
- `src/features/graph/GraphView.tsx`
- `src/features/graph/GraphControls.tsx`
- `src/features/graph/GraphCanvasControls.tsx`
- `src/features/graph/GraphInfo.tsx`
- `src/features/graph/GraphLegend.tsx`
- `src/features/graph/GraphInspector.tsx`
- `src/features/graph/legendEntries.ts`
- `src/features/graph/graphExport.ts`
- `src/features/graph/graphNeighbors.ts`
- `src/features/dashboard/DashboardView.tsx`
- `src/layout/RightContextPanel.tsx`
- `src/stores/graphStore.ts`
- `src/stores/projectStore.ts`
- `src/stores/taskStore.ts`
- `src/stores/wikiStore.ts`
- `src/hooks/useProjectStatus.ts`
- `src/lib/waitForTaskTerminal.ts`
- `src/types/graph.ts`
- `src/types/project.ts`
- `src/types/task.ts`
- `src/styles.css`
- `src-tauri/src/commands/graph_commands.rs`
- `src-tauri/src/services/graph_service.rs`
- `src-tauri/src/services/project_service.rs`
- `src-tauri/src/services/search_service.rs`
- `src-tauri/src/models/graph.rs`
- Existing graph/dashboard tests under `src/features/graph/`, `src/features/dashboard/`, `src/app/`, and `src/test/`

## Clarification Status

No blocking clarification is required before implementation.

The batch-02 spec title says it integrates graph beautification, graph reopen reliability, and dashboard rich panels. The body fully specifies graph items A and B, but does not include a separate dashboard subsection. This plan therefore includes a bounded Dashboard task inferred from the title, the audit file, and `UI-Frontend-design/dashboard.html`: add a compact graph overview/health panel that reads existing project/graph/task state and never starts graph builds on its own.

## First Principles

The implementation should follow these invariants:

- The wiki directory is canonical. A graph cache that is missing, corrupt, stale, legacy-empty, or layout-stale must be repaired or surfaced as a recoverable state.
- A blank graph canvas is acceptable only when there are truly zero wiki pages or sigma/WebGL cannot initialize and a fallback message is visible.
- Graph visual behavior must be deterministic, typed, and testable without a live sigma renderer.
- Search, type filters, degree filters, selection, hover, focus-neighbor mode, legend hover, and export filtering must share the same visibility rules.
- Dashboard is a read-only overview surface for graph health and recent activity. It should route the user to Graph for graph actions instead of duplicating Graph controls.
- React UI must not own filesystem, Git, Agent process, or secret logic. All file-derived graph truth stays behind Tauri commands and Rust services.
- `UI-Frontend-design/` is reference material only. Do not modify it.

## Key Decisions

- Do not change the backend `GraphData` schema and do not add graph relation types.
- Change `get_graph` so it scans current wiki pages and calls `GraphService::resolve`, allowing the backend to rebuild stale/corrupt caches on read.
- Keep `build_graph` as the explicit cancellable rebuild path, but replace graphStore's 250ms polling loop with `waitForTaskTerminal`.
- Extend graph frontend state with explicit empty/rebuilding states while preserving the last usable `data` during rebuild/error transitions.
- Add `src/features/graph/graphRenderStyle.ts` and tests so sigma reducers and export logic consume shared visibility/visual helpers.
- Keep Graphology node attribute `pageType`; never write graphology node attribute `type` because sigma v3 reserves it for shape.
- Add focus-neighbor behavior through store state so `GraphInspector` and `GraphView` can coordinate from different panes.
- Add a dashboard graph preview that uses existing graph/project/task state and a deterministic mini-SVG, not sigma.
- Add i18n strings for English and Chinese for every new visible label/state.
- Use existing CSS tokens in `src/styles.css`; do not hardcode colors outside token/graph helper constants already owned by the graph module.

## File Structure Map

Create:

- `src/features/graph/graphRenderStyle.ts`
- `src/features/graph/graphRenderStyle.test.ts`
- `src/features/dashboard/dashboardGraphPreview.ts`
- `src/features/dashboard/dashboardGraphPreview.test.ts`
- `src/features/dashboard/DashboardView.test.tsx` if no dashboard component test exists

Modify:

- `src/types/graph.ts`
- `src/stores/graphStore.ts`
- `src/features/graph/GraphView.tsx`
- `src/features/graph/GraphLegend.tsx`
- `src/features/graph/GraphInspector.tsx`
- `src/features/graph/GraphControls.tsx`
- `src/features/graph/legendEntries.ts`
- `src/features/graph/graphExport.ts`
- `src/features/graph/graphView.test.tsx`
- `src/features/graph/graphStore.test.ts`
- `src/features/graph/legendEntries.test.ts`
- `src/features/graph/graphExport.test.ts`
- `src/layout/RightContextPanel.tsx`
- `src/features/dashboard/DashboardView.tsx`
- `src/styles.css`
- `src/i18n/locales/en.json`
- `src/i18n/locales/zh-CN.json`
- `src/test/ui-css-contracts.test.ts`
- `src-tauri/src/commands/graph_commands.rs`
- `src-tauri/src/services/graph_service.rs`

Do not modify:

- `UI-Frontend-design/**`
- `raw/sources/**`
- User wiki Markdown content except test fixtures created inside temp directories
- Any OS credential or project secret files

---

## Task 1 - Backend Graph Read Reliability

### Objective

Make `get_graph` a reliable read-through resolver. Opening a project or graph view should recover missing/corrupt/stale cache through the same `GraphService::resolve` path already used by service tests.

### Steps

- [ ] Add a Rust service test in `src-tauri/src/services/graph_service.rs` proving a valid-but-stale empty cache is rebuilt when live pages exist.

  Use a test name that can be run directly:

  ```rust
  #[test]
  fn resolve_rebuilds_stale_empty_cache_when_live_pages_exist() {
      let temp = tempfile::tempdir().expect("temp dir");
      let context = test_context(temp.path());
      let service = GraphService::new();
      let pages = vec![sample_page("wiki/A.md", "A", vec!["B".into()]), sample_page("wiki/B.md", "B", vec![])];

      let stale = GraphData {
          nodes: Vec::new(),
          edges: Vec::new(),
          content_hash: "stale-hash".into(),
          built_at: "2026-07-04T00:00:00Z".into(),
          layout: None,
      };
      service.write_cache(&context, &stale).expect("write stale cache");

      let result = service.resolve(&context, &pages).expect("resolve graph");

      assert!(!result.cached);
      assert!(result.layout_stale);
      assert_eq!(result.data.nodes.len(), 2);
      assert_eq!(result.data.edges.len(), 1);
      assert_eq!(result.data.content_hash, service.content_hash(&pages));
  }
  ```

- [ ] Add a Rust service test proving layout mismatch is surfaced without discarding cached topology.

  ```rust
  #[test]
  fn resolve_marks_layout_stale_when_positions_do_not_cover_nodes() {
      let temp = tempfile::tempdir().expect("temp dir");
      let context = test_context(temp.path());
      let service = GraphService::new();
      let pages = vec![sample_page("wiki/A.md", "A", vec!["B".into()]), sample_page("wiki/B.md", "B", vec![])];
      let mut data = service.rebuild(&context, &pages).expect("initial build").data;

      data.layout = Some(GraphLayout {
          positions: HashMap::from([("wiki/A.md".into(), GraphPosition { x: 1.0, y: 2.0 })]),
      });
      service.write_cache(&context, &data).expect("write partial layout");

      let result = service.resolve(&context, &pages).expect("resolve graph");

      assert!(result.cached);
      assert!(result.layout_stale);
      assert_eq!(result.data.nodes.len(), 2);
  }
  ```

- [ ] Modify `src-tauri/src/commands/graph_commands.rs::get_graph` so the command resolves against live wiki pages instead of only reading `.app/graph-cache.json`.

  Replace the cache-only flow with this shape:

  ```rust
  #[tauri::command]
  pub async fn get_graph(
      state: State<'_, AppState>,
      project_id: String,
      root_path: String,
  ) -> BackendResult<GraphBuildResult> {
      let context = state.resolve_project_context(&project_id, &root_path)?;
      let tree = state.search_service.scan_wiki(&context)?;
      state.graph_service.resolve(&context, &tree.pages)
  }
  ```

- [ ] Keep `build_graph` and `run_graph_build` as the explicit background rebuild path. Do not add a second background task inside `get_graph`; read-through repair is synchronous and bounded by current wiki scan/build costs.

- [ ] Confirm `GraphService::resolve` still:
  - returns cached data when `content_hash` matches
  - rebuilds when `content_hash` differs
  - returns `layout_stale: true` when layout is missing or incomplete
  - treats corrupt cache as recoverable missing cache

### Focused Verification

- [ ] Run:

  ```powershell
  cargo test --manifest-path src-tauri/Cargo.toml resolve_rebuilds_stale_empty_cache_when_live_pages_exist
  cargo test --manifest-path src-tauri/Cargo.toml resolve_marks_layout_stale_when_positions_do_not_cover_nodes
  cargo test --manifest-path src-tauri/Cargo.toml graph_service
  cargo check --manifest-path src-tauri/Cargo.toml --lib --tests
  ```

Expected result: the named tests pass; `cargo check` completes with no new errors.

---

## Task 2 - Graph Store State Machine And Task Waiting

### Objective

Prevent unexplained blank states by making graph load/rebuild states explicit and by keeping the last usable graph visible during rebuilds or recoverable failures.

### Steps

- [ ] Extend `GraphStatus` in `src/types/graph.ts`.

  ```ts
  export type GraphStatus =
    | "idle"
    | "loading"
    | "rebuilding"
    | "ready"
    | "ready-empty"
    | "error";
  ```

- [ ] Extend `GraphState` in `src/stores/graphStore.ts` with focus-neighbor state.

  ```ts
  focusedNodeId: string | null;
  setFocusedNodeId: (nodeId: string | null) => void;
  clearFocus: () => void;
  ```

  Behavior:

  - `setSelectedNodeId(null)` clears `focusedNodeId`.
  - `setTypeFilter`, `setDegreeThreshold`, and `setSearch` do not clear selected/focused state; the view layer decides whether the selected node is currently visible.
  - Project change resets `selectedNodeId`, `focusedNodeId`, filters, and search only through the existing project-scoped load path.

- [ ] Replace the `get_task` plus `setTimeout(250)` polling loop in `runGraphBuild` with `waitForTaskTerminal`.

  Use this shape:

  ```ts
  import { waitForTaskTerminal } from "../lib/waitForTaskTerminal";

  const terminalTask = await waitForTaskTerminal(task);
  useTaskStore.getState().upsertTask(terminalTask);

  if (terminalTask.status !== "succeeded") {
    if (!isCurrentProject(projectId, rootPath, epoch)) return;
    set((state) => ({
      status: state.data ? "ready" : "error",
      error:
        terminalTask.status === "cancelled"
          ? "Graph build was cancelled."
          : terminalTask.error ?? "Graph build failed.",
    }));
    return;
  }
  ```

- [ ] Update `load(projectId, rootPath)` behavior:
  - Start with `status: "loading"` only when no usable `data` exists.
  - If usable `data` already exists for the same project, keep rendering and set `status: "rebuilding"` only when a build is triggered.
  - On successful `get_graph`, set `status` to `"ready-empty"` when `data.nodes.length === 0`, otherwise `"ready"`.
  - Preserve `cached` and `layoutStale` from the backend result.

- [ ] Update `rebuild(projectId, rootPath)` behavior:
  - Set `status: "rebuilding"` when `data` exists.
  - Set `status: "loading"` when no graph data exists.
  - Do not clear `data` before the task finishes.
  - If the task is cancelled or fails, keep previous `data` visible when present and surface `error`.

- [ ] Keep existing `createProjectScope`, `isCurrentProject`, and `currentEpoch` guards. Every async result from `get_graph`, `build_graph`, and terminal task waiting must check project scope before mutating state.

- [ ] Add or update `src/features/graph/graphStore.test.ts`.

  Required cases:

  ```ts
  it("loads graph and maps empty data to ready-empty", async () => {
    // mock get_graph with { data: { nodes: [], edges: [], ... }, cached: true, layoutStale: false }
    // expect status === "ready-empty"
  });

  it("uses waitForTaskTerminal instead of polling get_task during rebuild", async () => {
    // mock build_graph to return a running task
    // mock waitForTaskTerminal to resolve succeeded
    // expect invoke("get_task") is not used by graphStore
  });

  it("keeps previous data visible when rebuild is cancelled", async () => {
    // seed data
    // mock terminal task as cancelled
    // expect data still equals seeded data and status === "ready"
  });

  it("ignores terminal task results from a previous project scope", async () => {
    // start rebuild for project A
    // load project B before A resolves
    // resolve A terminal task
    // expect store remains scoped to project B
  });
  ```

### Focused Verification

- [ ] Run:

  ```powershell
  npm run test -- src/features/graph/graphStore.test.ts
  ```

Expected result: graphStore tests pass and no mock relies on a 250ms polling timer.

---

## Task 3 - Pure Graph Render Style Helpers

### Objective

Move graph node/edge visibility and visual styling into pure functions so renderer behavior, legend counts, inspector focus state, and exports stay consistent.

### Steps

- [ ] Create `src/features/graph/graphRenderStyle.ts`.

  Core types:

  ```ts
  import type { GraphColorMode, GraphEdge, GraphNode } from "../../types/graph";
  import type { WikiPageType } from "../../types/wiki";

  export type GraphHiddenReason = "type" | "degree" | "search" | null;

  export interface GraphRenderOptions {
    colorMode: GraphColorMode;
    selectedNodeId: string | null;
    hoveredNodeId: string | null;
    focusedNodeId: string | null;
    search: string;
    typeFilter: Set<WikiPageType>;
    degreeThreshold: number;
    neighborIds: Set<string>;
    hoveredType: WikiPageType | null;
  }

  export interface NodeVisual {
    hidden: boolean;
    hiddenReason: GraphHiddenReason;
    color: string;
    sizeDelta: number;
    opacity: number;
    highlighted: boolean;
    borderColor?: string;
    forceLabel: boolean;
  }

  export interface EdgeVisual {
    hidden: boolean;
    color: string;
    size: number;
    opacity: number;
  }
  ```

- [ ] Implement deterministic helpers:

  ```ts
  export function graphSearchMatches(node: GraphNode, search: string): boolean {
    const query = search.trim().toLocaleLowerCase();
    if (!query) return true;
    return (
      node.label.toLocaleLowerCase().includes(query) ||
      node.path.toLocaleLowerCase().includes(query) ||
      node.tags.some((tag) => tag.toLocaleLowerCase().includes(query))
    );
  }

  export function hiddenReasonForNode(
    node: GraphNode,
    options: Pick<GraphRenderOptions, "typeFilter" | "degreeThreshold" | "search">,
  ): GraphHiddenReason {
    if (options.typeFilter.size > 0 && !options.typeFilter.has(node.type)) return "type";
    if (node.degree < options.degreeThreshold) return "degree";
    if (!graphSearchMatches(node, options.search)) return "search";
    return null;
  }
  ```

  Visual rules:

  - type/degree/search non-matches set `hidden: true` with the correct `hiddenReason`
  - selected node gets `sizeDelta: 2`, `borderColor: "#0d9488"` or the graph selected constant, and `highlighted: true`
  - selected/focused neighbor nodes have `opacity: 1`
  - non-neighbor nodes while selected/focused have `opacity: 0.16`
  - hovered type keeps matching nodes at full opacity and dims non-matching visible nodes
  - search hits use `forceLabel: true`
  - edge visual hides edges connected to hidden nodes
  - selected/hovered/focused neighbor edges use teal color and larger size
  - non-neighbor edges while selected/focused use low opacity

- [ ] Add a helper for export-visible graph filtering:

  ```ts
  export function visibleNodeIdsForExport(
    nodes: GraphNode[],
    options: Pick<GraphRenderOptions, "typeFilter" | "degreeThreshold" | "search">,
  ): Set<string> {
    return new Set(
      nodes
        .filter((node) => hiddenReasonForNode(node, options) === null)
        .map((node) => node.id),
    );
  }
  ```

- [ ] Update `src/features/graph/graphExport.ts` to call `visibleNodeIdsForExport` instead of duplicating search/type/degree filtering.

- [ ] Add `src/features/graph/graphRenderStyle.test.ts`.

  Required cases:

  ```ts
  it("separates type, degree, and search hidden reasons", () => {
    // assert exact hiddenReason values
  });

  it("highlights selected node and dims non-neighbors", () => {
    // selected node sizeDelta === 2
    // neighbor opacity === 1
    // unrelated visible node opacity === 0.16
  });

  it("uses hovered type as a temporary highlight without hiding nodes", () => {
    // non-matching visible nodes opacity less than 1
    // hidden remains false when filters pass
  });

  it("exports only nodes visible after filters and search", () => {
    // visibleNodeIdsForExport excludes hiddenReason !== null
  });
  ```

- [ ] Update `src/features/graph/graphExport.test.ts` if assertion snapshots or counts change due to shared helper behavior.

### Focused Verification

- [ ] Run:

  ```powershell
  npm run test -- src/features/graph/graphRenderStyle.test.ts src/features/graph/graphExport.test.ts
  ```

Expected result: pure helper and export tests pass without constructing a sigma renderer.

---

## Task 4 - Graph View, Legend, Inspector, And Empty States

### Objective

Integrate the new render helpers into graph UI so 200-500 page graphs remain readable, selected neighborhoods are visually clear, legend counts match the canvas, and every failure/empty/loading state has an understandable surface.

### Steps

- [ ] Update `src/features/graph/GraphView.tsx` renderer state.

  Add stable maps in renderer setup:

  ```ts
  const nodeById = new Map(graphData.nodes.map((node) => [node.id, node]));
  const edgeByKey = new Map(graphData.edges.map((edge) => [`${edge.source}\u0000${edge.target}`, edge]));
  ```

  Extend `stateRef.current`:

  ```ts
  {
    hoveredNodeId: string | null;
    hoveredType: WikiPageType | null;
    selectedNodeId: string | null;
    focusedNodeId: string | null;
    search: string;
    typeFilter: Set<WikiPageType>;
    degreeThreshold: number;
    neighborIds: Set<string>;
  }
  ```

- [ ] Update the sigma renderer effect dependency from content hash only to data shape too.

  Required dependency shape:

  ```ts
  }, [data?.contentHash, data?.nodes.length, data?.edges.length]);
  ```

  This prevents a renderer from staying stale when an empty cache and rebuilt cache share component lifecycle but differ in node count.

- [ ] In `buildGraph(data)`, keep the current NaN/Infinity protection for layout save and random positions. Preserve current behavior:
  - no Graphology `type` attribute
  - `pageType` carries wiki type
  - layout fallback uses deterministic bounded values where possible
  - community detection failures fall back without crashing

- [ ] Replace inline `nodeReducer` and `edgeReducer` logic with `nodeVisualFor` and `edgeVisualFor`.

  Renderer integration shape:

  ```ts
  nodeReducer: (nodeId, attrs) => {
    const source = nodeById.get(nodeId);
    if (!source) return attrs;
    const visual = nodeVisualFor(source, currentRenderOptions());
    return {
      ...attrs,
      hidden: visual.hidden,
      color: visual.color,
      size: Math.max(1, Number(attrs.size ?? 1) + visual.sizeDelta),
      highlighted: visual.highlighted,
      forceLabel: visual.forceLabel,
      label: visual.hidden ? "" : attrs.label,
    };
  }
  ```

  Use sigma-supported attributes only. If `borderColor` or `opacity` is not directly supported by the current sigma settings, apply the strongest supported approximation:
  - selected node uses selected teal color and `highlighted: true`
  - opacity maps to dimmed color constants
  - edge opacity maps to dimmed edge color

- [ ] Add local legend hover state in `GraphView`.

  ```ts
  const [hoveredType, setHoveredType] = useState<WikiPageType | null>(null);
  ```

  When it changes:
  - update `stateRef.current.hoveredType`
  - call `renderer.refresh()`

- [ ] Update `src/features/graph/GraphLegend.tsx`.

  Props:

  ```ts
  hoveredType: WikiPageType | null;
  onTypeHover: (type: WikiPageType | null) => void;
  ```

  Behavior:
  - type entries show visible and hidden counts, for example `8 visible · 2 hidden`
  - hidden count is based on active type/degree/search rules, not only hidden type filter
  - hover/focus on a type row temporarily highlights that type in the canvas
  - keyboard focus on a legend row triggers the same highlight as pointer hover
  - color swatches still match `PAGE_TYPE_COLORS` and `COMMUNITY_PALETTE`

- [ ] Update `src/features/graph/legendEntries.ts`.

  Extend entry shape:

  ```ts
  export interface LegendEntry {
    id: string;
    label: string;
    color: string;
    count: number;
    visibleCount: number;
    hiddenCount: number;
  }
  ```

  Keep current exports stable by making `count` equal total count.

- [ ] Update `src/features/graph/GraphInspector.tsx`.

  Add props:

  ```ts
  focusedNodeId: string | null;
  onFocusNode: (nodeId: string | null) => void;
  layoutStale: boolean;
  cached: boolean;
  status: GraphStatus;
  ```

  Behavior:
  - "Open in Wiki" is the primary action for a selected node
  - "Focus neighbors" toggles `focusedNodeId` between selected node id and `null`
  - focus toggle is disabled when no node is selected
  - layout-stale state is visible but does not hide nodes
  - cache/rebuild state is shown in the existing graph status block

- [ ] Update `src/layout/RightContextPanel.tsx` to pass new graph store state/actions to `GraphInspector`.

- [ ] Update `src/features/graph/GraphControls.tsx` only for status affordances:
  - show compact rebuilding/loading state near the toolbar count
  - keep topbar height and dense layout aligned with design tokens
  - do not add marketing copy or explanatory paragraphs

- [ ] Update graph empty/failure rendering in `GraphView.tsx`.

  Required states:
  - `status === "loading"` and no `data`: loading surface
  - `status === "rebuilding"` and `data`: graph remains visible with a compact state banner
  - `status === "ready-empty"`: no-pages empty state with route to import/wiki
  - `status === "error"` and `data`: graph remains visible with retryable error banner
  - `status === "error"` and no `data`: retryable error empty state
  - sigma init failure: existing fallback remains and includes the error label

- [ ] Add i18n keys in `src/i18n/locales/en.json` and `src/i18n/locales/zh-CN.json`.

  Required key groups:

  ```json
  {
    "graph": {
      "status": {
        "rebuilding": "...",
        "layoutStale": "...",
        "cached": "...",
        "fresh": "..."
      },
      "legend": {
        "visibleHiddenCount": "..."
      },
      "inspector": {
        "focusNeighbors": "...",
        "clearFocus": "...",
        "openInWiki": "..."
      },
      "empty": {
        "noPages": "...",
        "buildCancelled": "..."
      }
    }
  }
  ```

- [ ] Update `src/styles.css`.

  Add compact classes using existing CSS variables:

  ```css
  .graph-state-banner { ... }
  .graph-legend__row { ... }
  .graph-legend__row[data-active="true"] { ... }
  .graph-inspector__actions { ... }
  ```

  CSS constraints:
  - no new color hex values if an existing token expresses the intent
  - body text remains 13px, secondary 12px, mono/muted 11px, micro labels 10.5px
  - no gradients, decorative blobs, nested cards, or hero patterns
  - text must fit in English and Chinese

- [ ] Update component tests.

  Required test cases:

  ```ts
  it("renders ready-empty graph state without constructing sigma", () => {
    // graphStore.status = "ready-empty"; data nodes []
    // expect no-pages copy
  });

  it("keeps canvas surface and shows rebuilding banner when data exists", () => {
    // graphStore.status = "rebuilding"; data has nodes
    // expect graph container and rebuilding label
  });

  it("passes focus-neighbor action from right panel inspector to graph store", async () => {
    // render shell/right panel with selected node
    // click Focus neighbors
    // expect focusedNodeId === selected node id
  });

  it("legend reports visible and hidden counts", () => {
    // type filter / search / degree threshold active
    // expect visibleHiddenCount text
  });
  ```

### Focused Verification

- [ ] Run:

  ```powershell
  npm run test -- src/features/graph/graphView.test.tsx src/features/graph/legendEntries.test.ts src/app/App.test.tsx
  npm run test -- src/test/ui-css-contracts.test.ts
  ```

Expected result: graph UI tests pass, CSS contract tests still protect absolute px sizing and token references.

---

## Task 5 - Dashboard Graph Overview Panel

### Objective

Add the dashboard portion implied by batch 02 without expanding Dashboard into a graph controller. The dashboard should summarize graph health, cache/build state, and basic topology in the compact workbench style.

### Steps

- [ ] Create `src/features/dashboard/dashboardGraphPreview.ts`.

  Model:

  ```ts
  import type { GraphData, GraphStatus } from "../../types/graph";
  import type { ProjectSummary } from "../../types/project";
  import type { TaskRecord } from "../../types/task";
  import type { WikiTree } from "../../types/wiki";

  export interface DashboardGraphPreviewModel {
    nodeCount: number;
    edgeCount: number;
    pageCount: number;
    graphState: ProjectSummary["graphState"];
    status: GraphStatus;
    activeTaskLabel: string | null;
    topTypes: Array<{ type: string; count: number }>;
    previewNodes: Array<{ id: string; label: string; type: string; x: number; y: number }>;
    previewEdges: Array<{ source: string; target: string }>;
  }
  ```

- [ ] Implement `buildDashboardGraphPreview(project, graphData, graphStatus, tasks, tree)`.

  Rules:
  - prefer live `graphData` from `useGraphStore` when available
  - fall back to `project.graphState` and `project.wikiPageCount` when graph data has not been loaded
  - use running graph task information from `tasks` but do not start a new graph task
  - compute top page types from `tree.pages` when available
  - return at most 18 preview nodes and 24 preview edges
  - assign mini-SVG coordinates deterministically from index on a bounded oval so snapshots are stable

- [ ] Add `src/features/dashboard/dashboardGraphPreview.test.ts`.

  Required cases:

  ```ts
  it("prefers loaded graph data over project summary counts", () => {
    // graphData nodes/edges win over project graphState
  });

  it("falls back to project summary before graph view has loaded", () => {
    // nodeCount from project.wikiPageCount, edgeCount 0
  });

  it("reports active graph task without starting a task", () => {
    // pass task list, assert label, no invoke mock needed
  });

  it("returns deterministic mini preview coordinates", () => {
    // same input produces same previewNodes
  });
  ```

- [ ] Modify `src/features/dashboard/DashboardView.tsx`.

  Add a dashboard section matching `UI-Frontend-design/dashboard.html` intent:
  - compact section header
  - graph status badge
  - node and edge count
  - small type distribution list
  - deterministic mini-SVG preview
  - a single "Open Graph" action that calls `setActiveView("graph")`

  Do not add:
  - graph rebuild button
  - export button
  - graph cache mutation
  - sigma renderer

- [ ] Fix the existing recent compile task selection while touching Dashboard.

  Current code uses `tasks.find((task) => task.taskType === "wiki_compile")`, which depends on store ordering. Replace it with a helper that sorts matching compile tasks by `updatedAt` descending and picks the first.

- [ ] Add or update `src/features/dashboard/DashboardView.test.tsx`.

  Required cases:

  ```ts
  it("renders graph preview counts from graph store data", () => {
    // seed graphStore data and dashboard project
    // expect nodes/edges in the graph overview
  });

  it("opens graph view from the dashboard graph panel", async () => {
    // click Open Graph
    // expect navigationStore.activeView === "graph"
  });

  it("shows graph build task state without invoking build_graph", () => {
    // seed running graph_build task
    // assert status label and no invoke call
  });
  ```

- [ ] Add dashboard CSS in `src/styles.css`.

  Suggested classes:

  ```css
  .dashboard-graph { ... }
  .dashboard-graph__metrics { ... }
  .dashboard-graph__preview { ... }
  .dashboard-graph__node { ... }
  .dashboard-graph__edge { ... }
  ```

  Keep it compact and analytical:
  - no card inside card
  - no gradient background
  - no decorative illustration
  - use existing border/surface/accent tokens

### Focused Verification

- [ ] Run:

  ```powershell
  npm run test -- src/features/dashboard/dashboardGraphPreview.test.ts src/features/dashboard/DashboardView.test.tsx
  npm run test -- src/test/ui-css-contracts.test.ts
  ```

Expected result: dashboard helper and component tests pass; Dashboard does not call `build_graph`.

---

## Task 6 - Integrated Verification And Review

### Objective

Finish with the required project checks, code review workflow, and progress logging.

### Steps

- [ ] Run focused graph/dashboard frontend tests:

  ```powershell
  npm run test -- src/features/graph/graphRenderStyle.test.ts src/features/graph/legendEntries.test.ts src/features/graph/graphExport.test.ts src/features/graph/graphNeighbors.test.ts src/features/graph/graphStore.test.ts src/features/graph/graphView.test.tsx src/features/dashboard/dashboardGraphPreview.test.ts src/features/dashboard/DashboardView.test.tsx src/test/ui-css-contracts.test.ts
  ```

- [ ] Run all frontend checks:

  ```powershell
  npm run test
  npm run lint
  npm run build
  ```

- [ ] Run all relevant Rust checks:

  ```powershell
  cargo test --manifest-path src-tauri/Cargo.toml graph_service
  cargo check --manifest-path src-tauri/Cargo.toml --lib --tests
  ```

- [ ] Confirm no unintended `console.log` remains:

  ```powershell
  Get-ChildItem -LiteralPath src -Recurse -File | Select-String -Pattern 'console\.log'
  ```

  Expected result: no matches. Existing `console.warn` in graph renderer fallback is allowed only if it remains intentional and does not leak secrets.

- [ ] Confirm import paths resolve:

  `npm run build` is the import-resolution gate for frontend TypeScript/Vite. `cargo check --manifest-path src-tauri/Cargo.toml --lib --tests` is the import/module gate for Rust backend code.

- [ ] Run the required review workflow after implementation:
  - Subagent A with shared context: review design intent, logic, consistency, integration with docs.
  - Subagent B with fresh context: review blind spots, missing tests, unclear behavior.
  - Merge both review results.
  - Fix every valid issue.
  - Rerun all checks from the beginning.
  - If subagents are unavailable, perform the two reviews manually and state that in the delivery report.

- [ ] Append a reverse-chronological entry to `SPEC/progress.txt` after implementation lands.

  Entry shape:

  ```text
  [2026-07-04] graph/dashboard batch-02 — Implemented graph visual styling, graph cache read-through recovery, and dashboard graph overview — Key decision: GraphData schema unchanged; get_graph now resolves live wiki state through GraphService::resolve; graphStore uses event-driven task waiting and preserves usable data during rebuilds.
  ```

- [ ] Add to `SPEC/gotchas.txt` only if implementation hits a subtle or recurring issue, using:

  ```text
  Symptom — Root cause — How to avoid
  ```

## Acceptance Criteria

### Backend Reliability

- WHEN `.app/graph-cache.json` is missing and wiki pages exist THEN the system SHALL rebuild graph data through `GraphService::resolve` and return a non-blank graph result.
- WHEN `.app/graph-cache.json` is corrupt and wiki pages exist THEN the system SHALL treat the cache as recoverable, rebuild it, and return graph data without crashing.
- WHEN `.app/graph-cache.json` is a legacy empty cache but wiki pages exist THEN the system SHALL rebuild it and return nodes derived from current Markdown files.
- WHEN cached `contentHash` differs from the live wiki hash THEN the system SHALL rebuild graph cache, return `cached: false`, and mark `layoutStale: true`.
- WHEN cached `contentHash` matches but layout positions are missing or incomplete THEN the system SHALL return cached topology and set `layoutStale: true`.
- WHEN graph cache is valid and layout covers all nodes THEN the system SHALL return cached graph data with `cached: true` and `layoutStale: false`.

### Graph Store And Task Flow

- WHEN Graph view loads with no existing graph data THEN the system SHALL show a loading state until `get_graph` resolves.
- WHEN Graph view loads and `get_graph` returns zero nodes because the project has no wiki pages THEN the system SHALL show a no-pages empty state instead of a blank canvas.
- WHEN a graph rebuild starts while usable graph data exists THEN the system SHALL keep the previous graph visible and show a rebuilding status banner.
- WHEN a graph rebuild task succeeds THEN the system SHALL reload graph data through `get_graph` and update `cached`, `layoutStale`, and `status`.
- WHEN a graph rebuild task is cancelled THEN the system SHALL keep previous graph data visible when present and show a recoverable cancelled/error state.
- WHEN a graph rebuild task fails THEN the system SHALL keep previous graph data visible when present and show the task error.
- WHEN the user switches projects while a graph load or rebuild is in flight THEN the system SHALL ignore late results from the previous project.
- WHEN graphStore waits for a build task THEN the system SHALL use `waitForTaskTerminal` and SHALL NOT introduce a new 250ms polling loop.

### Graph Visuals

- WHEN the graph has 200-500 pages THEN the system SHALL render readable nodes, subdued edges, compact controls, and no overlapping explanatory text panels.
- WHEN color mode is set to type THEN the system SHALL color nodes and legend swatches from the same `PAGE_TYPE_COLORS` source.
- WHEN color mode is set to community THEN the system SHALL color nodes and legend swatches from the same community palette.
- WHEN a node is selected THEN the system SHALL visually distinguish the selected node with a larger size and teal selected styling.
- WHEN a node is selected THEN the system SHALL keep neighbor nodes and connecting edges prominent while dimming unrelated visible nodes and edges.
- WHEN Focus neighbors is enabled in the inspector THEN the system SHALL keep the selected node neighborhood emphasized until the focus is cleared or selection changes.
- WHEN a type legend row is hovered or keyboard-focused THEN the system SHALL temporarily highlight nodes of that type without changing filters.
- WHEN type filters are applied THEN the system SHALL hide excluded node types from canvas and exports.
- WHEN degree threshold is applied THEN the system SHALL hide nodes below the threshold from canvas and exports.
- WHEN search text is applied THEN the system SHALL keep matching nodes discoverable and distinguish search-hidden nodes from type/degree-hidden nodes in helper logic.
- WHEN SVG or PNG export is triggered after filters/search THEN the system SHALL export only visible nodes and visible connecting edges.
- WHEN graph layout is stale THEN the system SHALL render available graph topology and show layout-stale status instead of hiding the graph.
- WHEN sigma/WebGL initialization fails THEN the system SHALL show the existing understandable fallback with retry/rebuild affordance rather than an unexplained blank surface.
- WHEN the graph has a single node and no edges THEN the system SHALL render that node and show zero edges without errors.
- WHEN graph layout contains NaN or Infinity coordinates THEN the system SHALL sanitize positions before saving layout.

### Graph Inspector And Legend

- WHEN a node is selected THEN the right inspector SHALL show node metadata, neighbors, graph status, and an "Open in Wiki" primary action.
- WHEN no node is selected THEN the inspector SHALL disable neighbor focus and avoid pretending there is a current node.
- WHEN legend counts are displayed THEN the system SHALL show visible and hidden counts derived from the same rules used by the canvas.
- WHEN CJK labels or Windows paths appear in graph labels, legend, inspector, or status messages THEN the system SHALL keep text within its container without overlap.

### Dashboard

- WHEN Dashboard opens and graph data is already loaded THEN the system SHALL show graph node count, edge count, status, and mini preview from that data.
- WHEN Dashboard opens before Graph view has loaded graph data THEN the system SHALL fall back to project summary graph state and wiki page count.
- WHEN a graph build task is running THEN Dashboard SHALL show the graph task state without starting another build task.
- WHEN the user clicks the Dashboard graph action THEN the system SHALL navigate to the Graph view.
- WHEN Dashboard shows recent compile information THEN the system SHALL use the most recently updated compile task, not an arbitrary first match.
- WHEN Dashboard renders in Chinese or English THEN the graph overview SHALL remain compact and aligned with the Codex-like shell design.

### Quality And Safety

- WHEN implementation is complete THEN `npm run test` SHALL pass.
- WHEN implementation is complete THEN `npm run lint` SHALL pass.
- WHEN implementation is complete THEN `npm run build` SHALL pass or report a pre-existing/project-initialization blocker with exact output.
- WHEN implementation is complete THEN `cargo test --manifest-path src-tauri/Cargo.toml graph_service` SHALL pass or report an environment blocker with exact output.
- WHEN implementation is complete THEN `cargo check --manifest-path src-tauri/Cargo.toml --lib --tests` SHALL pass.
- WHEN implementation is complete THEN a source scan SHALL find no unintended `console.log`.
- WHEN implementation changes graph/dashboard behavior THEN two review passes SHALL be completed before delivery, using subagents if available or manual review if subagents are unavailable.
- WHEN progress is delivered THEN `SPEC/progress.txt` SHALL contain a newest-on-top record of the implementation milestone.

## Out Of Scope

- Changing the `GraphData` backend schema.
- Adding new graph relationship types or evidence extraction.
- Introducing a database or index store for user wiki content.
- Natural-language search or LLM-backed graph answers.
- Dashboard-triggered graph rebuilds.
- Editing files under `UI-Frontend-design/`.
- Replacing or deleting files under `raw/sources/`.
- Changing markdown frontmatter semantics beyond reading existing graph fields.
- Reworking the entire app shell or theme system.
- Adding a separate graph layout editor.

## Execution Recommendation

Use subagent-driven development for implementation:

- Main agent owns backend reliability and graphStore because those changes affect project/task boundaries.
- One subagent can implement pure graph render helpers and tests.
- One subagent can implement Dashboard preview helper and component tests.
- Main agent integrates UI wiring, i18n, CSS, and final verification.

Implement in task order. The backend read-through resolver and graphStore state machine should land before visual polish so the UI never optimizes around stale blank-state behavior.
