# Current Architecture Documentation Alignment Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make every living project specification accurately describe the current AppShell workflow-controller architecture and Rust service use-case module architecture while preserving product decisions and historical evidence.

**Architecture:** Treat current `master` code as the implementation source of truth and update living documents by responsibility: product/flow, cross-layer architecture, backend structure, frontend boundaries, and roadmap evidence. Preserve the stable IPC/DTO/persistence contracts and add completion notices to the two historical implementation plans instead of rewriting their original contents.

**Tech Stack:** Markdown documentation, PowerShell validation, Git, existing React 19/Tauri v2/Rust implementation evidence.

## Global Constraints

- `SPEC/DESIGN.md` must remain byte-for-byte unchanged.
- Do not modify runtime source, tests, package files, DTOs, IPC commands, persistence formats, or product behavior.
- Do not rewrite historical audit evidence or old progress/gotcha entries.
- Product content remains Markdown + JSON + local files; no database language may be introduced.
- Preserve Git checkpoint, PendingAction, project-boundary, task, secret-storage, local-search, Agent/BYOK, Unicode/CJK, and `raw/sources/` safety rules.
- Living documentation must distinguish implemented facts from future recommendations.
- `chat_convenience_service.rs` and `wiki_index.rs` remain independent services/modules.
- User-owned `.superpowers/` and `UI-Frontend-design/import-v2-reference.html` remain untouched.

---

### Task 1: Align the product specification and application flow

**Files:**
- Modify: `SPEC/SPEC.md`
- Modify: `SPEC/APP_flow.md`

**Interfaces:**
- Consumes: current frontend workflow contracts and backend facade contracts from the approved design.
- Produces: the canonical current-state overview and cross-view flow rules used by all lower-level architecture documents.

- [ ] **Step 1: Update the current implementation record in `SPEC/SPEC.md`**

Replace the partial 2026-07-10 implementation section with a dated current-state record that explicitly covers:

```text
Frontend: AppShell -> WorkspaceController -> WorkspaceRouter -> lazy feature views
Global controllers: ProjectConfirmationController, TaskLogDrawer, Toaster
Workflows: useAiCapabilities, useTaskLauncher, useImportWorkflow,
           useProviderWorkflow, useAgentWorkflow
Backend: commands -> AppState -> stable service facades -> focused use-case modules
Facades: ImportService, SearchService, LintService, ChatService
Independent boundaries: ChatConvenienceService, WikiIndex
```

Retain the existing product, storage, visual, Chat, Search, Graph, Export, task, and audit constraints; update only implementation facts.

- [ ] **Step 2: Add implemented orchestration rules to `SPEC/APP_flow.md`**

Document the exact current flows:

```text
Import preview -> confirm_import_preview -> wikiStore.scan -> optional start_wiki_compile
Task launch -> backend task -> taskStore upsert -> current-project drawer open
PendingAction -> ProjectConfirmationController -> backend revalidation/checkpoint -> result task/update
Project switch -> project key/epoch invalidation -> stale UI commits and toasts suppressed
```

State that tasks from other projects remain persisted/visible even when stale UI commits are suppressed.

- [ ] **Step 3: Validate Task 1 terminology**

Run:

```powershell
Select-String -Path SPEC/SPEC.md,SPEC/APP_flow.md -Pattern 'AppShell|WorkspaceController|WorkspaceRouter|ProjectConfirmationController|ImportService|SearchService|LintService|ChatService'
git diff --check -- SPEC/SPEC.md SPEC/APP_flow.md
```

Expected: all named boundaries are present; `git diff --check` exits 0.

- [ ] **Step 4: Commit Task 1**

```powershell
git add SPEC/SPEC.md SPEC/APP_flow.md
git commit -m "docs: align application flow with workflow controllers"
```

### Task 2: Replace the obsolete cross-layer and backend architecture descriptions

**Files:**
- Modify: `SPEC/TECH_STACK.md`
- Modify: `SPEC/BACKEND_STRUCTURE.md`

**Interfaces:**
- Consumes: Task 1 canonical flow terminology, `src-tauri/src/app_state.rs`, `src-tauri/src/lib.rs`, `src-tauri/src/services/mod.rs`, and the four facade module directories.
- Produces: authoritative implemented architecture and backend module reference.

- [ ] **Step 1: Convert `TECH_STACK.md` from recommendation to implemented architecture**

Update the architecture diagram to:

```text
React shell/layout
  -> WorkspaceController + feature workflows
  -> typed Tauri invoke
  -> thin command modules
  -> AppState + ProjectRegistry
  -> stable service facades / TaskService / ConfirmationRegistry
  -> local files / Git / Agent / LLM / OS credential store
```

Remove the obsolete statement that the repository is not a complete application. Add lazy-view, Suspense/ErrorBoundary, project-key guard, taskStore, and secret-flow boundaries.

- [ ] **Step 2: Replace the backend directory tree with the actual tree**

In `BACKEND_STRUCTURE.md`, show the current command, model, task, and service layout, including these submodules exactly:

