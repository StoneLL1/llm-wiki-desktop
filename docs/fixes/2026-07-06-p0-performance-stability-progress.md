# P0 Performance Stability Fix Progress and Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restore a trustworthy test baseline and remove the highest-risk P0 performance/stability hot spots identified on 2026-07-06.

**Architecture:** Keep the app local-first and file-backed: Markdown, JSON, and local files remain the source of truth. Fixes should be narrow, measurable, and aligned with the existing React/Tauri service boundaries; no database, no broad UI redesign, and no large command/service rewrite in this batch.

**Tech Stack:** React 19, TypeScript, Vite, Vitest/jsdom, Tauri v2, Rust services, local JSON/cache files, graphology/sigma.js, Milkdown, Readability, remark/rehype markdown rendering.

---

## Context And Source Review

Read before this plan:

- `AGENTS.md`
- `docs/audits/2026-07-06-performance-complexity-audit.md`

Checked existing fix docs:

- `docs/fixes/00-codebase-audit.md`
- `docs/fixes/01-workbench-shell-layout-theme.md`
- `docs/fixes/02-graph-dashboard-visuals-reliability.md`
- `docs/fixes/03-knowledge-interaction-chat-bookmarks.md`
- `docs/fixes/04-export-workflow-preview.md`
- `docs/fixes/05-project-task-health-ux.md`
- `docs/fixes/plan/plan-batch-{01}.md` through `plan-batch-{05}.md`
- `docs/fixes/plan/progress-plan-batch-{01}.md` through `progress-plan-batch-{05}.md`

No existing `docs/fixes/2026-07-06-p0-performance-stability-progress.md` was present. This file is the ongoing progress document for the P0 performance/stability repair sequence.

Current worktree warning:

- The worktree already contains unrelated uncommitted and untracked changes. Do not overwrite, roll back, reorder, or "clean up" those changes while implementing this plan.
- Treat existing changes in `SPEC/progress.txt`, `SPEC/gotchas.txt`, `src-tauri/Cargo.toml`, `src/components/app/AppShell.tsx`, `src/stores/navigationStore.ts`, `docs/audits/`, `docs/fixes/`, and `wiki/.app/` as user-owned unless the user explicitly says otherwise.

## Goals

- Make `npm run test` trustworthy again by fixing the current failing assertion and removing Graph/Sigma jsdom noise.
- Make `waitForTaskTerminal` unable to wait forever when events are missed, listener registration fails, or a task reaches terminal state before the listener is ready.
- Split the first-screen bundle so Dashboard/App shell does not statically pull Graph, Milkdown, Import/Readability, or markdown renderer dependencies.
- Add the smallest useful wiki index/cache layer to reduce repeated full Markdown scans across Search, Chat retrieval, and Graph cache freshness.
- Remove the Graph reducer O(E*N) hot path by computing render visibility/options once per refresh instead of once per edge.

## Non-Goals

- Do not introduce a database for wiki content or derived index data.
- Do not rewrite AppShell, Tauri commands, or Rust services for cleanliness in this P0 batch.
- Do not redesign the UI shell or change product behavior beyond loading/fallback states needed for lazy boundaries.
- Do not change `raw/sources/` immutability or source replacement semantics.
- Do not silently install Agent tools, new system dependencies, or browser/canvas native packages just to hide jsdom noise.
- Do not commit generated `dist/` output unless the repo policy and user explicitly require it.

## Recommended Order

1. Batch 1: Test reliability and Graph/Sigma jsdom noise.
2. Batch 2: `waitForTaskTerminal` polling, timeout, and listener-failure handling.
3. Batch 3: First-screen bundle split.
4. Batch 4: Minimal wiki index/cache for Search, Chat, and Graph freshness.
5. Batch 5: Graph reducer render snapshot optimization.

This order is intentional: every later performance fix needs a reliable test signal, and async task waiting should be safe before adding or changing long-running Graph/Search flows.

## Scope Buckets

Must fix in this P0 batch:

- `npm run test` failure and Graph/Sigma jsdom noise.
- `waitForTaskTerminal` permanent wait risk.
- Lazy load Graph, Wiki/Milkdown, Import/Readability, and markdown renderer paths.
- Minimal per-project wiki index/cache, using memory and optionally `.app/index.json`.
- Graph reducer O(E*N) repeated computation.

Can fix opportunistically if it stays small:

- Replace stale App test copy with the current accessible UI contract.
- Add a shared compact loading fallback for lazy feature views.
- Reuse one markdown renderer wrapper for Chat and Wiki if it reduces duplication without changing behavior.
- Hash page content from already-read bytes in SearchService while building the index.
- Add a small chunk-size note to this progress file after `npm run build`.

Defer to later batches:

- Task log/progress persistence throttling and log pagination.
- Chat streaming chunk batching.
- Graph tag edge budget/top-k.
- Import source replacement confirmation-before-extraction cleanup.
- Tauri command use-case extraction.
- AppShell controller hook split.
- Large Rust service/file modularization.
- Formal bundle budget, 500-page synthetic fixture, and performance CI.

