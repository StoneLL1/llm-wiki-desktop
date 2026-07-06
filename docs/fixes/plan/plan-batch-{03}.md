# Batch 03 Knowledge Interaction, Chat, and Bookmarks Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement `docs/fixes/03-knowledge-interaction-chat-bookmarks.md` end to end: app-owned Wiki/HTML favorites, a reliable local Chat retrieval path for Chinese natural-language questions, and a Wiki-page scoped Ask AI side panel.

**Architecture:** Add a backend `BookmarkService` as the only writer for `.app/bookmarks.json`; keep Markdown/frontmatter immutable for user favorite actions; join bookmark state into Wiki scans and Export records; improve `SearchService` local keyword retrieval with normalized terms and phrase matching; extend Chat retrieval with an optional pinned Wiki page; add a compact React side-panel chat surface that reuses `chatStore`, `ChatComposer`, and the existing task/citation pipeline.

**Tech Stack:** React 19 + TypeScript + Zustand + i18next + Lucide React + Tailwind v4 token classes; Tauri v2 Rust commands -> DTOs -> services -> local Markdown/JSON files; Vitest/Testing Library; Rust unit tests through `cargo check --lib --tests` and targeted service tests.

---

## Read Context

- Product/scope: `SPEC/PRD.md`, `SPEC/SPEC.md`, `SPEC/APP_flow.md`.
- Architecture/style: `SPEC/TECH_STACK.md`, `SPEC/BACKEND_STRUCTURE.md`, `SPEC/FRONTEND_GUIDELINES.md`, `SPEC/DESIGN.md`.
- Design authority: `UI-Frontend-design/dashboard.html`, `UI-Frontend-design/wiki.html`, `UI-Frontend-design/chat.html`, `UI-Frontend-design/exports.html`, `UI-Frontend-design/assets/app.css`, `UI-Frontend-design/assets/app.js`.
- Fix specs: `docs/fixes/00-codebase-audit.md`, `docs/fixes/03-knowledge-interaction-chat-bookmarks.md`.
- Core frontend code read: `src/components/app/LeftSidebar.tsx`, `src/components/app/RightContextPanel.tsx`, `src/components/app/AppShell.tsx`, `src/stores/navigationStore.ts`, `src/features/wiki/WikiView.tsx`, `src/features/wiki/WikiTree.tsx`, `src/features/wiki/wikiStore.ts`, `src/stores/chatStore.ts`, `src/features/chat/ChatView.tsx`, `src/features/chat/ChatComposer.tsx`, `src/stores/exportStore.ts`, `src/features/exports/ExportsView.tsx`, `src/types/wiki.ts`, `src/types/chat.ts`, `src/types/export.ts`.
- Core backend code read: `src-tauri/src/models/wiki.rs`, `src-tauri/src/models/chat.rs`, `src-tauri/src/models/export.rs`, `src-tauri/src/services/search_service.rs`, `src-tauri/src/services/chat_service.rs`, `src-tauri/src/services/export_service.rs`, `src-tauri/src/commands/wiki_commands.rs`, `src-tauri/src/commands/chat_commands.rs`, `src-tauri/src/commands/export_commands.rs`, `src-tauri/src/app_state.rs`, `src-tauri/src/lib.rs`.

## Clarification Status

No blocking questions. The spec and codebase are sufficient to implement without asking the user first.

Implementation decisions fixed for this plan:

- User star/favorite state SHALL be private app state in `.app/bookmarks.json`, not Markdown frontmatter.
- Existing frontmatter `starred: true` SHALL remain readable compatibility metadata for tree/search/graph signals and SHALL NOT be deleted.
- Left sidebar `精选页面` SHALL list app-owned bookmarks only: Wiki page bookmarks and HTML export bookmarks.
- Existing Wiki `toggle_bookmark` IPC SHALL remain as a compatibility wrapper.
- Chat search SHALL remain local keyword/filter retrieval. No embeddings, vector database, or LLM-backed ordinary search.
- Ask AI from a Wiki page SHALL reuse the Chat backend route and task pipeline with a pinned page context.

## First Principles

1. A favorite is user preference, so it belongs in `.app/` app state and must not dirty knowledge Markdown.
2. A Chat answer is only trustworthy when citations match the exact context passed to Agent/BYOK.
3. Wiki-page Ask AI is not a separate AI product surface; it is Chat with a forced current-page context.
4. Export favorites identify generated artifacts, so `exportRecordId` is the stable primary key and `outputPath` is the fallback display/open target.
5. Project switching is a hard boundary: bookmark, chat, export, and preview state must not leak between `projectId/rootPath` scopes.

## File Structure Map

Create:

- `src-tauri/src/models/bookmark.rs`
- `src-tauri/src/services/bookmark_service.rs`
- `src/types/bookmark.ts`
- `src/features/bookmarks/bookmarkSelectors.ts`
- `src/features/bookmarks/bookmarkSelectors.test.ts`
- `src/features/chat/PageChatPanel.tsx`
- `src/features/chat/PageChatPanel.test.tsx`
- `src/stores/navigationStore.test.ts`

