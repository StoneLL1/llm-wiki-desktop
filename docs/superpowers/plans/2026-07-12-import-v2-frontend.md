# Import V2 Frontend Implementation Plan

> Historical implementation plan. Keep task-level code and test guidance only where it agrees with [`../specs/2026-07-24-import-source-media-flow-design.md`](../specs/2026-07-24-import-source-media-flow-design.md), which is the sole authority for current Import, Source, media, OCR / ASR, login, and AI 整理 behavior.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the legacy Import view with a compact, accessible Import V2 workspace where users can repeatedly add files, folders, or exact URLs, follow independent item progress, resolve required actions, preview Markdown and quality evidence, request Agent assistance, and explicitly confirm selected results.

**Architecture:** Keep the existing `AppShell -> WorkspaceController -> WorkspaceRouter -> lazy ImportView` boundary and evolve the focused `useImportWorkflow` plus `importStore` to consume the final typed Import V2 contracts. React owns presentation state only; thin Tauri presentation commands may read bounded preview/history DTOs, while all filesystem, URL security, capability, Agent, migration, task, and commit decisions remain in Rust services delivered by the first five Import V2 packages. The visual structure follows `UI-Frontend-design/import-v2-reference.html` inside the repository's authoritative Codex-like shell, tokens, right inspector, task drawer, and responsive pane behavior.

**Tech Stack:** React 19, TypeScript, Zustand, react-i18next, Tailwind CSS v4 plus `src/styles.css` tokens, Lucide React, Tauri v2 typed IPC, Vitest, Testing Library.

## Global Constraints

- Execute only after Import Core, File Ingestion, Web Ingestion, Agent Assistance, and Migration & Cutover are integrated, green, and recorded in `SPEC/progress.txt`.
- Before implementation, compare this plan with the final Rust/TypeScript DTOs. Preserve final wire names; update this plan's frontend adapter names only where the accepted backend deliberately changed them.
- Keep `AppShell`, `WorkspaceController`, `WorkspaceRouter`, `useImportWorkflow`, the global `taskStore`, `TaskLogDrawer`, `ProjectConfirmationController`, and `ViewErrorBoundary` ownership boundaries.
- React must not read arbitrary filesystem paths, run Git/Agent/capability processes, hold secrets, perform URL fetches, or decide commit destinations.
- Every valid backend task is upserted globally. After every `await`, project-scoped store, drawer, navigation, dialog, and toast commits require the initiating `projectId + rootPath`; supersedable session reads also require an epoch.
- All file, folder, URL, login, private-target, capability, Agent, BYOK, migration, and commit actions use typed Tauri IPC. Do not restore legacy `preview_import`, `fetch_import_url`, `preview_text_import`, or `confirm_import_preview` calls.
- Use only `src/styles.css` tokens. UI text is 13px body, 12px secondary, 11px muted/mono, and 10.5px micro-label; no hardcoded hex, gradients, glossy AI visuals, or marketing hero layout.
- Preserve the design shell dimensions: topbar 48px, main header 52px, right-panel header 52px, status bar 28px, panel header 44px.
- All visible copy must exist in both `src/i18n/locales/en.json` and `src/i18n/locales/zh-CN.json`; backend codes are translated in the frontend, never displayed as raw primary copy.
- The current session is an accumulative draft: repeated drops and URL additions append; parsing starts per item; only selected preview-ready items commit.
- Do not create empty Markdown artifacts for failures. Deselecting or skipping an uncommitted draft item is not source deletion; deleting/replacing an imported source remains a separate confirmed backend workflow.
- `waiting_login`, captcha, private target, BYOK, paid work, capability download, merge, source replacement, and commit are never auto-approved by the UI.
- Do not modify `UI-Frontend-design/`; it is reference material, not application source.

## Approved Interaction Model

```text
Import view opens
  -> recover unfinished V2 session, otherwise create a draft
  -> user repeatedly adds files/folders or exact URLs
  -> each item scans/extracts independently in the background
  -> queue exposes progress, warnings, failures, and required actions
  -> selected item drives the right inspector and Markdown preview
  -> user selects preview-ready items and resolves merge decisions
  -> explicit confirmation starts Core commit task
  -> committed items remain visible; partial failures stay actionable
```

The main surface contains a compact 52px page header, two bordered method panes, and one queue pane. The right panel is the normal detail surface; a modal is reserved for full Markdown/Diff inspection and explicit BYOK/capability/migration confirmations.

---

### Task 1: Freeze the Frontend Presentation Contract and API Adapter

**Files:**