```text
services/import_service/{artifacts,classification,confirmation,preview,promotion,source_actions,source_catalog,test_support}.rs
services/search_service/{catalog,excerpts,pages,query,test_support}.rs
services/lint_service/{deep,fixes,ignores,reports,rules,test_support}.rs
services/chat_service/{citations,retrieval,saved_answers,sessions,test_support}.rs
```

- [ ] **Step 3: Document facade and dependency rules**

State that:

```text
- commands and AppState depend on facade types, never private use-case modules;
- services/mod.rs is the crate-facing re-export boundary;
- multiple focused impl Service blocks implement use cases;
- submodules use the narrowest visibility;
- facade tests protect construction and selected public contracts;
- chat_convenience_service and wiki_index stay separate;
- command registration remains explicit in lib.rs;
- DTO and persistence compatibility are not changed by physical file moves.
```

Update service-specific sections to point to their current submodules and remove references to future skeleton creation.

- [ ] **Step 4: Validate backend path accuracy**

Run:

```powershell
$living = 'SPEC/TECH_STACK.md','SPEC/BACKEND_STRUCTURE.md'
Select-String -Path $living -Pattern '尚未初始化完整|还不是完整应用|services/import_service\.rs|services/search_service\.rs|services/lint_service\.rs|services/chat_service\.rs'
git diff --check -- $living
```

Expected: the stale-pattern search returns no matches; `git diff --check` exits 0.

- [ ] **Step 5: Commit Task 2**

```powershell
git add SPEC/TECH_STACK.md SPEC/BACKEND_STRUCTURE.md
git commit -m "docs: document current backend module architecture"
```

### Task 3: Align frontend architecture and implementation guardrails

**Files:**
- Modify: `SPEC/FRONTEND_GUIDELINES.md`
- Verify unchanged: `SPEC/DESIGN.md`

**Interfaces:**
- Consumes: `AppShell`, `WorkspaceController`, `WorkspaceRouter`, the five workflow hooks, existing UI authority rules in `AGENTS.md`, and Task 1 terminology.
- Produces: the canonical frontend ownership, async safety, task, secret, and bundle rules.

- [ ] **Step 1: Add the implemented shell ownership map**

Document:

```text
AppShell: frame, pane sizing, responsive behavior, global keyboard shortcuts, global controller mounting
WorkspaceController: project-scoped workflow composition and modal wiring
WorkspaceRouter: active-view dispatch and lazy boundary
ProjectConfirmationController: PendingAction and compile conflict orchestration
Feature workflows: domain-specific state/effects only
```

Explicitly prohibit workflow logic from returning to AppShell or being consolidated into one giant controller/hook.

- [ ] **Step 2: Add asynchronous project-scope rules**

Require `projectId + rootPath` (or an equivalent stable key) at every async commit point, epoch guards for supersedable operations, and stale suppression for UI state, navigation, drawer opening, and toasts.

- [ ] **Step 3: Add secret, task, and bundle rules**

Document that raw provider secrets pass directly from form callbacks to Tauri IPC and never enter Zustand, logs, or toasts; all long-task results enter `taskStore`; and lazy views remain wrapped by colocated `Suspense` and `ViewErrorBoundary` without heavy controller imports pulling async chunks into the initial bundle.

- [ ] **Step 4: Reconcile frontend guideline authority without editing `DESIGN.md`**

State in `FRONTEND_GUIDELINES.md` that `UI-Frontend-design/` is authoritative for exact DOM, tokens, font sizes, and component heights, while `SPEC/DESIGN.md` remains a high-level visual reference. Correct conflicting shell dimensions and typography guidance in the living frontend guideline only.

- [ ] **Step 5: Validate scope and commit**

Run:

```powershell
git diff --name-only -- SPEC/DESIGN.md
Select-String -Path SPEC/FRONTEND_GUIDELINES.md -Pattern 'WorkspaceController|WorkspaceRouter|projectId|rootPath|taskStore|ViewErrorBoundary|UI-Frontend-design'
git diff --check -- SPEC/FRONTEND_GUIDELINES.md
```

Expected: `SPEC/DESIGN.md` produces no output; all architecture terms are present; diff check exits 0.

Commit:

```powershell
git add SPEC/FRONTEND_GUIDELINES.md
git commit -m "docs: define frontend workflow and bundle boundaries"
```

### Task 4: Align living roadmaps and mark implementation plans complete

**Files:**
- Modify: `SPEC/roadmap/README.md`
- Modify: `SPEC/roadmap/shell-dashboard.md`
- Modify: `SPEC/roadmap/agent.md`
- Modify: `SPEC/roadmap/import.md`
- Modify: `SPEC/roadmap/chat.md`
- Modify: `SPEC/roadmap/lint.md`
- Modify: `SPEC/roadmap/wiki.md`
- Modify: `SPEC/roadmap/cross-cutting.md`
- Modify: `docs/superpowers/plans/2026-07-10-app-shell-workflow-controllers.md`
- Modify: `docs/superpowers/plans/2026-07-10-service-use-case-modularization.md`