Modify:

- `src-tauri/src/models/mod.rs`
- `src-tauri/src/models/wiki.rs`
- `src-tauri/src/models/chat.rs`
- `src-tauri/src/models/export.rs`
- `src-tauri/src/services/mod.rs`
- `src-tauri/src/services/search_service.rs`
- `src-tauri/src/services/chat_service.rs`
- `src-tauri/src/services/export_service.rs`
- `src-tauri/src/app_state.rs`
- `src-tauri/src/commands/wiki_commands.rs`
- `src-tauri/src/commands/chat_commands.rs`
- `src-tauri/src/commands/export_commands.rs`
- `src-tauri/src/commands/mod.rs` only if a separate `bookmark_commands.rs` module is chosen
- `src-tauri/src/lib.rs`
- `src/types/wiki.ts`
- `src/types/chat.ts`
- `src/types/export.ts`
- `src/stores/wikiStore.ts`
- `src/stores/chatStore.ts`
- `src/stores/exportStore.ts`
- `src/stores/navigationStore.ts`
- `src/components/app/LeftSidebar.tsx`
- `src/components/app/RightContextPanel.tsx`
- `src/features/wiki/WikiView.tsx`
- `src/features/wiki/WikiTree.tsx`
- `src/features/wiki/wiki.test.tsx`
- `src/features/chat/ChatView.tsx` only for exported reusable subcomponents or shared helpers
- `src/features/chat/ChatComposer.tsx` only to add optional compact placeholder/label props
- `src/features/chat/chatView.test.tsx`
- `src/features/exports/ExportsView.tsx`
- `src/features/exports/exportsView.test.tsx`
- `src/i18n/locales/en.json`
- `src/i18n/locales/zh-CN.json`
- `src/styles.css`
- `SPEC/progress.txt` after implementation lands

Do not modify:

- `UI-Frontend-design/`
- `raw/sources/`
- Wiki Markdown files for favorite toggles
- Any secret/provider credential files

---

## Task 1: Backend Bookmark Model And Service

Add an app-owned resource bookmark model with legacy read compatibility.

- [ ] Add failing Rust tests in `src-tauri/src/services/bookmark_service.rs`.
  - [ ] `reads_missing_file_as_empty_v2_file`
  - [ ] `migrates_legacy_string_array_to_wiki_page_entries`
  - [ ] `reads_v2_wiki_and_export_entries`
  - [ ] `toggle_wiki_page_preserves_export_entries`
  - [ ] `toggle_export_html_uses_export_record_id_as_primary_key`
  - [ ] `rejects_paths_outside_wiki_or_exports_html`
- [ ] Create `src-tauri/src/models/bookmark.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum BookmarkResourceKind {
    WikiPage,
    ExportHtml,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BookmarkEntry {
    pub id: String,
    pub kind: BookmarkResourceKind,
    pub path: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub export_record_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BookmarkFile {
    pub version: u32,
    #[serde(default)]
    pub entries: Vec<BookmarkEntry>,
}
```

- [ ] Create `BookmarkService` with these public methods:
  - [ ] `read_file(&self, context: &ProjectContext) -> Result<BookmarkFile, BackendError>`
  - [ ] `list_entries(&self, context: &ProjectContext) -> Result<Vec<BookmarkEntry>, BackendError>`
  - [ ] `wiki_page_paths(&self, context: &ProjectContext) -> Result<HashSet<String>, BackendError>`
  - [ ] `export_record_ids(&self, context: &ProjectContext) -> Result<HashSet<String>, BackendError>`
  - [ ] `toggle_wiki_page(&self, context, relative_path, title) -> Result<ToggleBookmarkResponse, BackendError>`
  - [ ] `toggle_export_html(&self, context, record) -> Result<ExportBookmarkResponse, BackendError>`
- [ ] Implement legacy reader in this order:
  - [ ] Parse as `BookmarkFile` when JSON has `{ "version": ..., "entries": [...] }`.
  - [ ] Parse as `Vec<String>` and convert each string to `BookmarkEntry { kind: WikiPage, path, title: filename stem, export_record_id: None }`.
  - [ ] Parse old object shapes defensively if present: `{ "pages": [...] }`, `{ "wikiPages": [...] }`, `{ "bookmarks": [...] }`.
  - [ ] Corrupt JSON returns a recoverable `BOOKMARK_PARSE_FAILED`, not silent empty.
- [ ] Persist only the v2 object shape after any toggle.
- [ ] Add `pub mod bookmark;` to `src-tauri/src/models/mod.rs`.
- [ ] Add `mod bookmark_service;` and `pub use bookmark_service::BookmarkService;` to `src-tauri/src/services/mod.rs`.
- [ ] Add `bookmark_service: BookmarkService` to `AppState`.

Expected test command:

```powershell
cargo check --manifest-path .\src-tauri\Cargo.toml --lib --tests
```

Expected result: Rust test targets compile with no new warnings from the bookmark files.