- Create: `src-tauri/src/models/import_v2_presentation.rs`
- Create: `src-tauri/src/commands/import_v2_presentation_commands.rs`
- Modify: `src-tauri/src/models/mod.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Create: `src/types/importV2Presentation.ts`
- Create: `src/services/importV2Api.ts`
- Create: `src/services/importV2Api.test.ts`
- Create: `src-tauri/tests/import_v2_presentation_commands.rs`

**Interfaces:**

- Consumes final `ImportSession`, `ImportItem`, `ImportIssue.availableActions`, file/web/Agent/migration DTOs, and existing `BackendTask`.
- Produces bounded read DTOs `ImportPreviewContent`, `ImportHistoryPage`, `ImportFrontendReadiness`, and a single typed `importV2Api` object.
- Adds no parsing, routing, Agent, installation, migration, or commit business logic.

- [ ] **Step 1: Write failing Rust and TypeScript contract tests**

```rust
#[test]
fn preview_content_is_bounded_and_never_accepts_a_path() {
    let request = GetImportPreviewContentV2Request {
        project_id: "p1".into(),
        project_root_path: "D:/wiki".into(),
        session_id: "s1".into(),
        item_id: "i1".into(),
        candidate_id: None,
    };
    let value = serde_json::to_value(request).unwrap();
    assert!(value.get("relativePath").is_none());
    assert!(value.get("absolutePath").is_none());
}
```

```ts
expect(importV2Api.commandNames).toEqual({
  createSession: "create_import_session_v2",
  getSession: "get_import_session_v2",
  addItems: "add_import_items_v2",
  addPaths: "add_import_paths_v2",
  addUrl: "add_import_url_v2",
  setSelection: "set_import_item_selection_v2",
  startItems: "start_import_items_v2",
  confirmSession: "confirm_import_session_v2",
  getPreviewContent: "get_import_preview_content_v2",
  getReadiness: "get_import_frontend_readiness_v2",
});
```

- [ ] **Step 2: Run tests and verify RED**

Run:

```text
npm run test -- src/services/importV2Api.test.ts
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --test import_v2_presentation_commands
```

Expected: adapter and presentation DTO modules do not exist.

- [ ] **Step 3: Implement bounded presentation DTOs**

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreviewContent {
    pub session_id: String,
    pub item_id: String,
    pub candidate_id: Option<String>,
    pub title: String,
    pub markdown: String,
    pub truncated: bool,
    pub total_bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportFrontendReadiness {
    pub backend_version: String,
    pub active: bool,
    pub migration_status: MigrationStatus,
    pub unfinished_session_id: Option<String>,
    pub legacy_history_available: bool,
}
```

`get_import_preview_content_v2` resolves content only from the session/item/candidate identity, reuses Core staging containment, rejects non-preview states, and returns at most 2 MiB. `get_import_frontend_readiness_v2` composes existing activation/migration/session facts without mutating them. `list_import_history_v2` returns cursor-paginated V2 plus `legacyReadOnly` projections, 50 entries per page.

- [ ] **Step 4: Implement one typed adapter over `invoke`**

```ts
export const importV2Api = {
  createSession: (request: CreateImportSessionV2Request) =>
    invoke<ImportSession>("create_import_session_v2", { request }),
  getSession: (request: GetImportSessionV2Request) =>
    invoke<ImportSession>("get_import_session_v2", { request }),
  addPaths: (request: AddImportPathsV2Request) =>
    invoke<ImportSession>("add_import_paths_v2", { request }),
  addUrl: (request: AddImportUrlV2Request) =>
    invoke<ImportSession>("add_import_url_v2", { request }),
  setSelection: (request: SetImportItemSelectionV2Request) =>
    invoke<ImportSession>("set_import_item_selection_v2", { request }),
  startItems: (request: StartImportItemsV2Request) =>
    invoke<BackendTask[]>("start_import_items_v2", { request }),
  confirmSession: (request: CommitImportSessionRequest) =>
    invoke<BackendTask>("confirm_import_session_v2", { request }),
  getPreviewContent: (request: GetImportPreviewContentV2Request) =>
    invoke<ImportPreviewContent>("get_import_preview_content_v2", { request }),
} as const;
```

Add explicit methods for `authorize_import_private_target_v2`, `begin_import_login_v2`, `revoke_import_login_v2`, `get_import_capability_requirement_v2`, `install_import_capability_v2`, `get_import_agent_policy_v2`, `set_import_agent_policy_v2`, `start_import_agent_assistance_v2`, `preview_import_byok_scope_v2`, `approve_import_byok_assistance_v2`, `select_import_agent_candidate_v2`, `discard_import_agent_candidate_v2`, the five migration commands, and `list_import_history_v2`. If the accepted backend lacks the two capability command wrappers, add them to `import_v2_presentation_commands.rs` as thin delegates to the existing capability manager; they may not download or verify inside the command. Do not expose a generic `invokeCommand(name, payload)` escape hatch.

- [ ] **Step 5: Verify command registration, payload casing, and secret denial**

Tests reject arbitrary paths, unknown item IDs, oversized preview content, secret-bearing readiness/history output, and mismatched project/session identity. TypeScript tests assert every method calls one exact command and forwards one typed `request` object.

- [ ] **Step 6: Run focused tests and commit**

```bash
git add src-tauri/src/models/import_v2_presentation.rs src-tauri/src/commands/import_v2_presentation_commands.rs src-tauri/src/models/mod.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs src/types/importV2Presentation.ts src/services/importV2Api.ts src/services/importV2Api.test.ts src-tauri/tests/import_v2_presentation_commands.rs
git commit -m "feat(import-ui): add bounded v2 presentation api"
```

---

### Task 2: Replace Legacy Preview State with a Recoverable Session Store

**Files:**

- Modify: `src/stores/importStore.ts`
- Create: `src/stores/importStore.test.ts`
- Create: `src/features/import/importViewModel.ts`
- Create: `src/features/import/importViewModel.test.ts`

**Interfaces:**

- Consumes `ImportSession`, `ImportItem`, `ImportItemStatus`, `ImportFrontendReadiness`.
- Produces `ImportQueueFilter`, stable selectors, dialog state keyed by item ID, and project-scoped session state.

- [ ] **Step 1: Write failing store transition tests**

```ts
it("resets presentation on project change without erasing backend task facts", () => {
  useImportStore.getState().attachSession(projectAKey, sessionA);
  useImportStore.getState().resetProjectPresentation(projectBKey);
  expect(useImportStore.getState().session).toBeNull();
  expect(useTaskStore.getState().tasks).toEqual(existingTasks);
});
```