## Current Baseline

From `docs/audits/2026-07-06-performance-complexity-audit.md`:

- `npm run lint`: passed in the last audit.
- `cargo check`: passed in the last audit.
- `npm run test`: failed in the last audit, with 1 failing test and repeated Graph/Sigma jsdom `HTMLCanvasElement.prototype.getContext` noise.

Known baseline evidence:

- Audit recorded `Test Files 1 failed | 47 passed` and `Tests 1 failed | 312 passed`.
- Audit recorded main bundle `dist/assets/index-Dmu_vTi_.js` at about 1.89MB and CSS at about 172KB.
- Fresh `npm run build` was intentionally not run during the audit because it rewrites `dist/` and TypeScript build metadata.

## Verification Rules For Every Implementation Batch

Run after each completed batch unless the user explicitly limits the turn to planning:

- `npm run test`
- `npm run lint`
- `cargo check --manifest-path src-tauri/Cargo.toml`
- `Get-ChildItem -Path src,src-tauri/src -Recurse -File | Select-String -Pattern 'console\.log'`
- Import/path resolution check through the commands above; use `npm run build` only for the bundle-splitting batch or when TypeScript import resolution must be proven beyond Vitest/lint.

If `npm run build` is used for bundle evidence:

- Run `git status --short` afterward.
- Record generated file churn separately.
- Do not overwrite or discard pre-existing user changes.

## Batch 1: Test Reliability And Graph/Sigma jsdom Noise

Problem:

- `npm run test` is currently not trustworthy: one App test fails and Graph/Sigma emits repeated jsdom canvas/WebGL noise.

Evidence:

- `src/test/setup.ts:9-13`: only `WebGL2RenderingContext` and `WebGLRenderingContext` are stubbed.
- `src/features/graph/GraphView.tsx:175-181`: Sigma initialization can fail and logs a warning.
- `src/features/graph/GraphView.tsx:487`: `new Sigma(graph, container, ...)` is constructed in the view.
- `src/app/App.test.tsx:454`: stale assertion searches for `Collapse sidebar`.
- `src/components/app/AppShell.tsx:11`: `GraphView` is statically imported into the App shell.

Expected modified files:

- `src/test/setup.ts`
- `src/app/App.test.tsx`
- Possibly `src/features/graph/GraphView.tsx` if test-only renderer injection is cleaner than global stubs.
- Possibly `src/components/app/AppShell.tsx` if this is paired with Batch 3 lazy import work.

Modification scope:

- First reproduce and preserve the failing signal.
- Update the stale sidebar assertion to match the current design contract only after confirming whether the manual collapse button was intentionally removed.
- Remove jsdom `HTMLCanvasElement.prototype.getContext` noise by either overriding the canvas context path in tests or by injecting/mocking the Graph renderer so App-level tests do not instantiate Sigma.
- Prefer lazy AppShell boundaries over broad mocks when they also support Batch 3.

Risk:

- A too-broad canvas/WebGL stub can hide real Graph regressions.
- Updating the failing test without confirming UI intent can encode the wrong accessibility contract.
- Suppressing `console.warn` globally can hide unrelated runtime errors.

Small steps:

- [ ] Run `npm run test -- src/app/App.test.tsx` and capture the exact failure text.
- [ ] Inspect the current sidebar collapse/resize behavior and choose whether the test should assert a splitter, icon rail, or restored accessible button.
- [ ] Add the smallest jsdom Graph isolation: canvas context stub, Sigma test mock, or renderer injection.
- [ ] Add/adjust a regression test that fails if App render emits the old jsdom `HTMLCanvasElement.prototype.getContext` noise.
- [ ] Run focused tests: `npm run test -- src/app/App.test.tsx`.
- [ ] Run full checks listed in "Verification Rules For Every Implementation Batch".

Completion standard:

- `npm run test` exits 0.
- No repeated jsdom `HTMLCanvasElement.prototype.getContext` noise appears in the test output.
- The sidebar assertion matches the intended current UI, not a deleted control.
- No new global console suppression is introduced.

## Batch 2: `waitForTaskTerminal` Cannot Hang Forever

Problem:

- `waitForTaskTerminal` can wait forever if terminal events are missed, listener registration fails, or a task transitions before listeners are ready.

Evidence:

- `src/lib/waitForTaskTerminal.ts:5`: waits only on `task://completed`, `task://failed`, and `task://cancelled`.
- `src/lib/waitForTaskTerminal.ts:14`: returns `Promise<BackendTask>` with no explicit timeout contract.
- `src/lib/waitForTaskTerminal.ts:50`: calls `get_task` only once.
- `src/components/app/AppShell.tsx:357`: Import preview blocks on this promise before reading preview.
- `src/stores/graphStore.ts:206`: Graph build blocks on this promise before reading graph data.

Expected modified files:

- `src/lib/waitForTaskTerminal.ts`
- New or existing focused test file such as `src/lib/waitForTaskTerminal.test.ts`
- Possibly `src/components/app/AppShell.tsx` and `src/stores/graphStore.ts` if error messaging needs small caller handling.