## Task 2: Wire Bookmarks Into Wiki Scan, Read, And Toggle

Move Wiki bookmark persistence out of `SearchService` while keeping existing IPC stable.

- [ ] Add failing Rust tests in `src-tauri/src/services/search_service.rs`.
  - [ ] A v2 `.app/bookmarks.json` Wiki entry sets `WikiPageMeta.bookmarked = true`.
  - [ ] The matching `WikiTreeNode.bookmarked = true` appears in `tree.root.children`, not only in `tree.pages`.
  - [ ] Legacy `Vec<String>` bookmarks still mark scanned pages.
- [ ] Change `SearchService::scan_wiki` and `read_page` to receive bookmark paths from `BookmarkService` instead of `self.load_bookmarks`.
  - Preferred signature:

```rust
pub fn scan_wiki(
    &self,
    context: &ProjectContext,
    bookmark_paths: &HashSet<String>,
) -> Result<WikiTree, BackendError>
```

  - Same approach for `read_page`.
- [ ] Remove `SearchService::toggle_bookmark` and `SearchService::load_bookmarks` after callers move to `BookmarkService`.
- [ ] Update `wiki_commands.rs`:
  - [ ] `scan_wiki`: call `state.bookmark_service.wiki_page_paths(&context)?`, then `search_service.scan_wiki(&context, &paths)`.
  - [ ] `read_wiki_page`: same bookmark path join.
  - [ ] `toggle_bookmark`: call `search_service.read_page` first for title/path validation, then `bookmark_service.toggle_wiki_page`.
- [ ] Preserve `ToggleBookmarkRequest` and `ToggleBookmarkResponse` names in `models/wiki.rs` for frontend compatibility.
- [ ] Make path validation explicit in `BookmarkService`:
  - Wiki paths must resolve under `context.wiki_dir`.
  - File must exist when toggling a Wiki page.
  - Toggle must not write Markdown.

Expected result: Existing Wiki tests still pass, and `toggle_bookmark` writes v2 `.app/bookmarks.json`.

## Task 3: Frontend Wiki Bookmark State And Tree Star Slot

Make the current UI update both flat metadata and recursive tree nodes.

- [ ] Add failing tests to `src/features/wiki/wiki.test.tsx`.
  - [ ] Toggling a bookmark updates `tree.pages`.
  - [ ] Toggling a bookmark updates matching recursive `tree.root` file node.
  - [ ] `WikiTree` renders a star when `node.bookmarked || node.starred`.
- [ ] In `src/stores/wikiStore.ts`, add a pure helper:

```ts
export function updateTreeNodeBookmark(
  node: WikiTreeNode,
  path: string,
  bookmarked: boolean,
): WikiTreeNode {
  if (node.kind === "file" && node.path === path) {
    return { ...node, bookmarked };
  }
  if (node.children.length === 0) return node;
  return {
    ...node,
    children: node.children.map((child) => updateTreeNodeBookmark(child, path, bookmarked)),
  };
}
```

- [ ] Use the helper in `toggleBookmark` when setting `tree`.
- [ ] Keep `page.meta.bookmarked` as the toolbar source of truth.
- [ ] In `src/features/wiki/WikiTree.tsx`, render the star for `node.bookmarked || node.starred`.
- [ ] Reserve a fixed `w-[18px]` or equivalent CSS slot for the row star so the actions menu does not shift.
- [ ] Use `Star` from Lucide with `fill="currentColor"` only when active.
- [ ] Do not add a second clickable star in the tree row in this batch; the toolbar remains the write control.

Expected result: user star state is visually consistent after toggle, scan, refresh, and page reopen.

## Task 4: Export HTML Bookmark Backend And Store

Let generated HTML export records participate in the same favorite system.

- [ ] Add `bookmarked: bool` to Rust `ExportRecord` with `#[serde(default)]`.
- [ ] Add `bookmarked?: boolean` or required `bookmarked: boolean` to TS `ExportRecord`. Prefer required with tests updated.
- [ ] Add `ToggleExportBookmarkRequest` and `ToggleExportBookmarkResponse` in `src-tauri/src/models/export.rs`:

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToggleExportBookmarkRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub export_record_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToggleExportBookmarkResponse {
    pub export_record_id: String,
    pub bookmarked: bool,
}
```

- [ ] Add failing Rust tests in `src-tauri/src/services/export_service.rs`.
  - [ ] `list_records_with_bookmarks_marks_matching_record_id`
  - [ ] `toggle_export_bookmark_keeps_missing_output_as_bookmarkable_history`
  - [ ] `failed_export_records_are_not_bookmarkable`
- [ ] Keep `ExportService::list_records(context)` as raw record reader.
- [ ] Add `ExportService::list_records_with_bookmarks(context, bookmark_ids)` or join in `export_commands::list_exports`.
- [ ] Add `toggle_export_bookmark` Tauri command:
  - [ ] Resolve project context.
  - [ ] Load export records.
  - [ ] Find matching `record.id`.
  - [ ] Reject missing record with `EXPORT_RECORD_NOT_FOUND`.
  - [ ] Reject `status == Failed` with `EXPORT_BOOKMARK_UNAVAILABLE`.
  - [ ] Toggle via `BookmarkService::toggle_export_html`.
- [ ] Register `toggle_export_bookmark` in `src-tauri/src/lib.rs`.
- [ ] Update `src/stores/exportStore.ts`:
  - [ ] `toggleBookmark(projectId, rootPath, recordId)`.
  - [ ] After backend response, update the matching record's `bookmarked`.
  - [ ] `loadExports` receives records already joined with `bookmarked`.
- [ ] Add store test coverage in `src/features/exports/exportsView.test.tsx` or a new `src/stores/exportStore.test.ts`.

Expected result: Export favorites persist by record id and survive identical output filenames or same-title exports.

## Task 5: Sidebar `精选页面` Favorites Section

Add a compact favorites section under Workflow and above Recent Pages, matching `dashboard.html` structure.

- [ ] Create `src/types/bookmark.ts`:

```ts
export type BookmarkResourceKind = "wiki_page" | "export_html";