Cover recovered session, repeated item append, same-item replacement from refreshed backend state, selected item disappearance, filter changes, preview dialog, BYOK dialog, capability dialog, login dialog, migration dialog, and stale epoch rejection.

- [ ] **Step 2: Run and verify RED**

Run: `npm run test -- src/stores/importStore.test.ts src/features/import/importViewModel.test.ts`

Expected: V2 state and selectors are missing.

- [ ] **Step 3: Implement normalized presentation state**

```ts
export type ImportQueueFilter =
  | "all"
  | "active"
  | "ready"
  | "needs_action"
  | "failed"
  | "completed";

interface ImportState {
  projectKey: string | null;
  session: ImportSession | null;
  selectedItemId: string | null;
  filter: ImportQueueFilter;
  isBootstrapping: boolean;
  mutationKeys: ReadonlySet<string>;
  previewItemId: string | null;
  byokItemId: string | null;
  capabilityItemId: string | null;
  loginItemId: string | null;
  migrationDialogOpen: boolean;
}
```

Store only public DTOs and UI identities. Never store cookies, Authorization values, provider keys, browser profile paths, raw BYOK send contents, absolute staging paths, or preview Markdown after its dialog closes.

- [ ] **Step 4: Implement pure queue selectors**

`selectVisibleItems`, `selectQueueCounts`, `selectCommittableItems`, and `selectSessionProgress` return memoizable results. Map `waiting_capability`, `waiting_login`, `needs_merge`, and `issue.userActionRequired` to `needs_action`; never collapse them into generic failure.

- [ ] **Step 5: Verify GREEN and commit**

```bash
git add src/stores/importStore.ts src/stores/importStore.test.ts src/features/import/importViewModel.ts src/features/import/importViewModel.test.ts
git commit -m "refactor(import-ui): model recoverable v2 sessions"
```

---

### Task 3: Rebuild File/Folder and URL Entry as Two Compact Method Panes

**Files:**

- Create: `src/features/import/ImportSourceMethods.tsx`
- Create: `src/features/import/ImportSourceMethods.test.tsx`
- Modify: `src/features/import/nativeFilePicker.ts`
- Modify: `src/features/import/nativeFilePicker.test.ts`
- Modify: `src/features/import/dragDrop.ts`
- Modify: `src/features/import/dragDrop.test.ts`
- Delete after replacement: `src/features/import/ImportUrlDialog.tsx`
- Delete after replacement: `src/features/import/OpenFolderAsProjectDialog.tsx`

**Interfaces:**

- Consumes `onAddPaths(paths: string[])`, `onAddUrl(url: string)`, and mutation flags from `ImportWorkflow`.
- Produces no filesystem objects; native Tauri file/folder selection returns path strings directly to the workflow.

- [ ] **Step 1: Write failing interaction tests from the reference HTML**

Test keyboard activation of the dropzone, multi-file picker, folder picker, native file/folder drag-drop, a second drop appending instead of replacing, CJK paths, URL paste, Enter submit, platform hint, unsupported local URL text, and visible supported format/platform lists.

```ts
fireEvent.drop(screen.getByRole("button", { name: /drop files or folders/i }), {
  dataTransfer: { files: [], items: [] },
});
expect(onAddPaths).not.toHaveBeenCalledWith([]);
```

- [ ] **Step 2: Run and verify RED**

Run: `npm run test -- src/features/import/ImportSourceMethods.test.tsx src/features/import/dragDrop.test.ts src/features/import/nativeFilePicker.test.ts`

- [ ] **Step 3: Implement the two-pane composition**

```tsx
<section className="import-v2-methods" aria-label={t("importV2.methods.label")}>
  <FileMethodPane onAddPaths={onAddPaths} disabled={addingPaths} />
  <UrlMethodPane onAddUrl={onAddUrl} disabled={addingUrl} />
</section>
```

Match the reference content hierarchy: eyebrow, title, one-sentence description, dropzone/URL form, explicit buttons, supported chips, and a quiet footnote. Use two flat bordered panes, not nested marketing cards. File support text includes MD/Markdown, DOC/DOCX, XLS/XLSX, PPT/PPTX, PDF. Platform chips distinguish stage-one available connectors from phase-two unavailable connectors using readiness facts; never advertise an unavailable platform as active.

- [ ] **Step 4: Keep native security behavior explicit**

Use the official dialog plugin for `multiple: true` files and `directory: true` folder selection. Native drag-drop subscription errors create one actionable toast. Browser DOM drag data never attempts recursive filesystem access; Tauri supplies authorized paths.

- [ ] **Step 5: Verify focus, loading, and repeat-add behavior**

Inputs remain usable while other items parse. Only the currently submitted method button shows local pending state. URL input is cleared after backend acceptance, not before; on rejection it preserves the user's URL.

- [ ] **Step 6: Commit**

```bash
git add src/features/import/ImportSourceMethods.tsx src/features/import/ImportSourceMethods.test.tsx src/features/import/nativeFilePicker.ts src/features/import/nativeFilePicker.test.ts src/features/import/dragDrop.ts src/features/import/dragDrop.test.ts
git rm src/features/import/ImportUrlDialog.tsx src/features/import/OpenFolderAsProjectDialog.tsx
git commit -m "feat(import-ui): add file folder and url entry panes"
```

---

### Task 4: Build the Persistent Import Queue, Filters, Selection, and Item Actions

**Files:**