Modification scope:

- Extend the API to accept options such as `timeoutMs`, `pollMs`, and testable adapters for `invoke`/`listen` if needed.
- Poll `get_task` every 750-1000ms until terminal state or timeout.
- Reject with a typed/recognizable timeout error when the task does not reach terminal state.
- Treat listener registration failure as recoverable only when polling is active; otherwise fail clearly.
- Preserve immediate resolve when the input task is already terminal.

Risk:

- Too-short timeout can falsely fail legitimate long Import/Graph tasks.
- Polling too frequently can add IPC noise.
- Changing promise rejection behavior may surface new UI error states in Import/Graph flows.

Small steps:

- [ ] Write tests for already-terminal tasks, event-driven completion, missed event plus polling completion, listener failure plus polling, and timeout rejection.
- [ ] Run the focused test and confirm it fails for the missing behavior.
- [ ] Implement polling, timeout, cleanup of listeners/timers, and clear error propagation.
- [ ] Confirm Import preview and Graph build still use the helper safely.
- [ ] Run `npm run test -- src/lib/waitForTaskTerminal.test.ts`.
- [ ] Run full checks listed in "Verification Rules For Every Implementation Batch".

Completion standard:

- No code path in `waitForTaskTerminal` can leave an unresolved promise after timeout.
- Listener cleanup runs on resolve, reject, and timeout.
- Import preview and Graph rebuild show or propagate a clear error rather than spinning forever.

## Batch 3: First-Screen Bundle Split

Problem:

- The App shell statically imports feature views and heavy dependencies, so first screen pays for Graph, Milkdown, Markdown renderer, and Readability even when the user only opens Dashboard.

Evidence:

- `src/components/app/AppShell.tsx:6-14`: static imports for Exports, Import, Chat, Graph, Lint, Settings, and Wiki views.
- `src/components/app/AppShell.tsx:631-664`: view dispatch renders those imported views.
- `src/features/graph/GraphView.tsx:2-5`: static imports for Louvain, ForceAtlas2, worker, and Sigma.
- `src/features/wiki/WikiEditor.tsx:15-30`: static imports for Milkdown packages and theme CSS.
- `src/features/chat/MessageContent.tsx:3-7`: static imports markdown renderer/plugins.
- `src/features/wiki/MarkdownReader.tsx:3-7`: static imports markdown renderer/plugins.
- `src/lib/readability.ts:1`: static import of `@mozilla/readability`.
- `package.json:18-44`: heavy runtime dependencies are present in the main dependency graph.

Expected modified files:

- `src/components/app/AppShell.tsx`
- `src/features/wiki/WikiView.tsx`
- `src/features/wiki/WikiEditor.tsx` or a lazy wrapper around it
- `src/features/chat/MessageContent.tsx`
- `src/features/wiki/MarkdownReader.tsx`
- `src/lib/readability.ts` and Import URL flow call sites if needed
- Possibly `vite.config.ts` for manual chunk naming after lazy boundaries are in place

Modification scope:

- Replace feature-view static imports in AppShell with `React.lazy` boundaries and compact fallbacks.
- Keep fallback UI dense and shell-like; no marketing cards, decorative visuals, or layout shifts.
- Lazy load `WikiEditor` only when edit mode is entered.
- Lazy load markdown renderer/plugin work behind Chat/Wiki message/reader boundaries or a shared renderer boundary.
- Use dynamic import for `@mozilla/readability` only in the URL import path.
- Add manual chunk hints only if lazy imports still produce poor chunk grouping.

Risk:

- Lazy boundaries can introduce flicker or focus loss if fallbacks resize panes.
- Named exports need correct `React.lazy` wrappers.
- Moving CSS imports can change Milkdown styling if loaded too late or not at all.
- Bundle output verification can dirty `dist/` and build metadata.

Small steps:

- [x] Convert AppShell feature views to lazy imports with a stable pane-sized fallback.
- [x] Run `npm run test -- src/app/App.test.tsx`.
- [x] Lazy load Wiki editor only for edit mode and verify read mode does not load Milkdown.
- [x] Lazy load or split markdown renderer for Chat/Wiki without changing rendered markdown semantics.
- [x] Change Readability to dynamic import in URL extraction path.
- [x] Run `npm run build` and record chunk table plus main chunk size in this progress file.
- [x] Run full checks listed in "Verification Rules For Every Implementation Batch".

Completion standard:

- Dashboard/App shell no longer statically imports Graph/Sigma, Milkdown, Readability, or markdown renderer modules.
- Fresh build produces separate async chunks for at least Graph, Wiki/Milkdown, Import/Readability, and Markdown renderer paths.
- App tests still pass and fallbacks do not break shell layout.

## Batch 4: Minimal Wiki Index/Cache

Problem:

- Search, Chat retrieval, and Graph cache freshness repeatedly scan and read all Markdown files.

Evidence:

- `src-tauri/src/services/search_service.rs:32-43`: `scan_wiki` lists markdown files and loads each page.
- `src-tauri/src/services/search_service.rs:513`: search path loads each page.
- `src-tauri/src/services/search_service.rs:633`: Chat retrieval calls `read_page` after search results.
- `src-tauri/src/services/search_service.rs:648-663`: `load_page` reads and parses files.
- `src-tauri/src/services/search_service.rs:675-706`: `build_meta` derives metadata and calls `file_hash`.
- `src-tauri/src/services/file_store.rs:151-157`: `file_hash` resolves and hashes a path.
- `src-tauri/src/services/file_store.rs:238`: `hash_file` rereads file bytes.
- `src-tauri/src/commands/graph_commands.rs:19-20`: `get_graph` scans wiki before resolving graph cache.
- `src-tauri/src/commands/graph_commands.rs:79`: graph rebuild scans wiki before rebuilding.
- `src-tauri/src/app_state.rs:16-29`: `AppState` already centralizes service instances.

Expected modified files:

- `src-tauri/src/services/search_service.rs`
- Possibly new `src-tauri/src/services/wiki_index_service.rs` or `src-tauri/src/services/wiki_index.rs`
- `src-tauri/src/services/mod.rs`
- `src-tauri/src/app_state.rs` if the index is a shared service rather than owned by `SearchService`
- `src-tauri/src/commands/search_commands.rs`
- `src-tauri/src/commands/wiki_commands.rs`
- `src-tauri/src/commands/graph_commands.rs`
- Possibly `src-tauri/src/services/graph_service.rs` for cache freshness input
- Rust tests near the modified service or in a focused test module

Modification scope:

- MVP index entry fields: project-relative path, mtime, size, content hash, title, page type, tags, sources, wikilinks, word count, bookmarked flag, and a bounded body excerpt.
- Start with per-project in-memory cache keyed by project id/root and invalidated by path + mtime + size.
- When a file must be read, compute hash from the already-read content instead of rereading via `file_hash`.
- Keep the public `SearchService`/command behavior stable while internally reusing the index for `scan_wiki`, search scoring, Chat retrieval excerpts, and Graph freshness.
- Optional persistence may use `.app/index.json`, but only as derived JSON cache and only after the in-memory MVP is correct.

Risk:

- Stale index entries could hide external Markdown edits.
- Incorrect invalidation can break optimistic concurrency hash checks.
- Bookmark state joins must remain current even if content metadata is cached.
- CJK filenames, Unicode paths, Windows separators, and case-sensitivity differences must be covered.

Small steps:

- [x] Write Rust tests for index reuse, external edit invalidation, deleted page removal, Unicode filename handling, and bookmark refresh.
- [x] Add a minimal index snapshot type without changing command DTOs.
- [x] Refactor `load_page`/metadata construction so one file read produces body, metadata, and hash.
- [x] Route `scan_wiki` through the index while preserving `WikiTree` output.
- [x] Route search and Chat retrieval to reuse indexed body/excerpt data where possible.
- [x] Route Graph cache freshness through the index snapshot so cache checks do not require a fresh full page read.
- [x] Run focused Rust tests for search/index/graph.
- [x] Run full checks listed in "Verification Rules For Every Implementation Batch".

Completion standard:

- Search, Chat retrieval, and Graph freshness share one current per-project index snapshot.
- Repeated search/chat/graph calls do not reread unchanged Markdown files solely to recompute metadata/hash.
- External Markdown edits are detected by mtime/size and refreshed before returning data.
- No user wiki content is stored outside Markdown/JSON/local files.

## Batch 5: Graph Reducer Render Snapshot

Problem:

- Graph reducer work is O(E*N) in hot interactions because each edge rebuilds render options and recomputes hidden nodes by scanning all nodes.

Evidence:

- `src/features/graph/GraphView.tsx:119-147`: many state changes call `refresh`.
- `src/features/graph/GraphView.tsx:468-480`: `currentRenderOptions` and `hiddenNodeIds` are computed dynamically.
- `src/features/graph/GraphView.tsx:496-499`: node reducer calls `currentRenderOptions`.
- `src/features/graph/GraphView.tsx:512-516`: edge reducer calls `currentRenderOptions` and `hiddenNodeIds(options)` for each edge.
- `src/features/graph/GraphView.tsx:565-566`: refresh always uses `skipIndexation: false`.
- `src/features/graph/GraphView.tsx:600`: layout refresh runs every 50ms.

Expected modified files:

- `src/features/graph/GraphView.tsx`
- Preferably new pure utility such as `src/features/graph/graphRenderModel.ts`
- New focused tests such as `src/features/graph/graphRenderModel.test.ts`

Modification scope:

- Build a render snapshot once per refresh/state change: render options, community map, hidden node set, and highlight state.
- Reducers should only read the latest snapshot and per-node/per-edge source data.
- Move pure render visibility/visual calculation into a testable utility if it keeps `GraphView` smaller without a broad rewrite.
- Evaluate `skipIndexation: true` only for hover/highlight changes where topology and hidden status do not change.
- Consider requestAnimationFrame/debounce only if reducer snapshotting is not enough.