export interface FavoriteSidebarItem {
  id: string;
  kind: BookmarkResourceKind;
  title: string;
  path: string;
  exportRecordId?: string;
  missing?: boolean;
}
```

- [ ] Create `src/features/bookmarks/bookmarkSelectors.ts`.
- [ ] Add tests in `bookmarkSelectors.test.ts`.
  - [ ] Wiki pages with `bookmarked === true` become `wiki_page` items.
  - [ ] Wiki pages with only `starred === true` do not enter sidebar favorites.
  - [ ] Succeeded export records with `bookmarked === true` become `export_html` items.
  - [ ] Items are sorted newest-first for exports, path/title order for Wiki pages, with Wiki pages first.
  - [ ] Missing export output remains visible with `missing: true` if the record is bookmarked and preview fails later.
- [ ] Modify `LeftSidebar.tsx`:
  - [ ] Import `Star`, `FileText`, and `FileOutput` Lucide icons.
  - [ ] Read Wiki `tree.pages`.
  - [ ] Read Export `records`, `loadExports`, and `loadPreview`.
  - [ ] Compute favorites using selector.
  - [ ] Render section label `shell.favorites`.
  - [ ] Use row height 26-30px and font sizes matching current nav/recent rows.
  - [ ] For Wiki favorite click: set active view `wiki`, call `openPage(projectId, rootPath, path)`.
  - [ ] For Export favorite click: set active view `exports`, ensure exports are loaded for current project, call `loadPreview({ projectId, projectRootPath: rootPath, outputPath }, exportRecordId)`.
  - [ ] If `loadPreview` fails, keep user in Exports view and surface `exportStore.error`.
- [ ] Add i18n keys:
  - `shell.favorites`
  - `shell.favorites.empty`
  - `shell.favorite.openWiki`
  - `shell.favorite.openExport`
  - `shell.favorite.missingExport`
- [ ] Hide the favorites section label/text under existing `@media (max-width: 820px)` collapsed sidebar rules, same as recent pages.

Expected result: the left sidebar remains the same Codex-like shell, with one additional dense section from the design hierarchy.

## Task 6: Export Favorites UI

Add star controls to generated HTML export rows.

- [ ] Add failing tests to `src/features/exports/exportsView.test.tsx`.
  - [ ] A succeeded export row renders a `Bookmark export` icon button.
  - [ ] Clicking the button calls `exportStore.toggleBookmark`.
  - [ ] A bookmarked row renders filled/starred visual state and `Unbookmark export` label.
  - [ ] Failed rows do not render bookmark buttons.
- [ ] Modify `ExportsView.tsx`:
  - [ ] Import `Star`.
  - [ ] Add `toggleBookmark` from `useExportStore`.
  - [ ] Place star icon in success-row actions before Preview.
  - [ ] Use icon-only button with `aria-label` and `title`.
  - [ ] Keep row action height 26px.
  - [ ] Do not wrap actions in extra cards or panels.
- [ ] Add i18n keys:
  - `exports.actions.bookmark`
  - `exports.actions.unbookmark`

Expected result: HTML export favorites are discoverable from Export history and mirrored in the left sidebar.

## Task 7: Search Retrieval Fix For Chinese Natural Questions

Fix the "context insufficient / empty Sources" bug without changing ordinary search into semantic AI search.

- [ ] Add failing Rust tests in `src-tauri/src/services/search_service.rs`.
  - [ ] `search_matches_chinese_question_by_extracted_title_term`
  - [ ] `retrieve_with_excerpts_handles_chinese_question_suffix`
  - [ ] `search_uses_unicode_lowercase_not_ascii_only`
  - [ ] `search_prefers_exact_title_or_alias_over_body_term`
  - [ ] `search_returns_no_hits_for_truly_unmatched_question`
- [ ] Seed the test page:

```markdown
---
title: 约束先行
aliases: [约束先行2]
tags: [方法]
---