- Create: `src/features/import/ImportQueue.tsx`
- Create: `src/features/import/ImportQueue.test.tsx`
- Create: `src/features/import/ImportItemStatus.tsx`
- Create: `src/features/import/ImportItemActions.tsx`
- Create: `src/features/import/importStatusPresentation.ts`
- Create: `src/features/import/importStatusPresentation.test.ts`

**Interfaces:**

- Consumes visible `ImportItem[]`, counts, selected item ID, and explicit workflow action callbacks.
- Produces item selection, inspector selection, retry/start/cancel/Agent/login/capability/preview/deselect intents; it invokes no backend API directly.

- [ ] **Step 1: Write failing state-matrix tests**

Cover every Core status: `queued`, `inspecting`, `waiting_capability`, `waiting_login`, `extracting`, `validating`, `preview_ready`, `needs_merge`, `committing`, `completed`, `paused`, `cancelled`, `skipped`, and `failed`. Assert label, icon, progress, primary action, secondary actions, row selectability, and commit checkbox state.

- [ ] **Step 2: Run and verify RED**

Run: `npm run test -- src/features/import/importStatusPresentation.test.ts src/features/import/ImportQueue.test.tsx`

- [ ] **Step 3: Implement the queue header and filters**

Use `全部 / 处理中 / 可提交 / 需处理 / 失败 / 已完成`. Show total items, ready count, failed count, and combined measurable progress without layout shift. The empty state offers “选择文件”, “选择文件夹”, and URL focus actions through callbacks.

- [ ] **Step 4: Implement compact accessible rows**

Each row shows source/platform icon, title or filename, file type/domain and route, status/progress, warning or error summary, and icon actions with tooltips. Checkbox selection is available only for backend-selectable preview/merge items. Row click selects the inspector; buttons stop propagation.

- [ ] **Step 5: Implement action derivation without raw-code switches in JSX**

```ts
export interface ImportItemPresentation {
  tone: "neutral" | "accent" | "warning" | "danger";
  labelKey: string;
  progressMode: "none" | "indeterminate" | "measured";
  actions: readonly ImportItemAction[];
  committable: boolean;
}

export type ImportItemAction =
  | "inspect"
  | "retry"
  | "cancel"
  | "preview_markdown"
  | "begin_login"
  | "authorize_private_target"
  | "view_capability"
  | "invoke_local_agent"
  | "request_byok"
  | "compare_candidate"
  | "discard_candidate"
  | "resolve_merge"
  | "open_result";
```

Use typed backend `availableActions` plus status to expose only supported actions. A failed item may show retry and “Agent 辅助”; `waiting_login` shows “继续登录”; `waiting_capability` shows “查看能力包”; `needs_merge` shows “查看 Diff”; active tasks show cancel; completed items show preview/open-result. Unchecking an item calls only `set_import_item_selection_v2`; no queue action calls source deletion.

- [ ] **Step 6: Test partial success and stable row identity**

Refreshing one item must not reset filter, scroll, selected item, or other checkbox states. A completed item remains visible while a failed sibling is retried.

- [ ] **Step 7: Commit**

```bash
git add src/features/import/ImportQueue.tsx src/features/import/ImportQueue.test.tsx src/features/import/ImportItemStatus.tsx src/features/import/ImportItemActions.tsx src/features/import/importStatusPresentation.ts src/features/import/importStatusPresentation.test.ts
git commit -m "feat(import-ui): render actionable import queue"
```

---

### Task 5: Rebuild `useImportWorkflow` Around Import V2 Sessions and Tasks

**Files:**

- Modify: `src/features/import/useImportWorkflow.ts`
- Replace: `src/features/import/useImportWorkflow.test.tsx`
- Modify: `src/components/app/WorkspaceController.tsx`
- Modify: `src/components/app/WorkspaceRouter.tsx`
- Modify: `src/hooks/useTaskEvents.ts`
- Modify: `src/hooks/useTaskEvents.test.tsx`

**Interfaces:**

- Consumes `importV2Api`, project identity, active view, `TaskLauncher.cancel`, `taskStore`, toast store, and wiki store.
- Produces one stable `ImportWorkflow` facade used by the lazy `ImportView`; heavy view components do not enter the initial bundle.

- [ ] **Step 1: Write failing bootstrap and project-switch tests**

Test: readiness -> recover unfinished session; no unfinished session -> create balanced draft; rapid A/B project switch; late A session; repeated add paths/URL; backend task upsert during stale project; stale drawer/toast suppression; app restart recovery; active-view re-entry refresh.

- [ ] **Step 2: Run and verify RED**

Run: `npm run test -- src/features/import/useImportWorkflow.test.tsx src/hooks/useTaskEvents.test.tsx`

- [ ] **Step 3: Implement bootstrap with project key and epoch**

```ts
export interface ImportWorkflow {
  session: ImportSession | null;
  readiness: ImportFrontendReadiness | null;
  bootstrapState: "loading" | "ready" | "blocked" | "error";
  addPaths(paths: string[]): Promise<void>;
  addUrl(url: string): Promise<void>;
  setItemSelected(itemId: string, selected: boolean): Promise<void>;
  startItems(itemIds: string[]): Promise<void>;
  retryItem(itemId: string): Promise<void>;
  cancelItem(itemId: string): Promise<void>;
  confirm(decisions: CommitItemDecision[]): Promise<void>;
  refreshSession(): Promise<void>;
}
```

After backend accepts added inputs, start only new queued item IDs and upsert every task returned by `start_import_items_v2`. Do not await terminal completion to keep the view responsive.

