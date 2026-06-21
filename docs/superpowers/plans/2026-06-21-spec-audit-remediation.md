# Spec Audit Remediation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Every production change starts with a failing regression test.

**Goal:** Close every automatically verifiable Task 1-14 gap found by the 2026-06-21 specification audit without changing locked product decisions or adding binary parsers.

**Architecture:** Preserve the React -> typed Tauri IPC -> Rust service boundary. Bind IPC requests to backend-registered project contexts, route long work through TaskService, and keep Markdown/JSON/local files as the only project persistence.

**Tech Stack:** React 19, TypeScript, Vitest, Zustand, Tauri v2, Rust, Cargo tests.

---

### Task 1: Bind IPC to backend-registered projects

**Files:** `src-tauri/src/app_state.rs`, `src-tauri/src/commands/*.rs`, new focused registry module/tests.

- [ ] Add a failing test that registers project A, then supplies A's id with project B's root and expects `PROJECT_CONTEXT_MISMATCH` with no write to B.
- [ ] Implement a thread-safe project-context registry owned by `AppState`; create/open/confirmed-initialize register canonical roots.
- [ ] Replace command-local `ProjectContext::new(request root)` with registry resolution. Keep create/open as the only commands accepting a new absolute root.
- [ ] Run Rust tests, fmt, and clippy; update `SPEC/progress.txt`.

### Task 2: Restore a real project start/switch flow and global search

**Files:** `src/app/App.tsx`, `src/stores/projectStore.ts`, `src/components/app/TopBar.tsx`, project-start UI/tests, search result UI/tests.

- [ ] Add failing tests proving an empty install shows create/open/recent choices and never renders the hardcoded demo project.
- [ ] Add failing tests proving Enter in global search calls `search_wiki` and selecting a result opens the Wiki page.
- [ ] Remove fake current-project defaults, load recent projects on startup, and make project switching reachable.
- [ ] Wire keyword search only; never call Agent/LLM.
- [ ] Run frontend checks and update progress.

### Task 3: Make file/folder/URL/clipboard import reachable and honest

**Files:** import commands/services/models, `ImportView.tsx`, `AppShell.tsx`, Readability adapter/tests.

- [ ] Add failing tests for native File objects without a `.path`, recursive folders with CJK files, URL input, and clipboard Markdown.
- [ ] Use a Tauri-native picker/drop path source or show a visible error; never silently clear preview.
- [ ] Recursively expand folders and preserve classification/conflict rules.
- [ ] Archive URL body/metadata and clipboard Markdown through backend writes; keep binary formats as explicit unsupported extraction.
- [ ] Run all checks and update progress/gotchas.

### Task 4: Enforce backend-owned confirmations

**Files:** confirmation models/registry, lint/chat commands/services/stores/dialog tests.

- [ ] Add failing tests showing direct `confirmHighRisk=true` and `allowOverwrite=true` without a stored action are rejected.
- [ ] Resume lint/chat overwrite only by backend-stored action id with expiry/state revalidation and Git checkpoint.
- [ ] Keep compile conflict confirmation backend-owned and expose keep-current/use-generated/manual-merge choices where the contract requires them.
- [ ] Run all checks and update progress.

### Task 5: Make task recovery truthful and long tasks cancellable

**Files:** `task_service.rs`, task persistence model, import/graph commands and frontend stores/tests.

- [ ] Add failing tests for queued/running task persistence with logs, project switching cancellation, and recovery as interrupted/failed.
- [ ] Persist lifecycle/progress/log changes atomically; cancel old-project workers before detaching them.
- [ ] Return BackendTask ids immediately for import parsing and first graph build; report progress and honor cancellation before final writes.
- [ ] Fix `list_tasks` callers to send the required request DTO.
- [ ] Run all checks and update progress/gotchas.

### Task 6: Close secret, rendering, error, i18n, and accessibility gaps

**Files:** secret/settings DTOs and UI, Markdown reader/style entry, import/dialog i18n, dialog focus tests.

- [ ] Add failing tests that no secret suffix reaches IPC/UI and only configured state is displayed.
- [ ] Import KaTeX/highlight styles and apply the reading font/styles.
- [ ] Replace hardcoded import/dialog strings with i18n keys and surface rejected IPC calls visibly.
- [ ] Add dialog initial focus, focus trap, Escape close, and focus restoration; add accessible names to icon-only controls.
- [ ] Run all checks and update progress.

### Task 7: Existing-Wiki Git readiness, Agent profiles, and update boundary

**Files:** project open flow, Git confirmation, Agent invocation tests, update settings/adapter.

- [ ] Add a failing test for opening a non-Git Wiki and attempting compile; require an explicit Git-initialization path before writes.
- [ ] Align chat/lint/export Agent invocation safety profiles with compile where the CLI supports it.
- [ ] Implement update check through a replaceable adapter only if endpoint/public-key configuration exists; otherwise keep controls explicitly unavailable and document the external configuration blocker instead of claiming Task 13 complete.
- [ ] Run all checks and update progress.

### Task 8: Replace hollow acceptance evidence

**Files:** `src-tauri/tests/mvp_flow.rs`, command-contract tests, frontend flow tests, `docs/qa/mvp-acceptance.md`.

- [ ] Make absent sample data fail visibly rather than return success; add a 200-page synthetic performance/cancellation fixture.
- [ ] Exercise real command/store boundaries for Agent and BYOK fake compile/chat/lint/export instead of manually constructing final objects.
- [ ] Correct QA counts, CSV status, verified/unverified statements, and current blockers.
- [ ] Run the complete user-specified checklist, dual reviews, and a final spec matrix pass.