# 约束先行

约束先行是一种先定义限制条件再生成方案的工作方式。
```

- [ ] Implement local query normalization helpers in `search_service.rs`:
  - `normalize_for_search(value: &str) -> String`
  - `extract_query_terms(query: &str) -> Vec<String>`
  - `score_field(field, terms, exact_phrase_weight, term_weight) -> Option<i64>`
- [ ] `normalize_for_search` requirements:
  - [ ] Use `.to_lowercase()`, not `.to_ascii_lowercase()`.
  - [ ] Convert punctuation and whitespace runs to single spaces.
  - [ ] Preserve CJK characters and ASCII digits.
  - [ ] Trim common Chinese question prefixes/suffixes as term boundaries: `什么是`, `请解释`, `解释一下`, `是什么`, `？`, `?`.
- [ ] `extract_query_terms` requirements:
  - [ ] Keep the normalized whole query as a phrase candidate.
  - [ ] Add CJK contiguous terms with length >= 2.
  - [ ] Add ASCII alphanumeric terms with length >= 2.
  - [ ] For terms ending in digits, add a base term without trailing digits. This catches `约束先行2` -> `约束先行`.
  - [ ] Deduplicate while preserving order.
- [ ] Update search scoring:
  - [ ] Title exact phrase: +120.
  - [ ] Title term match: +80 each.
  - [ ] Alias exact/term: +70/+45.
  - [ ] Tags term: +35.
  - [ ] Sources term: +25.
  - [ ] Body exact/term: +18/+8.
  - [ ] Path/stem term: +20.
  - [ ] Matched fields retain `title`, `aliases`, `tags`, `sources`, `content`, `path`.
- [ ] Update snippets:
  - [ ] Use first matching term for `snippet_for_query`.
  - [ ] If no snippet from body but title/path matched, use the first body excerpt sentence/line.
- [ ] Keep `search_wiki` behavior keyword-only and deterministic; it may return better results for natural phrases, but it must not call Agent/BYOK.
- [ ] Add a gotcha entry only if implementation reveals a subtle recurring issue beyond the known whole-string `contains` bug.

Expected result: Chat retrieval finds relevant Wiki pages for natural Chinese questions like `什么是约束先行2`.

## Task 8: Pinned Page Chat DTO And Backend Retrieval

Extend Chat so a Wiki page can be forced into context first.

- [ ] Add failing Rust tests in `src-tauri/src/models/chat.rs`:
  - [ ] `send_request_defaults_pinned_page_path_to_none`
  - [ ] `citation_serializes_is_pinned_when_true`
- [ ] Add failing Rust tests in `src-tauri/src/services/chat_service.rs`:
  - [ ] `retrieval_context_includes_pinned_page_first`
  - [ ] `retrieval_context_dedupes_pinned_page_from_search_hits`
  - [ ] `retrieval_context_errors_when_pinned_page_missing`
- [ ] Modify Rust DTOs:

```rust
pub struct ChatCitation {
    pub page_path: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
    pub score: i64,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_pinned: bool,
}

pub struct ChatRetrievalHit {
    pub path: String,
    pub title: String,
    pub snippet: Option<String>,
    pub score: i64,
    pub excerpt: Option<String>,
    #[serde(default)]
    pub is_pinned: bool,
}

pub struct SendChatMessageRequest {
    ...
    #[serde(default)]
    pub pinned_page_path: Option<String>,
}
```

- [ ] If `skip_serializing_if = "std::ops::Not::not"` is rejected by Rust type inference, use a local helper:

```rust
fn is_false(value: &bool) -> bool { !*value }
```

- [ ] Change `ChatService::build_retrieval_context` signature:

```rust
pub fn build_retrieval_context(
    &self,
    context: &ProjectContext,
    search_service: &SearchService,
    query: &str,
    session: &ChatSession,
    language: &str,
    pinned_page_path: Option<&str>,
) -> Result<RetrievalContext, BackendError>
```

- [ ] Pinned page behavior:
  - [ ] Resolve the path through `ProjectContext`.
  - [ ] Require it to be under `wiki/`.
  - [ ] Read the page fresh at send time.
  - [ ] Add it as the first `ChatRetrievalHit` with `score = 10_000` and `is_pinned = true`.
  - [ ] Dedupe subsequent search hits by path.
  - [ ] Keep total source count bounded to `RETRIEVAL_LIMIT`.
  - [ ] Prompt section label for pinned page: `--- Current Wiki page ---`.
- [ ] Update `chat_commands.rs` to pass `request.pinned_page_path.as_deref()`.
- [ ] Ensure Agent unavailable / BYOK missing errors are unchanged because routing happens after retrieval.
- [ ] Update TS `ChatCitation` and `SendChatMessageRequest`:

```ts
export interface ChatCitation {
  pagePath: string;
  title: string;
  snippet?: string;
  score: number;
  isPinned?: boolean;
}