- [ ] **Step 4: Reconcile task events into session state**

On matching Import task progress/terminal events, refresh the affected session through an epoch-coalesced read. Always upsert task facts globally; only update current Import presentation when project and session identities match.

- [ ] **Step 5: Implement commit ordering and partial results**

`confirm_import_session_v2` returns a task immediately. On success, refresh session, then `wikiStore.scan(projectId, rootPath)`. Compilation is no longer hidden inside Import confirmation; if the final product retains “导入后编译”, launch it only after scan and only for the still-current project.

- [ ] **Step 6: Prove legacy calls are gone**

An architecture test scans frontend source and fails on `preview_import`, `fetch_import_url`, `preview_text_import`, and `confirm_import_preview` outside historical documentation/tests.

- [ ] **Step 7: Run focused suite and commit**

```bash
git add src/features/import/useImportWorkflow.ts src/features/import/useImportWorkflow.test.tsx src/components/app/WorkspaceController.tsx src/components/app/WorkspaceRouter.tsx src/hooks/useTaskEvents.ts src/hooks/useTaskEvents.test.tsx
git commit -m "refactor(import-ui): orchestrate v2 sessions and tasks"
```

---

### Task 6: Turn the Right Panel into a Source, Quality, Attempts, and Markdown Inspector

**Files:**

- Modify: `src/features/import/ImportRightPanel.tsx`
- Create: `src/features/import/ImportRightPanel.test.tsx`
- Create: `src/features/import/ImportMarkdownPreviewDialog.tsx`
- Create: `src/features/import/ImportMarkdownPreviewDialog.test.tsx`
- Create: `src/features/import/ImportQualitySummary.tsx`
- Create: `src/features/import/ImportAttemptTimeline.tsx`

**Interfaces:**

- Consumes the selected `ImportItem`, bounded `ImportPreviewContent`, and workflow preview/Diff/action callbacks.
- Produces read-only detail presentation; no component receives an arbitrary filesystem path.

- [ ] **Step 1: Write failing inspector tests**

Cover no selection, local file, generic URL, platform video, quality pass/warning/fail, multiple attempts, source version/update, duplicate, preview truncation, failed preview fetch, and a fast A-item/B-item switch where A content resolves late.

- [ ] **Step 2: Run and verify RED**

Run: `npm run test -- src/features/import/ImportRightPanel.test.tsx src/features/import/ImportMarkdownPreviewDialog.test.tsx`

- [ ] **Step 3: Implement inspector sections**

Sections are: selected source; route/status; source/version/provenance; quality metrics and warnings; attempt timeline; output assets; actions. Replace the old hardcoded archive-rule list with actual backend-derived destination/provenance facts. Paths use the existing path display helper and preserve full text in `title`.

- [ ] **Step 4: Implement safe Markdown preview**

Load preview content by session/item/candidate identity only when opened. Render with the existing sanitized Markdown stack (`remark-gfm`, math, KaTeX, highlight); external images/links do not auto-fetch unsafe targets. Provide Copy Markdown, hash/size, truncation notice, and close. Clear content on close and project/item change.

- [ ] **Step 5: Add error and accessibility behavior**

Dialog traps focus, closes on Escape, restores trigger focus, uses `aria-modal`, and exposes loading/error states with `role="status"`/`role="alert"`. A fetch error leaves the queue and inspector usable.

- [ ] **Step 6: Commit**

```bash
git add src/features/import/ImportRightPanel.tsx src/features/import/ImportRightPanel.test.tsx src/features/import/ImportMarkdownPreviewDialog.tsx src/features/import/ImportMarkdownPreviewDialog.test.tsx src/features/import/ImportQualitySummary.tsx src/features/import/ImportAttemptTimeline.tsx
git commit -m "feat(import-ui): inspect quality attempts and markdown"
```

---

### Task 7: Add Agent Assistance Policy, Manual Recovery, BYOK Approval, and Diff Review

**Files:**

- Create: `src/features/import/ImportAgentControls.tsx`
- Create: `src/features/import/ImportAgentControls.test.tsx`
- Create: `src/features/import/ImportByokApprovalDialog.tsx`
- Create: `src/features/import/ImportByokApprovalDialog.test.tsx`
- Create: `src/features/import/ImportCandidateDiffDialog.tsx`
- Create: `src/features/import/ImportCandidateDiffDialog.test.tsx`
- Modify: `src/features/import/useImportWorkflow.ts`

**Interfaces:**

- Consumes final `AgentAssistancePolicy`, `AgentRecoveryAction`, `AgentSendScope`, `AgentCandidate`, and `AgentCandidateDiff` DTOs.
- Produces explicit policy update, local assistance, BYOK preview/approval, candidate selection/discard, and merge-decision intents.

- [ ] **Step 1: Write failing policy and action tests**

Assert “失败自动调用 Agent 辅助” defaults to backend policy, changes only after backend success, displays no auto-BYOK implication, and remains project-scoped. Failed items show manual local Agent when available and BYOK only when explicitly requested.

- [ ] **Step 2: Write failing BYOK approval tests**

The dialog displays provider, model, exact file list, byte/token estimates, redactions, estimated cost when available, scope expiry, and Confirm/Cancel. It never renders or stores an API key. A changed/expired scope forces a new preview.

- [ ] **Step 3: Write failing Diff tests**

Render deterministic baseline versus Agent candidate and, for edited Wiki, baseline/current/candidate three-way information. Actions are: choose deterministic result, choose Agent candidate, apply backend-provided merged candidate, keep current Wiki, create new document, discard Agent candidate. No choice writes until normal Import confirmation.