Risk:

- Snapshot state can go stale if effects do not update refs in the right order.
- `skipIndexation: true` can render incorrect hidden/visible states if used for filter changes.
- Exported PNG/SVG must match the visible filtered graph.

Small steps:

- [x] Extract pure render model tests for hidden nodes, node visuals, edge visuals, selected/focused/hover states, and filters.
- [x] Add a render snapshot ref and update it before renderer refresh.
- [x] Change node and edge reducers to read snapshot instead of recomputing options/hidden set.
- [x] Limit `skipIndexation: true` to verified hover/highlight-only refreshes, or leave it false if not proven safe.
- [x] Run focused graph render model tests.
- [x] Run full checks listed in "Verification Rules For Every Implementation Batch".

Completion standard:

- `hiddenNodeIds` is computed once per render snapshot, not once per edge.
- `communityByNodeId` is built once per render snapshot, not inside per-edge/per-node reducer calls.
- Search, filter, hover, selected/focused, layout, and export behavior remain visually consistent.

## Progress Log

Use this section for future repair conversations. Newest entries go on top.

Format:

`[YYYY-MM-DD] 阶段/任务 — 完成内容 — 验证结果 — 遗留问题`

Entries:

- `[2026-07-06] Batch 5/Graph reducer render snapshot — Eliminated the O(E*N) hot path in GraphView (audit PERF-005). Changes: (1) NEW src/features/graph/graphRenderModel.ts — pure utility. buildRenderSnapshot(graphData, input) returns { options: GraphRenderOptions, hiddenNodeIds: Set<string> } computed ONCE: communityByNodeId Map from graphData.layout.communities (previously currentRenderOptions() rebuilt this Map on every edge); hiddenNodeIds via one nodes-scan + hiddenReasonForNode (previously hiddenNodeIds(options) scanned all nodes on every edge). visibleTypeFilterFromHidden converts the store's hidden-types Set to the visible-types Set GraphRenderOptions.typeFilter expects — character-for-character equivalent to the old GraphView.visibleTypeFilter. RenderSnapshotInput mirrors RenderState + useGraphStore.colorMode. (2) NEW src/features/graph/graphRenderModel.test.ts — 19 focused tests pinning: community map built once from layout.communities; empty community map when layout null; hiddenNodeIds covers type/degree/search; empty hidden set when no filter; visualForNode parity with the prior per-call path (color/highlight/borderColor/opacity); edge hidden when endpoint hidden; edge visible when both endpoints survive; selected dims non-neighbors; selected-neighbor edge highlight; hoveredType dimming without hiding; focused-node highlight parity with selectedNodeId (sizeDelta 1, forceLabel, dims far); search-hit forceLabel; no-focus-root edge full opacity; snapshot not memoized across calls; empty graph handled; input fields propagated verbatim. (3) MODIFIED src/features/graph/GraphView.tsx — RenderRefs gained snapshot/dataRef/stateRef fields; stateRef and dataRef now declared before refs and passed in; dataRef.current = data synced each render. NEW refresh(refs, renderer) calls updateRenderSnapshot(refs) then renderer.refresh({ skipIndexation: false }). NEW refreshVisuals(refs, renderer) calls updateRenderSnapshot(refs) then renderer.refresh({ skipIndexation: true }) for visual-only changes. NEW updateRenderSnapshot(refs) reads dataRef.current + stateRef.current + useGraphStore.getState().colorMode, calls buildRenderSnapshot, assigns refs.snapshot. createRenderer now takes refs; nodeReducer/edgeReducer read refs.snapshot (with an emptySnapshot fallback for sigma's construction-time reducer calls) instead of calling currentRenderOptions()/hiddenNodeIds(options) — the O(E*N) path is gone. skipIndexation safety was evaluated per call site: skipIndexation: true applied to selectedNodeId, focusedNodeId, hoveredType, enterNode, leaveNode, colorMode effects (hidden set + positions unchanged; visualForNode returns hidden:false opacity:0.16 for dimmed non-neighbors, so hidden set is stable; colorMode mutates only node color attribute via applyColors). Kept skipIndexation: false for search, typeFilter, degreeThreshold (hidden set changes), data rebuild, layout refresh timer, reset/recompute layout (positions change). Removed dead visibleTypeFilter, currentRenderOptions, hiddenNodeIds closures and unused imports (hiddenReasonForNode, GraphRenderOptions, GraphNode). Review workflow ran two subagents (A shared-context, B fresh-context); no blockers. A: no defects; design intent / snapshot freshness / skipIndexation safety / dataRef pattern / export consistency all confirmed; recommended adding focused-node parity + search-hit forceLabel + no-focus-root edge opacity tests (added). B found one minor real defect, fixed: B#2 disposeRenderer did not clear refs.snapshot → on renderer recreation sigma's construction-time reducers read the previous graph's stale snapshot instead of the emptySnapshot fallback; fixed by adding refs.snapshot = null in disposeRenderer; also corrected the emptySnapshot comment (B#3). B#1 observation: visibleTypeFilterFromHidden's "empty = all visible" equivalence is load-bearing on the size > 0 guard inside hiddenReasonForNode — added a comment documenting this dependency. All checks pass after review fixes: npm run test 337 passed (318 baseline + 16 + 3 review-added), npm run lint 0 warnings, cargo check clean, no console.log in src or src-tauri/src. No UI changes, no Rust changes, no user wiki content changes. P0 batch sequence (1-5) complete; no further batches advanced in this turn.`
- `[2026-07-06] Batch 4/Minimal wiki index/cache — Added a per-project in-memory wiki index so Search, Chat retrieval, and Graph cache freshness share one current snapshot and stop re-reading unchanged Markdown (audit PERF-004). Changes: (1) NEW src-tauri/src/services/wiki_index.rs — WikiIndex (Mutex<HashMap<project_id, IndexSnapshot>> + LRU order Vec, MAX_CACHED_PROJECTS=8). IndexEntry holds path/mtime_secs/nanos/size/hash/full WikiPageMeta/full body_markdown/content_reads counter. refresh() walks wiki/ once via FileStore::list_markdown_files; for each file, reuses the cached entry when mtime+size match (zero fs.read), otherwise reads bytes once and derives body+frontmatter+meta+SHA-256 hash from that one buffer (no second fs.read for hashing — eliminates the pre-index double read). Deleted files dropped via next.retain(live_paths). evict()/entries() for explicit close + cache reads. (2) src-tauri/src/services/search_service.rs — added index: WikiIndex field; routed scan_wiki (builds WikiTree from cached metas, overlays live bookmark paths at scan time — bookmark state is NOT cached because a toggle changes bookmarks.json without moving page mtime/size), search (scores against cached body_markdown; scoring/snippet/sort logic is character-for-character identical to the pre-index version), and retrieve_with_excerpts (excerpt derived from cached body, no per-result read_page re-read) through the index. Removed the now-dead private load_page helper. read_page (single-file path) is unchanged. (3) src-tauri/src/services/mod.rs — registered + re-exported wiki_index::{IndexEntry, WikiIndex}. No command/DTO changes — graph_commands/get_graph and run_graph_build still call search_service.scan_wiki (now index-backed, so Graph freshness rides the same cache for free), search_commands/search_wiki calls search (index-backed), wiki_commands/scan_wiki calls scan_wiki (index-backed), chat_service.rs:202 retrieve_with_excerpts is index-backed. Key decisions: (a) Cache the FULL body_markdown, not the "bounded body excerpt" the plan listed — search scores the full body, so caching only an excerpt would leave search as a full-scan hot path (the exact thing PERF-004 targets); for the 200-500 page target the memory cost is acceptable, and the chat excerpt is derived from the cached body at retrieval time. (b) In-memory only, no .app/index.json persistence (MVP; plan explicitly allows it). (c) Hash derivation is SHA-256(raw bytes), identical to FileStore::file_hash — so graph content_hash_for (which consumes page.hash) is unchanged and graph cache hit/miss behavior is preserved (pinned by index_hash_matches_file_store_file_hash test). Review workflow ran two subagents (A shared-context, B fresh-context). A: no defects, all acceptance criteria met, full-body caching decision is correct, byte-identical results for valid UTF-8. B found two real items + one accepted-risk observation, all addressed: B#1 (real defect) evict is never called in production and there is no project-close command today → unbounded memory growth across project switches — fixed with MAX_CACHED_PROJECTS=8 cap + LRU order (promote-on-refresh, drop-oldest-when-over), evict syncs the order list; added cap_drops_oldest_project_snapshot_when_limit_exceeded + refreshing_an_old_project_promotes_it_and_evicts_a_different_one tests. B#2 (minor) content_reads was pub despite its doc saying "not exposed outside the crate" — changed to pub(crate) + #[allow(dead_code)] (lib build doesn't read it, test build does). B#3 (accepted-risk observation) read_page (fresh FileStore::file_hash) vs scan_wiki (cached hash) can diverge in the same-mtime+same-size content-edit edge case — this is inside the plan's explicit "mtime/size/hash" risk envelope (hash is a derived field, not a per-call re-verification trigger, or PERF-004's fs.read savings evaporate); recorded in gotchas.txt. Added B's test-gap fills: refresh_on_empty_wiki_returns_no_entries_without_error, index_hash_matches_file_store_file_hash_for_the_same_bytes. All checks pass after review fixes: npm run test 48 files/318 tests, npm run lint 0 warnings, cargo check clean (0 warnings), cargo test --lib --no-default-features 422 passed (Batch 4 added 18 Rust tests: wiki_index 12 + index_integration_tests 6), no console.log in src or src-tauri/src. NOTE: Rust tests on Windows require --no-default-features (gui feature + cdylib crate-type triggers STATUS_ENTRYPOINT_NOT_FOUND at test-binary load; recorded in gotchas.txt). No user wiki content mutated; raw/sources/ immutability untouched; no database; no UI changes. Batch 5 (Graph reducer render snapshot) starts next per the recommended order; not advanced in this turn.`
- `[2026-07-06] Batch 3/First-screen bundle split — Split the App shell's static dependency graph so the Dashboard first screen no longer pays for Graph/Sigma, Milkdown, Markdown renderer, or Readability. Changes: (1) src/components/app/AppShell.tsx — converted ChatView, ExportsView, GraphView, ImportView, LintView, SettingsView, WikiView, AgentView to React.lazy (kept DashboardView static as the default first screen), wrapped the view dispatch in <Suspense fallback={<ViewFallback/>}>, and replaced the static `articleToMarkdown`/`extractArticleFromHtml` import with `await import("../../lib/readability")` inside the URL branch of requestTextImportPreview so @mozilla/readability only loads when a URL import actually runs. (2) src/components/app/RightContextPanel.tsx — discovered via the first build that PageChatPanel statically imported ChatView's MessageBubble/StreamingBubble, and RightContextPanel is statically imported by AppShell, which pulled the entire ChatView → MessageContent → react-markdown chain (and the shared 496KB markdown-renderer chunk) into the startup graph; fixed by lazy-loading PageChatPanel with its own Suspense boundary (Wiki "ask AI" assistant mode only). (3) src/features/wiki/WikiView.tsx — converted WikiEditor to React.lazy so Milkdown/ProseMirror only loads when edit mode is entered (read/preview modes never load it). (4) New src/components/app/ViewFallback.tsx — shared compact, shell-aligned Suspense fallback (centered spinner + i18n label, role="status") with no layout shift or decorative chrome; added shell.view.loading key to en.json + zh-CN.json. Build evidence (npm run build, fresh): main entry bundle shrank from ~1.93MB (dist/assets/index-Dmu_vTi_.js baseline) to 314.99 kB / 89.83 kB gzip (dist/assets/index-wuMKAKsx.js). Verified async-only (NOT in index.html modulepreload) chunks for all four heavy paths: GraphView-ZjvxkcNd.js 221.62 kB (Sigma + graphology + ForceAtlas2 + Louvain), WikiEditor-D4IzKaJa.js 347.55 kB (Milkdown/ProseMirror) + separate WikiEditor-*.css 16.64 kB (nord theme), readability-BNKGh9XK.js 36.02 kB (@mozilla/readability), and the shared markdown-renderer chunk lib-CiyRAvPn.js 496.00 kB (react-markdown + remark-gfm + remark-math + rehype-katex + rehype-highlight + KaTeX + highlight.js, reached only via ChatView/PageChatPanel and WikiView/MarkdownReader). Other async view chunks: WikiView 33.04 kB, ChatView 19.02 kB, PageChatPanel 3.48 kB. DashboardView intentionally kept in the entry bundle. Review workflow ran two subagents (A shared-context, B fresh-context); no blockers; fixed the single test-regression (App.test.tsx graph-canvas-noise test: lazy GraphView + Suspense + Sigma init now exceeds findByText's 1000ms default under full-suite load, bumped to { timeout: 5000 } with a comment documenting the lazy-load reason) and added src/components/app/ViewErrorBoundary.tsx — a minimal class ErrorBoundary (getDerivedStateFromError + componentDidCatch → console.error) wrapping the three lazy Suspense sites (AppShell dispatch keyed by activeView so navigation resets the boundary, RightContextPanel PageChatPanel, WikiView WikiEditor) so a failed dynamic import in a packaged Tauri build (Windows antivirus / partial update; tauri.conf.json csp is null so not CSP-blocked) shows a compact in-place "view could not be loaded / Reload" panel via window.location.reload() instead of white-screening the whole shell; this is the standard React.lazy complement to Suspense and was the top finding from both reviewers. Removed the unused `label` prop from ViewFallback (review nit). Deferred (justified, documented here): (a) B#2 — WikiView keeps MarkdownReader as a STATIC import inside the lazy WikiView chunk rather than lazy-loading it per-read; the plan's "Chat/Wiki rendering boundary" / "shared renderer boundary" is already satisfied by the shared async chunk lib-CiyRAvPn.js (loaded once on first Chat or Wiki open), and splitting MarkdownReader further would add a spinner before every wiki page read to save loading the renderer only when a user opens Wiki and goes straight to edit without reading — a rare case that does not justify the common-case latency hit. (b) A#S1/B#4 — no module-graph regression test pinning that Dashboard render does NOT load Graph/WikiEditor/markdown-renderer; jsdom makes "fallback appears first" assertions brittle (lazy resolves faster than the 1000ms default) and the durable guarantee is a CI bundle-budget check (deferred per the plan's Defer list); the build output (index.html modulepreload list) remains the authoritative source of truth and is recorded above. Observation (not addressed, out of Batch 3 scope): settingsStore-CYV7WFNN.js (171.75 kB) is modulepreloaded because App.tsx statically imports useSettingsStore; if a future batch wants to shrink the preloaded set further, candidate is to defer settings bootstrap or split the store — All checks pass after review fixes: npm run test 48 files/318 tests, npm run lint 0 warnings, cargo check 6.54s clean, tsc -b clean, no console.log in src or src-tauri/src, fresh build main bundle 315.05 kB / gzip 89.85 kB (ErrorBoundary added ~0.06 kB). No visual/UX redesign; Codex-like compact shell preserved. Batch 4 (minimal wiki index/cache) starts next per the recommended order; not advanced in this turn.`
- `[2026-07-06] Batch 2/waitForTaskTerminal cannot hang forever — Rewrote src/lib/waitForTaskTerminal.ts to eliminate every indefinite-pending path: added { pollMs (default 1s), timeoutMs (default 10 min) } options; a recursive setTimeout poll on get_task as a race-safe fallback for missed events, pre-listener-attachment termination, and transient get_task failures (retries on reject); a deadline setTimeout that rejects with a typed WaitForTaskTerminalTimeoutError (code "TASK_WAIT_TIMEOUT", taskId, timeoutMs) when no terminal signal arrives; cleanup() releases all 3 event listeners + both timers on resolve/reject/timeout; listener registration failure stays non-fatal (browser-dev compat — polling is authoritative). Hardened the poll path with next.id === taskId (symmetric with the event-path filter) and clamped pollMs to >=1 to prevent a setTimeout(0) macro-task starvation loop. NO caller changes needed — verified graphStore.runGraphBuild (try/catch → status "error" + errorMessage) and AppShell Import preview (.then/.catch → setImportPreview(null) + toast) already surface a timeout rejection as a clear error; user-owned AppShell.tsx was not touched — Tests: kept the 4 event-driven tests (real timers); replaced the old "stays pending forever" test (which pinned the bug) with a fake-timer group of 5 — polling recovery, transient-get_task-failure retry, wrong-id-doesn't-resolve, typed timeout rejection + cleanup, timer-cleanup on event resolve. Review workflow ran two subagents (A shared-context, B fresh-context); no blockers; fixed B#1 (pollMs clamp) + B#2 (taskId check + test); deferred A#1/B#3 (caller code-based timeout UX) as P0-acceptable follow-up — All checks pass: npm run test 48 files/318 tests (was 314; +4 net), npm run lint 0 warnings, cargo check 0.68s clean, tsc --noEmit exit 0, no console.log — Deferred: callers do not yet branch on error.code === "TASK_WAIT_TIMEOUT" for a friendlier "timed out, retry" UX (both just render errorMessage verbatim, which meets the plan's "clear error, not infinite loading" bar); Batch 3 (first-screen bundle split) starts next per the recommended order, not advanced in this turn.`
- `[2026-07-06] Batch 1/Test reliability + Graph/Sigma jsdom noise — Fixed the failing App.test.tsx sidebar assertion (the topbar Collapse/Expand sidebar buttons were intentionally removed by the user's uncommitted AppShell/TopBar diff; collapse is now splitter-drag-only via paneSizes.sidebar <= SIDEBAR_COLLAPSE_THRESHOLD). Silenced the repeated jsdom "Not implemented: HTMLCanvasElement.prototype.getContext" noise by overriding HTMLCanvasElement.prototype.getContext to return null in src/test/setup.ts (models "WebGL unavailable"; GraphView's existing try/catch then renders the "canvas unavailable" placeholder — this is environment setup, NOT global console suppression). Added a regression test with three complementary checks: (a) pin the stub via a non-enumerable __silencedJSDOMCanvasNoise marker, (b) confirm the end-to-end fallback renders, (c) assert GraphView's real "[graph] sigma renderer init failed:" console.warn still fires (proves Batch 1 goal #4 — no global console suppression). Review workflow ran two subagents (A shared-context, B fresh-context); merged-and-fixed findings: de-duplicated a comment block in setup.ts, renamed the sidebar test to honestly say "pane size reaches collapse threshold" (full pointer-drag simulation out of Batch 1 scope; drag/clamp path covered by useResizablePane.test.ts), trimmed absolutist regression-test phrasing, documented the 2D-canvas latent footgun in the setup comment, added check (c) — All checks pass: npm run test 48 files/314 tests (was 313, +1 regression; zero jsdom noise), npm run lint 0 warnings, cargo check 0.62s clean, tsc --noEmit exit 0, no console.log in src or src-tauri/src — Deferred (later batches or out of scope): full pointer-event drag/collapse simulation, splitter double-click / keyboard Home/End coverage, formal bundle budget. Batch 2 (waitForTaskTerminal timeout/polling) starts next per the recommended order; not advanced in this turn.`
- `[2026-07-06] Planning/P0 performance stability — Created this P0 progress and implementation plan from the 2026-07-06 audit; no source code changes started — Baseline copied from audit: lint passed, cargo check passed, npm test failed with 1 failing test plus Graph/Sigma jsdom noise — Implementation pending; worktree already has unrelated uncommitted changes that must be preserved`