export interface SendChatMessageRequest {
  ...
  pinnedPagePath?: string | null;
}
```

Expected result: a page-scoped chat request always cites the current page first when the page exists.

## Task 9: Chat Store Send Options

Allow both global Chat and page-scoped Chat to share one store method.

- [ ] Add failing tests in `src/stores/chatStore.test.ts`.
  - [ ] Existing `send(..., "auto")` still omits or sends `pinnedPagePath: null`.
  - [ ] `send(..., "auto", undefined, undefined, { pinnedPagePath })` sends `pinnedPagePath`.
- [ ] Replace positional optional arguments with an options object while keeping call sites readable:

```ts
export interface SendChatOptions {
  agent?: AgentKind | null;
  provider?: LlmProviderKind | null;
  pinnedPagePath?: string | null;
}

send: (
  projectId: string,
  rootPath: string,
  sessionId: string,
  content: string,
  route: ChatRoutePreference,
  options?: SendChatOptions,
) => Promise<string | null>;
```

- [ ] Update `ChatView` call:

```ts
void send(projectId, rootPath, activeSessionId, content, routePreference);
```

- [ ] Update backend invoke payload:

```ts
request: {
  projectId,
  projectRootPath: rootPath,
  sessionId,
  content,
  route,
  agent: options?.agent ?? null,
  provider: options?.provider ?? null,
  pinnedPagePath: options?.pinnedPagePath ?? null,
}
```

- [ ] Verify existing chat streaming, reload, and overwrite tests remain unchanged.

Expected result: global Chat is unaffected while page Ask AI can pass forced context.

## Task 10: Navigation Store Right Panel Mode

Add explicit right-panel mode for the Wiki assistant.

- [ ] Add `src/stores/navigationStore.test.ts`.
  - [ ] Initial state is `{ activeView: "dashboard", rightPanelOpen: true, rightPanelMode: "default", wikiAssistantPagePath: null }`.
  - [ ] `openWikiAssistant(path)` sets active view `wiki`, opens right panel, mode `wikiAssistant`, and stores path.
  - [ ] `setActiveView("chat")` resets mode to `default` and clears `wikiAssistantPagePath`.
  - [ ] `setWikiAssistantPagePath(path)` updates path without closing panel.
- [ ] Modify `src/stores/navigationStore.ts`:

```ts
export type RightPanelMode = "default" | "wikiAssistant";