- [ ] **Step 4: Implement controls using exact Agent commands**

Use `get_import_agent_policy_v2`, `set_import_agent_policy_v2`, `start_import_agent_assistance_v2`, `preview_import_byok_scope_v2`, `approve_import_byok_assistance_v2`, `select_import_agent_candidate_v2`, and `discard_import_agent_candidate_v2`. Every returned task enters `taskStore`; project-switch guards suppress stale dialogs/toasts.

- [ ] **Step 5: Test cancellation, failure, and deterministic baseline preservation**

Cancellation closes only active progress affordances, not the underlying item. Agent failure leaves the deterministic candidate and original issue visible. No frontend code sends raw project paths or writes Diff results.

- [ ] **Step 6: Commit**

```bash
git add src/features/import/ImportAgentControls.tsx src/features/import/ImportAgentControls.test.tsx src/features/import/ImportByokApprovalDialog.tsx src/features/import/ImportByokApprovalDialog.test.tsx src/features/import/ImportCandidateDiffDialog.tsx src/features/import/ImportCandidateDiffDialog.test.tsx src/features/import/useImportWorkflow.ts
git commit -m "feat(import-ui): review agent assistance candidates"
```

---

### Task 8: Add Login, Private-Target, and Capability-Pack User Gates

**Files:**

- Create: `src/features/import/ImportLoginDialog.tsx`
- Create: `src/features/import/ImportLoginDialog.test.tsx`
- Create: `src/features/import/ImportPrivateTargetDialog.tsx`
- Create: `src/features/import/ImportPrivateTargetDialog.test.tsx`
- Create: `src/features/import/ImportCapabilityDialog.tsx`
- Create: `src/features/import/ImportCapabilityDialog.test.tsx`
- Modify: `src/features/import/useImportWorkflow.ts`

**Interfaces:**

- Consumes final Web target/login and `CapabilityRequirement`/pack manifest DTOs.
- Produces one-time target authorization, begin/revoke dedicated login session, and explicit capability installation intents.

- [ ] **Step 1: Write failing login/captcha tests**

Show connector, public domain, dedicated-profile explanation, Begin Login, Check Again, Revoke, and Cancel. Captcha text says the user must complete it themselves. Never request a password or copy cookies into the application form.

- [ ] **Step 2: Write failing private-target tests**

Display normalized final target, resolved address category, reason for blocking, exact one-item/one-target scope, and authorize/cancel. Redirecting to a different private target requires a new dialog. The primary default is Cancel.

- [ ] **Step 3: Write failing capability tests**

Display capability purpose, source, pinned version, compressed/installed/model size, disk requirement, license, supported platform, and fallback. Install/update requires explicit confirmation and returns a cancellable backend task. Unsupported platform offers fallback, not a dead Install button.

- [ ] **Step 4: Implement typed command flows**

Use `begin_import_login_v2`, `revoke_import_login_v2`, `authorize_import_private_target_v2`, `get_import_capability_requirement_v2`, and `install_import_capability_v2`. The UI never downloads, unpacks, verifies, launches, or deletes a pack itself.

- [ ] **Step 5: Verify secret and project-switch boundaries**

Scan rendered props, Zustand state, task summaries, toasts, and snapshots for seeded cookies, Authorization, signed query parameters, home-directory profile paths, and provider keys. Late dialog results from project A cannot open in project B.

- [ ] **Step 6: Commit**

```bash
git add src/features/import/ImportLoginDialog.tsx src/features/import/ImportLoginDialog.test.tsx src/features/import/ImportPrivateTargetDialog.tsx src/features/import/ImportPrivateTargetDialog.test.tsx src/features/import/ImportCapabilityDialog.tsx src/features/import/ImportCapabilityDialog.test.tsx src/features/import/useImportWorkflow.ts
git commit -m "feat(import-ui): gate login targets and capabilities"
```

---

### Task 9: Surface Migration, Activation, and Read-Only Legacy History

**Files:**

- Create: `src/features/import/ImportMigrationNotice.tsx`
- Create: `src/features/import/ImportMigrationNotice.test.tsx`
- Create: `src/features/import/ImportMigrationDialog.tsx`
- Create: `src/features/import/ImportMigrationDialog.test.tsx`
- Create: `src/features/import/ImportHistoryPanel.tsx`
- Create: `src/features/import/ImportHistoryPanel.test.tsx`
- Modify: `src/features/import/useImportWorkflow.ts`

**Interfaces:**

- Consumes final `MigrationStatus`, dry-run report, apply task, activation/readiness, and paginated history DTOs.
- Produces scan, dry-run review, explicit apply, resume, and history pagination intents; activation is never inferred from local UI state.

- [ ] **Step 1: Write failing migration-state tests**

Cover not scanned, scanning, dry-run ready, awaiting confirmation, applying, interrupted/resumable, applied/not activated, activated, and verification failed. Import entry is blocked only when backend readiness says writes are unsafe; a failed migration never silently falls back to V1 writes.

- [ ] **Step 2: Write failing report/confirmation tests**

The dialog shows automatic links, proposed records, conflicts, unmanaged items, warnings, metadata paths affected, content paths guaranteed untouched, Git checkpoint state, and rollback explanation. Apply is disabled until the user explicitly confirms the report fingerprint.

- [ ] **Step 3: Implement through `importV2MigrationApi`**