**Interfaces:**
- Consumes: Tasks 1-3 living architecture terminology and current code paths.
- Produces: current roadmap evidence without rewriting historical audits or plan bodies.

- [ ] **Step 1: Add roadmap architecture migration guidance**

Add a dated note to `SPEC/roadmap/README.md` mapping old monolith references to current module directories and AppShell workflow ownership to controllers/hooks. State that dated audits remain historical evidence.

- [ ] **Step 2: Update changed frontend roadmap facts**

In `shell-dashboard.md`, `agent.md`, and `import.md`, correct only items changed by this refactor, including:

```text
- AppShell no longer owns feature workflow commands.
- WorkspaceRouter owns view-level lazy/Suspense/ErrorBoundary dispatch.
- the RunAgentDialog and Agent skill routing now exist.
- import workflow ownership and confirm/scan/compile order moved to useImportWorkflow.
- responsive right-panel collapse and persisted pane behavior reflect current code.
```

Leave unrelated visual/product gaps unchanged.

- [ ] **Step 3: Update backend path evidence in affected roadmaps**

Replace current-state references to deleted monolith files in `chat.md`, `lint.md`, `wiki.md`, and `cross-cutting.md` with the relevant directory module/symbol. Do not alter historical audit documents or old progress entries.

- [ ] **Step 4: Add completion notices to the two implementation plans**

At the top of each 2026-07-10 plan, add a short blockquote containing:

```text
Status: completed on 2026-07-10 and integrated into master.
Current architecture documentation: links to the relevant living SPEC files.
The task body remains the historical implementation plan.
```

- [ ] **Step 5: Validate roadmap and history boundaries**

Run:

```powershell
$roadmaps = Get-ChildItem SPEC/roadmap -File
$roadmaps | Select-String -Pattern 'src-tauri/src/services/(import_service|search_service|lint_service|chat_service)\.rs'
git diff --name-only -- docs/audits
git diff --check -- SPEC/roadmap docs/superpowers/plans/2026-07-10-app-shell-workflow-controllers.md docs/superpowers/plans/2026-07-10-service-use-case-modularization.md
```

Expected: no deleted-monolith path matches in living roadmaps; no audit files changed; diff check exits 0.

- [ ] **Step 6: Commit Task 4**

```powershell
git add SPEC/roadmap docs/superpowers/plans/2026-07-10-app-shell-workflow-controllers.md docs/superpowers/plans/2026-07-10-service-use-case-modularization.md
git commit -m "docs: refresh architecture roadmap evidence"
```

### Task 5: Record the milestone, review, and verify

**Files:**
- Modify: `SPEC/progress.txt`
- Review: all files changed since the design commit

**Interfaces:**
- Consumes: completed Tasks 1-4.
- Produces: reviewed, verified documentation and an auditable milestone.

- [ ] **Step 1: Add the progress milestone**

Prepend one record using the required format:

```text
[2026-07-11] Architecture documentation alignment — Aligned living SPEC, flow, technical, backend, frontend, and roadmap documentation with the AppShell workflow controllers and Rust service use-case modules while preserving product contracts and historical evidence — Key decision: current code is the implementation source of truth; DESIGN.md and historical audits remain unchanged.
```

- [ ] **Step 2: Perform the main-agent consistency review**

Check every changed document against the actual frontend imports, workflow signatures, AppState fields, service exports, command registration, and facade contract tests. Reject claims that are recommendations disguised as implemented facts.

- [ ] **Step 3: Run the two required independent reviews**

Reviewer A receives full context and checks design intent, cross-document consistency, safety boundaries, and roadmap status. Reviewer B receives only repository path, current HEAD, approved design path, implementation plan path, and changed-file list, and checks stale paths, API/DTO drift claims, missing documents, and historical-record contamination.

- [ ] **Step 4: Validate and fix findings**

Use `superpowers:receiving-code-review` before accepting or rejecting findings. Apply only evidence-backed corrections, then rerun the relevant documentation scans.

- [ ] **Step 5: Run full verification**

Run:

```powershell
npm run check
git diff --check 10ed2a8...HEAD
git status --porcelain
```

Expected: 937 project tests remain passing unless the test suite has legitimately grown; lint, build, console scan, Tauri GUI check, Rust tests, and the overall command exit 0. Tracked worktree state is clean after the final commit; the two pre-existing user-owned untracked paths may remain.

- [ ] **Step 6: Commit the milestone and any review corrections**

```powershell
git add SPEC/progress.txt SPEC/SPEC.md SPEC/APP_flow.md SPEC/TECH_STACK.md SPEC/BACKEND_STRUCTURE.md SPEC/FRONTEND_GUIDELINES.md SPEC/roadmap docs/superpowers/plans/2026-07-10-app-shell-workflow-controllers.md docs/superpowers/plans/2026-07-10-service-use-case-modularization.md
git commit -m "docs: finish current architecture alignment"
```

- [ ] **Step 7: Re-run final status checks**

```powershell
git diff --check HEAD^..HEAD
git status --porcelain
git log --oneline -6
```

Expected: no whitespace errors; only the two known user-owned untracked paths remain; recent commits show the scoped documentation series.