interface NavigationState {
  activeView: AppView;
  rightPanelOpen: boolean;
  rightPanelMode: RightPanelMode;
  wikiAssistantPagePath: string | null;
  setActiveView: (view: AppView) => void;
  setRightPanelOpen: (open: boolean) => void;
  openWikiAssistant: (path: string) => void;
  setWikiAssistantPagePath: (path: string | null) => void;
  closeWikiAssistant: () => void;
}
```

- [ ] Make `setActiveView` reset `wikiAssistant` for every non-`wiki` view.
- [ ] Do not close the right panel when resetting mode.

Expected result: the side panel can switch between related-pages context and page chat predictably.

## Task 11: Wiki Ask AI Button And Page Chat Panel

Add a page-scoped side panel that reuses Chat.

- [ ] Create `src/features/chat/PageChatPanel.tsx`.
- [ ] Add `src/features/chat/PageChatPanel.test.tsx`.
  - [ ] Renders current page title/path.
  - [ ] Creates a page chat session when no active session exists.
  - [ ] Sends message with `pinnedPagePath`.
  - [ ] Shows pinned citation label when latest assistant citation has `isPinned`.
  - [ ] Displays the same store error string as global Chat when route/provider is missing.
- [ ] Implementation structure:
  - [ ] Props:

```ts
interface PageChatPanelProps {
  page: WikiPageContent | null;
  projectId: string;
  rootPath: string;
}
```

  - [ ] If `page` is null, render compact empty state.
  - [ ] Use `useChatStore` active session.
  - [ ] On first send, if no `activeSessionId`, call `createSession(projectId, rootPath, "Ask: {page.meta.title}")`.
  - [ ] Call `send(projectId, rootPath, sessionId, content, "auto", { pinnedPagePath: page.meta.path })`.
  - [ ] Use existing `sendTaskId`, `streamingText`, `clearSendTask`, `reloadActive`, and task terminal behavior pattern from `ChatView`.
  - [ ] Keep panel width within existing right panel; no nested cards.
- [ ] To avoid duplicating 200+ lines from `ChatView`, extract these shared pieces from `ChatView.tsx`:
  - [ ] Export `MessageBubble`, `StreamingBubble`, and `formatTime`, or move them to `src/features/chat/ChatMessageList.tsx`.
  - [ ] Keep public props small and typed.
  - [ ] Existing `chatView.test.tsx` must still pass.
- [ ] Modify `ChatComposer.tsx` only if needed:
  - [ ] Optional `placeholderKey?: string`
  - [ ] Optional `compact?: boolean`
  - [ ] Defaults preserve global Chat.
- [ ] Modify `WikiView.tsx`:
  - [ ] Import `MessageSquareText`.
  - [ ] Add icon-only Ask AI button near HTML/star controls.
  - [ ] `onClick`: `openWikiAssistant(page.meta.path)`.
  - [ ] Disable when no page.
  - [ ] `aria-label={t("wiki.actions.askAi")}` and `title` same key.
- [ ] Add an effect in `WikiView.tsx`:

```ts
useEffect(() => {
  if (rightPanelMode === "wikiAssistant" && page?.meta.path) {
    setWikiAssistantPagePath(page.meta.path);
  }
}, [rightPanelMode, page?.meta.path, setWikiAssistantPagePath]);
```

- [ ] Modify `RightContextPanel.tsx`:
  - [ ] Read `rightPanelMode` from `navigationStore`.
  - [ ] If `activeView === "wiki" && rightPanelMode === "wikiAssistant"`, render `PageChatPanel`.
  - [ ] Header title key: `wiki.askAi.panelTitle`.
  - [ ] Otherwise keep `RelatedPagesPanel`.
- [ ] Add i18n keys:
  - `wiki.actions.askAi`
  - `wiki.askAi.panelTitle`
  - `wiki.askAi.empty`
  - `wiki.askAi.currentPage`
  - `wiki.askAi.placeholder`
  - `chat.citations.currentPage`

Expected result: Wiki users can ask follow-up questions from the current page without leaving the Wiki view.

## Task 12: Citation UI For Pinned Sources

Expose current-page context honestly in Chat and right panel citations.

- [ ] Update `MessageBubble` citation buttons:
  - [ ] If `citation.isPinned`, render a small `chat.citations.currentPage` badge.
  - [ ] Keep the path visible.
- [ ] Update `RightContextPanel.tsx` Chat citations section:
  - [ ] Display the same current-page badge for pinned citation.
- [ ] Update `src/features/chat/MessageContent.tsx` only if citation numbering changes are needed; pinned citation still counts as citation `[1]`.
- [ ] Add tests in `chatView.test.tsx` or `PageChatPanel.test.tsx` asserting the badge renders.

Expected result: the user can see when an answer used the current Wiki page as forced context.

## Task 13: Styling And I18n Contracts

Keep density and bilingual UI aligned with design instructions.

- [ ] Add styles in `src/styles.css` only when Tailwind utility classes become repetitive.
- [ ] Preferred new classes:
  - `.page-chat`
  - `.page-chat__head`
  - `.page-chat__body`
  - `.favorite-row`
  - `.tree-star-slot`
- [ ] Use existing token variables only: `--surface`, `--border`, `--text-muted`, `--accent`, `--accent-soft`, `--radius-sm`, `--radius-md`.
- [ ] No new hex colors.
- [ ] No gradients, decorative blobs, landing-page sections, or nested cards.
- [ ] Add every new English key to `src/i18n/locales/en.json`.
- [ ] Add every new Chinese key to `src/i18n/locales/zh-CN.json`.
- [ ] Check text fit in Chinese:
  - Sidebar section label: `精选页面`.
  - Ask AI button: icon-only with tooltip.
  - Export bookmark labels: tooltip text only.

Expected result: UI remains compact and Codex-like in both languages.

## Task 14: Integration Tests And Manual Review

Cover cross-feature behavior that individual unit tests miss.

- [ ] Add or extend component tests:
  - [ ] `LeftSidebar` renders favorites section between Workflow and Recent Pages.
  - [ ] Clicking a Wiki favorite calls `openPage` and sets `activeView = "wiki"`.
  - [ ] Clicking an Export favorite sets `activeView = "exports"` and calls `loadPreview`.
  - [ ] Ask AI button opens right panel mode.
  - [ ] Page switch updates `wikiAssistantPagePath`.
- [ ] Add backend compile checks:

```powershell
cargo check --manifest-path .\src-tauri\Cargo.toml --lib --tests
```

- [ ] Run frontend focused tests during implementation:

```powershell
npm run test -- src/features/wiki/wiki.test.tsx src/features/exports/exportsView.test.tsx src/stores/chatStore.test.ts src/features/chat/chatView.test.tsx
```

- [ ] Run full checks from project root:

```powershell
npm run test
npm run lint
npm run build
cargo check --manifest-path .\src-tauri\Cargo.toml --lib --tests
```

- [ ] Confirm no unintended `console.log` remains:

```powershell
Get-ChildItem -Path .\src,.\src-tauri\src -Recurse -File |
  Select-String -Pattern "console\.log" |
  Where-Object { $_.Path -notmatch "\\node_modules\\" }