Reuse `scan_import_v2_migration`, `plan_import_v2_migration`, `apply_import_v2_migration`, `get_import_v2_migration_status`, and `resume_import_v2_migration`. Persisted status comes from backend reads/tasks; the dialog closing does not cancel or lose the migration.

- [ ] **Step 4: Implement read-only legacy history labels**

Legacy entries carry a visible `只读旧记录 / Read-only legacy` badge and support detail viewing only. Retry, delete, replace, or commit buttons are absent. V2 history entries may open result, logs, and source/version details according to backend capabilities.

- [ ] **Step 5: Verify no dual-write or casual backend toggle**

An architecture test rejects legacy mutation command names in the active frontend. There is no UI switch between V1/V2. Emergency disabled state explains release rollback rather than enabling V1 writes.

- [ ] **Step 6: Commit**

```bash
git add src/features/import/ImportMigrationNotice.tsx src/features/import/ImportMigrationNotice.test.tsx src/features/import/ImportMigrationDialog.tsx src/features/import/ImportMigrationDialog.test.tsx src/features/import/ImportHistoryPanel.tsx src/features/import/ImportHistoryPanel.test.tsx src/features/import/useImportWorkflow.ts
git commit -m "feat(import-ui): expose safe migration and history"
```

---

### Task 10: Compose the Final View, Tokens, Responsive Behavior, and Localization

**Files:**

- Rewrite: `src/features/import/ImportView.tsx`
- Rewrite: `src/features/import/ImportView.test.tsx`
- Create: `src/features/import/ImportV2Header.tsx`
- Create: `src/features/import/ImportCommitBar.tsx`
- Create: `src/features/import/ImportV2Dialogs.tsx`
- Modify: `src/styles.css`
- Modify: `src/test/ui-css-contracts.test.ts`
- Modify: `src/i18n/locales/en.json`
- Modify: `src/i18n/locales/zh-CN.json`
- Modify: `src/components/app/RightContextPanel.tsx`
- Modify: `src/test/app-shell-architecture.test.ts`

**Interfaces:**

- Consumes the stable `ImportWorkflow`, store selectors, method panes, queue, right inspector, and dialogs from Tasks 1–9.
- Produces the final lazy Import V2 view without changing shell ownership.

- [ ] **Step 1: Write failing full-view behavior tests**

Render bootstrap loading, empty draft, populated mixed-state queue, blocked migration, partial success, compact width, collapsed right panel, Chinese copy, English copy, keyboard-only navigation, and lazy-view error-boundary architecture.

- [ ] **Step 2: Run and verify RED**

Run:

```text
npm run test -- src/features/import/ImportView.test.tsx src/test/ui-css-contracts.test.ts src/test/app-shell-architecture.test.ts
```

- [ ] **Step 3: Compose the view with no business logic**

```tsx
export function ImportView({ workflow }: { workflow: ImportWorkflow }) {
  return (
    <div className="import-v2-layout">
      <ImportV2Header session={workflow.session} />
      <div className="import-v2-scroll app-pane-scrollbar">
        <ImportMigrationNotice />
        <ImportSourceMethods onAddPaths={workflow.addPaths} onAddUrl={workflow.addUrl} />
        <ImportQueue />
      </div>
      <ImportCommitBar />
      <ImportV2Dialogs />
    </div>
  );
}
```

The header subtitle states the outcome: sources become readable Markdown. Its statistic uses backend/session counts. Keep the commit bar sticky inside the central pane; it shows selected-ready count, unresolved-action count, checkpoint/merge facts, and one explicit Confirm button.

- [ ] **Step 4: Implement reference-aligned CSS contracts**

Central content uses a maximum readable width only for method panes; the queue fills available width. At widths below the existing 1180px shell breakpoint, the right inspector follows the shell drawer behavior. Below 820px, method panes stack and the commit bar wraps without covering rows. No horizontal page scroll at 1024px. Queue name truncation preserves the full value in `title`.

- [ ] **Step 5: Add complete bilingual copy and accessibility checks**

Use semantic headings, `aria-live="polite"` for queue count/progress summaries, `role="alert"` only for actionable failures, labeled icon buttons, visible focus rings, non-color status icons/text, dialog focus return, and reduced-motion handling. Verify all Chinese and English labels fit at 100%, 125%, and 150% OS text scaling.

- [ ] **Step 6: Remove obsolete V1-only styles and props**

Delete unused `.import-grid`, legacy preview table/action CSS, old `ImportView` prop fan-out, and dead translation keys only after source scans and tests prove no consumer. Do not remove shared `.table`, `.badge`, or button primitives.

- [ ] **Step 7: Run focused tests and commit**

```bash
git add src/features/import/ImportView.tsx src/features/import/ImportView.test.tsx src/features/import/ImportV2Header.tsx src/features/import/ImportCommitBar.tsx src/features/import/ImportV2Dialogs.tsx src/styles.css src/test/ui-css-contracts.test.ts src/i18n/locales/en.json src/i18n/locales/zh-CN.json src/components/app/RightContextPanel.tsx src/test/app-shell-architecture.test.ts
git commit -m "feat(import-ui): deliver import v2 workspace"
```

---

### Task 11: Run End-to-End UX, Security, Performance, and Release Gates

**Files:**

- Create: `src/features/import/ImportV2.integration.test.tsx`
- Create: `docs/qa/import-v2-frontend.md`
- Modify: `SPEC/progress.txt`
- Modify: `SPEC/gotchas.txt` only when a subtle or recurring issue is discovered

**Interfaces:**

- Produces release evidence only; no new production interface.