```

- [ ] Verify import paths resolve through `npm run build` and `cargo check`.
- [ ] Launch review subagents per `AGENTS.md` after implementation:
  - [ ] Subagent A with shared context: design intent, logic, consistency, docs integration.
  - [ ] Subagent B fresh context: blind spots, missing tests, unclear behavior.
  - [ ] Merge findings, fix valid issues, rerun all checks.
- [ ] If subagents are unavailable, perform the two reviews manually and record that in final delivery.
- [ ] Add a top entry to `SPEC/progress.txt` after implementation completes.

Expected result: batch 03 is verified across backend contracts, frontend interactions, and design constraints before delivery.

---

## Acceptance Criteria

### Wiki Bookmarks

- WHEN the user clicks the Wiki page star in the toolbar THEN the system SHALL write or remove a `wiki_page` entry in `.app/bookmarks.json`.
- WHEN the user toggles a Wiki page favorite THEN the system SHALL NOT modify the Markdown file or its frontmatter.
- WHEN a Wiki page is bookmarked THEN the system SHALL show a star on the corresponding file row in the Wiki tree immediately and after refresh.
- WHEN a Wiki page is unbookmarked THEN the system SHALL remove it from the left sidebar `精选页面` section immediately and after refresh.
- WHEN `.app/bookmarks.json` is an old `["wiki/...md"]` array THEN the system SHALL read it and expose equivalent Wiki page bookmarks.
- WHEN a page has frontmatter `starred: true` but no app bookmark THEN the system SHALL keep the compatibility star signal in the Wiki tree but SHALL NOT treat it as a user sidebar favorite.
- WHEN a project is switched THEN the system SHALL reload bookmarks from the active project only.

### HTML Export Favorites

- WHEN the user clicks the star on a succeeded export row THEN the system SHALL persist an `export_html` bookmark keyed by `exportRecordId`.
- WHEN the user reloads the app or project THEN the system SHALL mark the same export record as bookmarked from `.app/bookmarks.json`.
- WHEN the user clicks a bookmarked HTML export in `精选页面` THEN the system SHALL switch to Exports and load that record's preview.
- WHEN a bookmarked HTML export file is missing THEN the system SHALL keep the favorite visible and SHALL show a recoverable preview/open error instead of crashing.
- WHEN an export record has status `failed` THEN the system SHALL NOT offer a favorite star for that row.

### Chat Retrieval

- WHEN the user asks `什么是约束先行2` and a relevant Wiki page title or alias contains `约束先行` or `约束先行2` THEN the system SHALL include that page in Chat citations.
- WHEN the user asks a Chinese natural-language question with punctuation or question words THEN the system SHALL normalize the query into deterministic local search terms.
- WHEN no Wiki page matches the normalized query terms THEN the system SHALL keep citations empty and allow the model prompt to state context is insufficient.
- WHEN ordinary topbar/wiki search runs THEN the system SHALL remain local keyword/filter search and SHALL NOT call Agent or BYOK.

### Wiki Page Ask AI

- WHEN the user clicks the Ask AI icon on a Wiki page THEN the system SHALL open the right panel in Wiki assistant mode for the current page.
- WHEN the user sends a question from the Wiki assistant panel THEN the system SHALL call `send_chat_message` with `pinnedPagePath` equal to the current page path.
- WHEN the backend builds retrieval context with `pinnedPagePath` THEN the system SHALL put the current Wiki page first in sources and citations.
- WHEN search also returns the pinned page THEN the system SHALL dedupe it so the page appears once.
- WHEN the user navigates to another Wiki page while the assistant panel is open THEN the system SHALL update the pinned page path to the newly selected page.
- WHEN Agent/BYOK is unconfigured THEN the system SHALL show the same recoverable Chat configuration error used by global Chat.
- WHEN the user uses global Chat after using page Ask AI THEN the system SHALL continue to send global Chat requests without `pinnedPagePath`.

### Safety And Quality

- WHEN bookmarks are toggled THEN the system SHALL write only `.app/bookmarks.json`.
- WHEN export records are listed THEN the system SHALL derive `bookmarked` from BookmarkService and SHALL NOT duplicate bookmark persistence in `.app/exports.json`.
- WHEN implementation completes THEN the system SHALL pass `npm run test`, `npm run lint`, `npm run build`, and `cargo check --manifest-path .\src-tauri\Cargo.toml --lib --tests`, or SHALL report the exact blocking command and error.
- WHEN implementation completes THEN the system SHALL have no unintended `console.log` in `src/` or `src-tauri/src/`.

## Out Of Scope

- Vector search, embeddings, semantic index files, or database-backed content storage.
- Editing or removing Markdown `starred` frontmatter.
- Custom favorite folders, drag ordering, tags, or collections.
- Browser/maximized export preview workflow from batch 04.
- New Agent installation flow or silent Agent command execution.
- Raw source citation enrichment beyond existing Chat citation fields.
- Full Chat session redesign.

## Execution Recommendation

Implement in this order:

1. Backend BookmarkService and Wiki scan join.
2. Export bookmark backend/store/UI.
3. Sidebar favorites selector/UI.
4. Search normalization retrieval fix.
5. Pinned page Chat DTO/backend.
6. PageChatPanel and Wiki Ask AI UI.
7. Styling/i18n polish.
8. Full verification and two-pass review.

This order keeps persistence stable first, then builds UI on typed backend behavior, then fixes Chat retrieval, then layers the page-scoped assistant on top of the shared Chat pipeline.