- [ ] **Step 1: Run the complete user-journey matrix**

Test empty project; recovered draft; one file; multiple files; folder with skipped entries; repeated drops; generic URL; WeChat/Zhihu/Bilibili; unavailable phase-two platform; one active, one ready, one failed; partial commit; retry; cancel; restart; login; captcha; private target; capability install; Agent manual assistance; automatic local hard-failure policy; BYOK approval; candidate Diff; needs-merge; migration dry run/resume; legacy history; and project switch during every awaited action.

- [ ] **Step 2: Run accessibility and keyboard gates**

Tab through method panes, queue filters, rows, right inspector, sticky commit bar, and every dialog. Verify focus trap/return, Escape, accessible names, live regions, contrast across all existing theme presets, reduced motion, and no color-only state.

- [ ] **Step 3: Run security and secret gates**

Seed cookies, Authorization, signed URLs, provider keys, browser profile paths, local usernames, malicious Markdown links/HTML, and prompt-injection content. Assert none enters Zustand snapshots, DOM outside an explicitly redacted preview, toast, task summary, local storage, translation interpolation, console, or exported test snapshots.

- [ ] **Step 4: Run performance and bundle gates**

Render 2,000 items with windowed or bounded queue behavior and assert interaction remains responsive on the documented reference machine. Coalesce task-driven session refreshes. Confirm Playwright, Markdown renderer, Diff UI, and migration UI remain behind the lazy Import chunk and do not enter the initial shell bundle. Scrolling one updating row must not rerender every row.

- [ ] **Step 5: Run exact focused checks and the unified gate**

```text
npm run test -- src/features/import
npm run test -- src/stores/importStore.test.ts src/services/importV2Api.test.ts
npm run test -- src/test/ui-css-contracts.test.ts src/test/app-shell-architecture.test.ts
npm run check
```

If `npm run check` fails, fix the first in-scope failure and rerun from the beginning. Do not delete or modify unrelated worktrees/build directories to make lint pass.

- [ ] **Step 6: Perform two independent reviews**

- Review A with shared context: compare the implementation with this plan, `import-v2-reference.html`, shell design tokens, all five backend packages, and approved user flows.
- Review B with fresh context: attack stale-project commits, missing states/actions, secret leaks, accessibility, dialog focus, task cancellation, recovery, partial success, V1 calls, bundle leakage, and empty assertions.

Fix every valid finding, rerun focused checks, then rerun `npm run check` from the beginning.

- [ ] **Step 7: Record final evidence and commit**

`docs/qa/import-v2-frontend.md` records screenshots for Chinese/English and light/dark themes, tested window sizes, keyboard path, user-journey matrix, bundle evidence, exact check output, two review results, and remaining risks. Insert the newest `SPEC/progress.txt` record without changing history.

```bash
git add src/features/import/ImportV2.integration.test.tsx docs/qa/import-v2-frontend.md SPEC/progress.txt SPEC/gotchas.txt
git commit -m "test(import-ui): certify import v2 frontend"
```

---

## Dependency Order

- Task 1 requires the final accepted contracts from all five backend plans.
- Task 2 depends on Task 1 and becomes the state boundary for every later view task.
- Tasks 3 and 4 may proceed in parallel after Task 2; they share only typed callbacks.
- Task 5 integrates Tasks 1–4 and must stabilize before task-driven dialogs are implemented.
- Task 6 depends on Tasks 2 and 5.
- Tasks 7, 8, and 9 depend on Task 5 and may proceed in parallel in separate files.
- Task 10 composes Tasks 3–9 and owns final visual/i18n cleanup.
- Task 11 is the final release gate and blocks delivery.

## Explicitly Rejected Approaches

- Reusing the reference HTML's mock timers, local queue array, DOM event delegation, inline SVG sprite, or browser `webkitGetAsEntry` as application implementation.
- Fetching a URL or running Readability directly in React.
- Letting the frontend read `preview.relativePath`, staging directories, source files, cookies, browser profiles, or keyring values.
- Keeping both the V1 preview workflow and V2 session workflow behind a user toggle.
- Treating every item as one blocking batch spinner or clearing successful siblings when one item fails.
- Auto-approving login, captcha, private targets, capabilities, BYOK, Agent Diff, merge, migration, or commit.
- Showing unsupported Xiaohongshu/X routes as available merely because the design reference contains their chips.
- Putting task facts only in `importStore`, or letting old-project results open the current project's drawer/dialog/toast.
- Creating a second Markdown renderer, Diff engine, toast system, dialog primitive, task drawer, or path formatter.
- Large decorative cards, hero copy, gradients, nested card stacks, or hardcoded colors outside tokens.

## Definition of Done

- The Import view matches the reference hierarchy while remaining native to the existing Codex-like shell and right inspector.
- Users can repeatedly add files/folders and exact URLs to one recoverable session without understanding parser routes.
- Every Import V2 status has a clear label, progress mode, safe action set, and bilingual accessible presentation.
- Partial success, cancellation, restart recovery, login/capability waits, Agent assistance, BYOK approval, Diff/merge, migration, and legacy history are usable without bypassing backend safety.
- React contains no filesystem, URL fetching, Agent/process, secret, capability installation, migration, or commit logic.
- No legacy import mutation command remains in the active frontend path and no V1/V2 toggle exists.
- Visual, responsive, localization, accessibility, security, project-switch, performance, bundle, focused, unified, and two-review gates pass.
- No merge, push, real-project migration, activation, capability download, or Agent installation occurs implicitly during implementation.
