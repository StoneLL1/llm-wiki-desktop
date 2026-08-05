# LLM Wiki Desktop MVP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
> **Status note (2026-07-30):** This is the original MVP bootstrap plan, so its baseline and create/modify labels are historical rather than a current repository inventory. For Workflows, follow [`docs/superpowers/specs/2026-07-30-workflows-panel-redesign.md`](docs/superpowers/specs/2026-07-30-workflows-panel-redesign.md) and [`SPEC/roadmap/agent.md`](SPEC/roadmap/agent.md). For Task 2, Task 3, Task 14 and the project-lifecycle acceptance matrix, follow [`docs/superpowers/specs/2026-07-30-first-run-project-open-workbench-design.md`](docs/superpowers/specs/2026-07-30-first-run-project-open-workbench-design.md). For Task 5 and every Import/Source acceptance item, follow [`docs/superpowers/specs/2026-07-24-import-source-media-flow-design.md`](docs/superpowers/specs/2026-07-24-import-source-media-flow-design.md). The updated text below reflects those overrides; remaining historical baseline wording must not restore a standalone launch page, ordinary-folder in-place initialization, media-type-only evidence layout, partial evidence/Source commits, or compile-after-import.

**Goal:** Build the MVP local-first Tauri desktop app that turns personal sources into a Markdown wiki with import, graph, chat, lint, Workflows, Agent/BYOK execution routes, export, Git safety, background tasks, and settings.

**Architecture:** Keep React as a compact Codex-like workbench and keep all filesystem, Git, Agent process, LLM, task, and secret logic behind typed Tauri IPC commands. The project folder remains the source of truth, with Markdown and JSON persisted under `purpose.md`, `schema.md`, `raw/`, `wiki/`, `.app/`, `exports/`, and `skills/`.

That root layout describes a newly created native knowledge base. Compatible external vaults retain their structure; after explicit enablement, app-owned compatibility guidance lives under `.app/compat/`, never in root files that may belong to the user.

**Tech Stack:** React 19, TypeScript, Vite, Tailwind CSS v4, shadcn-style local components, Lucide React, Zustand, react-i18next, Tauri v2, Rust services, Markdown/JSON/local files, Git checkpoints, sigma.js, graphology, ForceAtlas2, Louvain, remark/rehype Markdown rendering, Milkdown editor.

---

## 0. Execution Contract For Agents

Every implementation agent must follow these rules before editing code:

- Read `AGENTS.md`, `CLAUDE.md`, `SPEC/PRD.md`, `SPEC/SPEC.md`, `SPEC/APP_flow.md`, `SPEC/TECH_STACK.md`, `SPEC/BACKEND_STRUCTURE.md`, `SPEC/FRONTEND_GUIDELINES.md`, `SPEC/DESIGN.md`, and the feature-specific confirmed design documents linked from those files.
- Treat `wiki/wiki/` as validation data only. Do not move it into app source code.
- Keep user project content as Markdown, JSON, and local files. Do not introduce a database for wiki content.
- Keep `raw/sources/` immutable by default. Replacement or deletion requires an explicit confirmation flow and a Git checkpoint.
- Store API keys only through OS credential storage. Do not write secrets to project files, logs, exports, or tests.
- Route all filesystem, Git, Agent, LLM, task, and secret behavior through Tauri IPC and Rust services. React must not own those concerns.
- For every meaningful milestone, add a new top entry to root `progress.txt` with `[YYYY-MM-DD] Module/Task - Summary - Key decision or open issue`.
- Add one root `gotchas.txt` entry only when an issue recurs, is subtle, or is easy to repeat.
- After each feature or meaningful fix, run tests and lint, perform the review workflow from `AGENTS.md`, fix valid findings, then rerun checks from the beginning.
- Required final checks for every completed implementation task: `npm run test`, `npm run lint`, no unintended `console.log`, import paths resolve through `npm run build` or equivalent TypeScript build verification.

Acceptance for this contract:

- A new agent can start from this file and know which docs to read, which data is source-of-truth, which operations need confirmation, and which checks are mandatory.
- No implementation step in this plan asks the agent to bypass Git checkpoints, secret storage, or typed IPC.

## 1. Current Baseline

The repository already contains:

- React/Vite frontend scaffold under `src/`.
- A Codex-like shell skeleton in `src/components/app/AppShell.tsx`.
- Zustand navigation state in `src/stores/navigationStore.ts`.
- i18n placeholders under `src/i18n/locales/`.
- Tauri/Rust skeleton under `src-tauri/`.
- Stub service modules for project, file, import, Git, Agent, LLM, search, graph, export, settings, and task service.
- A sample wiki under `wiki/wiki/` with 345 Markdown files for validation.
- `npm run test` and `npm run lint` scripts.

Baseline gaps to close:

- `ExtractionService`, `LintService`, `SecretService`, full task models, confirmation models, and most command modules are missing.
- Existing services are stubs and do not yet implement project lifecycle, file safety, import, Git checkpoints, Agent, BYOK, graph, lint, export, settings, or secrets.
- Frontend feature folders currently contain README placeholders rather than working views.

2026-07-30 migration reality supersedes the original scaffold inventory above: the app is broadly implemented, but no-project rendering still branches to standalone `ProjectStartView`; project opening still uses binary detection, can initialize Git while opening an external folder, and retains an ordinary-folder move / initialization continuation. There is no complete global trust identity, access policy, typed repair plan or restricted/read-only project-open pipeline yet.

Acceptance for this baseline:

- Execution agents must preserve the existing scaffold where useful instead of replacing the app wholesale.
- Each task below should leave the app runnable and testable.

## 2. File Ownership Map

Frontend application shell:

- Modify: `src/app/App.tsx`
- Modify: `src/components/app/AppShell.tsx`
- Create: `src/components/app/TopBar.tsx`
- Create: `src/components/app/LeftSidebar.tsx`
- Create: `src/components/app/RightContextPanel.tsx`
- Create: `src/components/app/BottomStatusBar.tsx`
- Create: `src/components/app/TaskActivityButton.tsx`
- Modify: `src/styles.css`
- Modify: `src/app/App.test.tsx`

Frontend shared primitives and utilities:

- Modify: `src/components/ui/button.tsx`
- Create: `src/components/ui/input.tsx`
- Create: `src/components/ui/dialog.tsx`
- Create: `src/components/ui/drawer.tsx`
- Create: `src/components/ui/tooltip.tsx`
- Create: `src/components/ui/badge.tsx`
- Create: `src/lib/tauri.ts`
- Create: `src/lib/format.ts`
- Create: `src/lib/paths.ts`

Frontend state:

- Modify: `src/stores/navigationStore.ts`
- Create: `src/stores/projectStore.ts`
- Create: `src/stores/taskStore.ts`
- Create: `src/stores/agentStore.ts`
- Create: `src/stores/settingsStore.ts`
- Create: `src/stores/chatStore.ts`
- Create: `src/stores/graphStore.ts`

Frontend feature views:

- Create: `src/features/dashboard/DashboardView.tsx`
- Create: `src/features/wiki/WikiView.tsx`
- Create: `src/features/wiki/WikiTree.tsx`
- Create: `src/features/wiki/MarkdownReader.tsx`
- Create: `src/features/wiki/WikiEditor.tsx`
- Create: `src/features/import/ImportView.tsx`
- Create: `src/features/graph/GraphView.tsx`
- Create: `src/features/chat/ChatView.tsx`
- Legacy baseline: `src/features/agent/AgentView.tsx`; migrate the user-facing surface to Workflows per Task 8 and `SPEC/roadmap/agent.md`
- Create: `src/features/lint/LintView.tsx`
- Create: `src/features/exports/ExportsView.tsx`
- Create: `src/features/settings/SettingsView.tsx`

Frontend types:

- Modify: `src/types/project.ts`
- Create: `src/types/backend.ts`
- Create: `src/types/import.ts`
- Create: `src/types/wiki.ts`
- Create: `src/types/task.ts`
- Create: `src/types/agent.ts`
- Create: `src/types/llm.ts`
- Create: `src/types/graph.ts`
- Create: `src/types/lint.ts`
- Create: `src/types/export.ts`
- Create: `src/types/settings.ts`

Rust backend core:

- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/app_state.rs`
- Modify: `src-tauri/src/errors/mod.rs`
- Create: `src-tauri/src/errors/backend_error.rs`
- Create: `src-tauri/src/errors/error_codes.rs`
- Modify: `src-tauri/src/models/mod.rs`
- Create: `src-tauri/src/models/paths.rs`
- Create: `src-tauri/src/models/confirmation.rs`
- Create: `src-tauri/src/models/task.rs`
- Create: `src-tauri/src/models/import.rs`
- Create: `src-tauri/src/models/wiki.rs`
- Create: `src-tauri/src/models/git.rs`
- Create: `src-tauri/src/models/agent.rs`
- Create: `src-tauri/src/models/llm.rs`
- Create: `src-tauri/src/models/search.rs`
- Create: `src-tauri/src/models/graph.rs`
- Create: `src-tauri/src/models/lint.rs`
- Create: `src-tauri/src/models/export.rs`
- Create: `src-tauri/src/models/settings.rs`

Rust backend commands:

- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/commands/project_commands.rs`
- Create: `src-tauri/src/commands/file_commands.rs`
- Create: `src-tauri/src/commands/import_commands.rs`
- Create: `src-tauri/src/commands/wiki_commands.rs`
- Create: `src-tauri/src/commands/git_commands.rs`
- Create: `src-tauri/src/commands/agent_commands.rs`
- Create: `src-tauri/src/commands/llm_commands.rs`
- Create: `src-tauri/src/commands/search_commands.rs`
- Create: `src-tauri/src/commands/graph_commands.rs`
- Create: `src-tauri/src/commands/lint_commands.rs`
- Create: `src-tauri/src/commands/export_commands.rs`
- Create: `src-tauri/src/commands/settings_commands.rs`
- Create: `src-tauri/src/commands/task_commands.rs`

Rust backend services:

- Modify: `src-tauri/src/services/mod.rs`
- Modify: `src-tauri/src/services/project_service.rs`
- Modify: `src-tauri/src/services/file_store.rs`
- Modify: `src-tauri/src/services/import_service.rs`
- Create: `src-tauri/src/services/extraction_service.rs`
- Modify: `src-tauri/src/services/git_service.rs`
- Modify: `src-tauri/src/services/agent_service.rs`
- Modify: `src-tauri/src/services/llm_service.rs`
- Modify: `src-tauri/src/services/search_service.rs`
- Modify: `src-tauri/src/services/graph_service.rs`
- Create: `src-tauri/src/services/lint_service.rs`
- Modify: `src-tauri/src/services/export_service.rs`
- Modify: `src-tauri/src/services/settings_service.rs`
- Create: `src-tauri/src/services/secret_service.rs`

Rust task and utilities:

- Modify: `src-tauri/src/tasks/mod.rs`
- Modify: `src-tauri/src/tasks/task_service.rs`
- Create: `src-tauri/src/tasks/task_model.rs`
- Create: `src-tauri/src/tasks/task_events.rs`
- Create: `src-tauri/src/tasks/cancellation.rs`
- Modify: `src-tauri/src/utils/path_utils.rs`
- Create: `src-tauri/src/utils/json_utils.rs`
- Create: `src-tauri/src/utils/markdown_utils.rs`
- Create: `src-tauri/src/utils/process_utils.rs`
- Create: `src-tauri/src/utils/time_utils.rs`

Project templates and skills:

- Create: `src-tauri/templates/projects/general/purpose.md`
- Create: `src-tauri/templates/projects/general/schema.md`
- Create: `src-tauri/templates/projects/research/purpose.md`
- Create: `src-tauri/templates/projects/research/schema.md`
- Create: `src-tauri/templates/projects/reading/purpose.md`
- Create: `src-tauri/templates/projects/reading/schema.md`
- Create: `src-tauri/templates/projects/personal-growth/purpose.md`
- Create: `src-tauri/templates/projects/personal-growth/schema.md`
- Create: `src-tauri/templates/projects/business/purpose.md`
- Create: `src-tauri/templates/projects/business/schema.md`
- Create: `src-tauri/templates/skills/wiki-ingest/SKILL.md`
- Create: `src-tauri/templates/skills/wiki-lint/SKILL.md`
- Create: `src-tauri/templates/skills/wiki-query/SKILL.md`
- Create: `src-tauri/templates/skills/html-beautiful-read/SKILL.md`
- Create: `src-tauri/templates/skills/html-beautiful-read/template.html`
- Create: `src-tauri/templates/skills/html-knowledge-card/SKILL.md`
- Create: `src-tauri/templates/skills/html-knowledge-card/template.html`
- Create: `src-tauri/templates/skills/html-project-report/SKILL.md`
- Create: `src-tauri/templates/skills/html-project-report/template.html`
- Create: `src-tauri/templates/skills/html-concept-map/SKILL.md`
- Create: `src-tauri/templates/skills/html-concept-map/template.html`

## 3. Task 1: Backend Contract Foundation

**Files:**

- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/app_state.rs`
- Modify: `src-tauri/src/errors/mod.rs`
- Create: `src-tauri/src/errors/backend_error.rs`
- Create: `src-tauri/src/errors/error_codes.rs`
- Modify: `src-tauri/src/models/mod.rs`
- Create: `src-tauri/src/models/confirmation.rs`
- Create: `src-tauri/src/models/task.rs`
- Create: `src-tauri/src/models/paths.rs`
- Modify: `src-tauri/src/utils/path_utils.rs`
- Create: `src-tauri/src/utils/time_utils.rs`
- Test: Rust unit tests inside each touched Rust module.

- [ ] Step 1: Add failing tests for path normalization and project-boundary rejection.

  Run: `cargo test --manifest-path src-tauri/Cargo.toml path_utils -- --nocapture`

  Expected before implementation: tests fail for path traversal, absolute path injection, CJK path preservation, and Windows separator normalization.

- [ ] Step 2: Implement `ProjectContext`, canonical project-relative path conversion, and `BackendError`.

  Acceptance: `ProjectContext` carries typed layout and access policy rather than assuming every compatible vault has native directories. `resolve_project_path` accepts safe paths like `wiki/concepts/agent.md`, preserves CJK names, converts backslashes to `/` for relative display paths, and rejects `../` and absolute injection. A root symlink/junction may open after canonicalization; contained internal links are loop-safe, while external targets are display-only and never indexed or written. Every mutation checks trusted/writable/checkpoint capabilities.

- [ ] Step 3: Add `PendingAction`, `RiskLevel`, `ActionPreview`, `BackendTask`, `TaskStatus`, `TaskProgress`, and `BackendEvent` models.

  Acceptance: all DTOs serialize with camelCase fields, stable string enum values, and no open-ended map for fixed fields.

- [ ] Step 4: Register all service skeletons in `AppState`, including `ExtractionService`, `LintService`, and `SecretService`.

  Acceptance: `AppState::default()` compiles and every service has one clear struct exported through `src-tauri/src/services/mod.rs`.

- [ ] Step 5: Run backend contract checks.

  Run: `cargo test --manifest-path src-tauri/Cargo.toml`

  Expected: all Rust tests pass.

- [ ] Step 6: Commit.

  Commit message: `chore: establish backend contracts`

## 4. Task 2: Frontend Shell, Routing State, And Design Tokens

**Files:**

- Modify: `src/components/app/AppShell.tsx`
- Create: `src/components/app/TopBar.tsx`
- Create: `src/components/app/LeftSidebar.tsx`
- Create: `src/components/app/RightContextPanel.tsx`
- Create: `src/components/app/BottomStatusBar.tsx`
- Modify: `src/stores/navigationStore.ts`
- Create: `src/stores/projectStore.ts`
- Create: `src/stores/taskStore.ts`
- Modify: `src/styles.css`
- Modify: `src/i18n/locales/en.json`
- Modify: `src/i18n/locales/zh-CN.json`
- Modify: `src/app/App.test.tsx`

- [ ] Step 1: Write failing tests for shell navigation and status rendering.

  Run: `npm run test -- src/app/App.test.tsx`

  Expected before implementation: tests fail because navigation does not switch views and status data is static.

- [ ] Step 2: Split the shell into top bar, left sidebar, center workspace outlet, right context panel, and bottom status bar.

  Acceptance: `AppShell` stays mounted with or without a current project. In the no-project state the center shows exactly `New Knowledge Base` and `Open Existing Knowledge Base` as compact task cards; navigation remains visible with truthful dependency states, Settings remains usable, scope-less search is disabled, and the right panel explains local/open policy without Agent/BYOK setup. With a project, the left sidebar includes Dashboard, Wiki, Chat, Graph, Workflows, Import, Lint, Exports, Settings; the former `工作流` group is named `知识处理 / Knowledge Processing`; Workflows uses Lucide `Workflow` without a badge; the existing Agent status foot remains.

- [ ] Step 3: Move hardcoded labels into i18n keys.

  Acceptance: English and Chinese locale files both include shell labels, navigation labels, status labels, and empty-state labels; labels fit inside existing controls without truncating Chinese text at normal desktop widths.

- [ ] Step 4: Define CSS tokens from `SPEC/FRONTEND_GUIDELINES.md` and `SPEC/DESIGN.md`.

  Acceptance: `src/styles.css` exposes semantic variables for background, foreground, surfaces, borders, text colors, accent, danger, warning, info, radius, and font stacks; no decorative gradients, blobs, nested cards, or viewport-scaled font sizes are introduced.

- [ ] Step 5: Run frontend checks.

  Run: `npm run test`

  Expected: tests pass.

  Run: `npm run lint`

  Expected: lint passes with zero warnings.

- [ ] Step 6: Commit.

  Commit message: `feat: build desktop shell foundation`

## 5. Task 3: First-run Workbench, Typed Open Assessment, Trust, And Recovery

**Files:**

- Modify: `src/app/App.tsx`
- Modify: `src/components/app/AppShell.tsx`
- Modify: `src/features/project/ProjectStartView.tsx` or replace it with a no-project workspace feature inside `AppShell`
- Modify: `src/features/dashboard/DashboardView.tsx`
- Modify: `src/stores/projectStore.ts`
- Modify: `src/types/project.ts`
- Modify: `src-tauri/src/models/project.rs`
- Modify: `src-tauri/src/models/settings.rs`
- Modify: `src-tauri/src/services/project_service.rs`
- Modify: `src-tauri/src/services/settings_service.rs`
- Modify: `src-tauri/src/services/file_store.rs`
- Modify: `src-tauri/src/services/git_service.rs`
- Modify: `src-tauri/src/commands/project_commands.rs`
- Modify: `src-tauri/src/app_state.rs`
- Create or extend: backend assessment / trust / repair registries behind stable service facades
- Keep: the five creation-time templates under `src-tauri/templates/projects/`

- [ ] Step 1: Lock the no-project and native-creation contract with failing tests.

  Acceptance: `AppShell` remains mounted without a project and shows exactly two compact actions; creation accepts name + parent location + creation-time template, defaults to General, derives the final child path, handles CJK / Unicode / platform-invalid names, blocks an existing non-empty target, and rolls back partial creation transactionally.

- [ ] Step 2: Implement creation preferences and the Import handoff.

  Acceptance: the initial parent is Documents/LLM Wiki, the last successful parent is remembered globally, templates affect only initial root `purpose.md` and `schema.md`, no template-switch command exists after creation, and successful creation navigates to Import without automatically opening a system picker.

- [ ] Step 3: Replace binary folder detection with typed, zero-write quick assessment.

  Acceptance: `start_project_open_assessment` returns an application-scoped operation id; typed query/event delivery reports progress and completion; `cancel_project_open_assessment` accepts only that opaque id. Completion returns a backend-owned assessment id plus independent format, health, trust, filesystem access, layout, Git, capability, confidence, warning and recommended-action fields. The matrix covers current native, legacy native, `nashsu/llm_wiki`, Obsidian, Markdown vault, ambiguous Markdown, ordinary materials, damaged/recoverable and unreadable directories. Assessment creates no `.app`, Git repository, cache or project task and executes no folder content; cancellation discards the incomplete snapshot and keeps the no-project shell.

- [ ] Step 4: Implement intent routing without mutating ordinary materials.

  Acceptance: ambiguous Markdown offers `Open as Markdown Knowledge Base` or `Create a Knowledge Base with These Materials` and remembers the choice globally without a folder marker. Ordinary materials only enter the latter flow; the new project is created elsewhere and Import copies / archives confirmed inputs while the source folder remains byte-for-byte and structurally unchanged.

- [ ] Step 5: Implement independent trust, filesystem-access, health and capability policy.

  Acceptance: the backend derives an access policy for every `ProjectContext` from independent trust, filesystem access, health, layout and capabilities; trusted read-only is representable. Restricted mode allows Markdown/tree/local search/in-memory graph, background inventory and local quick health checks, but blocks external AI, Agent, Skill, project-writing tasks and writes. Read-only can remain permanent. Runtime path registration is not trust; global trust is keyed by canonical folder identity and invalidates after move, replacement or identity mismatch.

- [ ] Step 6: Implement compatibility enablement and Git policy.

  Acceptance: explicit enablement may create only `.app/` and `.app/compat/{purpose.md,schema.md}`; root same-name files remain user content and no `.app/project.json` is introduced. External opening never initializes or commits Git. The enablement page defaults local Git on; declining disables checkpoint-required writes while reading, search and explicitly authorized Chat remain available. Dirty worktrees are never auto-committed or stashed.

- [ ] Step 7: Implement repair planning and confirmation.

  Acceptance: safe derived state may be calculated before confirmation, but disk writes use a backend-held repair plan id and revalidate canonical identity, hashes, permissions and Git. The full page exposes `Trust, Repair, and Open` and `Open Restricted Without Repair`; readable Markdown remains available when repair is declined or `.app` is corrupt. Project-open repair is distinct from Wiki/Lint content repair.

- [ ] Step 8: Implement cancellable deep scan and truthful partial presentation.

  Acceptance: compatible projects enter the normal Dashboard immediately, deep scan runs read-only in the background, streams partial counts/warnings, and can be cancelled. Cancellation or failure leaves discovered Markdown readable; graph stays in memory until trusted+writable, and missing values explain `Scanning`, `Restricted`, or `Read-only` rather than appearing empty.

- [ ] Step 9: Implement deterministic startup and sample-vault coverage.

  Acceptance: when the single most recently opened knowledge base is valid, startup opens it and always lands Dashboard. With no history, or when that latest path is missing/inaccessible, startup keeps the full shell on the no-project workbench; the invalid-latest case shows its path error and never silently falls back to an older project. The sample wiki and representative Obsidian/Markdown/CJK/symlink fixtures open through the typed assessment path.

- [ ] Step 10: Run the risk-proportional checks and commit.

  Run the current full repository gate because this task changes filesystem, Git, IPC, trust, permission and background-task boundaries.

  Commit message: `feat: add safe first-run and project-open workbench`

## 6. Task 4: Git Safety, Confirmation Flow, And FileStore

**Files:**

- Modify: `src-tauri/src/services/file_store.rs`
- Modify: `src-tauri/src/services/git_service.rs`
- Create: `src-tauri/src/commands/git_commands.rs`
- Create: `src-tauri/src/commands/file_commands.rs`
- Modify: `src-tauri/src/models/git.rs`
- Modify: `src-tauri/src/models/confirmation.rs`
- Create: `src/components/app/ConfirmationDialog.tsx`
- Create: `src/types/backend.ts`

- [ ] Step 1: Write failing tests for safe JSON writes, safe Markdown writes, hash comparison, duplicate path handling, Git init, checkpoint, and diff generation.

  Run: `cargo test --manifest-path src-tauri/Cargo.toml file_store git_service -- --nocapture`

  Expected before implementation: tests fail for atomic JSON writes, no-overwrite behavior, and checkpoint creation.

- [ ] Step 2: Implement `FileStore` read/write helpers.

  Acceptance: Markdown reads preserve UTF-8; JSON writes are atomic; writes reject project-boundary escapes; writes do not silently overwrite unless the caller supplies an explicit overwrite mode and the service verifies the current hash.

- [ ] Step 3: Implement `GitService`.

  Acceptance: newly created native knowledge bases are initialized as Git repos and receive an initial commit. Assessing or opening an external knowledge base never initializes, stages, commits or stashes Git. Compatible enablement offers an explicit default-on Git choice; declining disables checkpoint-required writes. Dirty worktrees remain untouched unless the user explicitly approves a checkpoint containing all current changes. Checkpoint failure blocks the high-risk operation; generated diffs are Markdown-friendly and include affected paths.

- [ ] Step 4: Implement `confirm_pending_action`.

  Acceptance: high-risk operations resume only through backend-stored `PendingAction` ids; expired or state-mismatched actions fail safely; confirmation response says whether a Git checkpoint exists.

- [ ] Step 5: Add UI confirmation dialog.

  Acceptance: dialogs show what changes, affected paths, risk level, checkpoint status, and exact actions; cancel is always available; destructive actions use danger styling with accessible labels.

- [ ] Step 6: Run checks and commit.

  Run: `npm run test`

  Run: `npm run lint`

  Run: `cargo test --manifest-path src-tauri/Cargo.toml`

  Commit message: `feat: enforce git-backed safety`

## 7. Task 5: Import Archive And Extraction Preview

**Files:**

- Modify: `src-tauri/src/services/import_service.rs`
- Create: `src-tauri/src/services/extraction_service.rs`
- Create: `src-tauri/src/commands/import_commands.rs`
- Create: `src-tauri/src/models/import.rs`
- Create: `src-tauri/src/utils/url_utils.rs`
- Modify: `package.json`
- Modify: `package-lock.json`
- Create: `src/lib/readability.ts`
- Create: `src/features/import/ImportView.tsx`
- Create: `src/types/import.ts`
- Modify: `src/stores/taskStore.ts`
- Create: `docs/architecture/parser-adapters.md`

- [ ] Step 1: Write failing tests for file classification, duplicate handling, CJK filenames, folder import, and extraction preview records.

  Run: `cargo test --manifest-path src-tauri/Cargo.toml import_service extraction_service -- --nocapture`

  Expected before implementation: tests fail because source archiving and preview generation are missing.

- [ ] Step 2: Implement archive rules.

  Acceptance: every retained original and localized asset is resolved beneath the evidence root returned by `ProjectContext.layout` and organized by stable source channel/identity rather than media type alone. For a newly created native knowledge base, local files and media use `raw/sources/`, webpage/platform evidence uses `raw/web/<host>/`, and localized Source assets use `raw/assets/`; document type remains metadata. A compatible layout without an unambiguous writable evidence root returns a typed prerequisite instead of inventing `raw/`.

- [ ] Step 3: Implement duplicate and conflict handling.

  Acceptance: exact duplicate content reuses the existing logical `sourceId`, records a new locator as an alias when needed, and does not create a second Source; a changed version of the same logical source creates a new immutable `versionId` while protecting the edited current Source. Same-name different-content evidence is renamed deterministically. Conflicts, renames, failures, hashes, aliases, versions, and source paths are written beneath `ProjectLayout.importStateRoot` / `sourceStateRoot` (native conflict-record mapping: `.app/import-conflicts.json`) only when project app state is writable.

- [ ] Step 4: Implement extraction interface, Readability adapter, and parser adapter record.

  Acceptance: MD, TXT, CSV, and HTML extract text directly; backend URL fetch writes source metadata, `src/lib/readability.ts` wraps `@mozilla/readability` for article extraction, and backend file writes remain in `ExtractionService`; `docs/architecture/parser-adapters.md` records the selected PDF, DOCX, PPTX, and XLSX parser adapters before those adapters are added; MVP acceptance is blocked until valid PDF, DOCX, PPTX, and XLSX fixture files extract text, metadata, and preview statistics successfully, while corrupt files produce per-file failures without aborting the import batch.

- [ ] Step 5: Build import review UI.

  Acceptance: Import view shows file name, type, size, parse status, error reason, extracted text preview, pages or word count when available, conflicts, renamed paths, and keeps “导入到来源库” disabled until the user confirms the preview. Import does not expose a compile toggle or start Update Wiki.

- [ ] Step 6: Verify Import never starts compilation.

  Acceptance: tests prove `confirm_import_preview` revalidates trusted + writable access and requires layout-provided `appStateRoot`, `importStateRoot`, `sourceStateRoot`, `evidenceRoot`, and `sourceWriteRoot`; every successful item atomically commits both immutable evidence and a readable Source plus source/version state, while a failed or unconfirmed item creates neither half. It never creates a Wiki compile task. After a successful commit, “用这些来源更新 Wiki” may carry the committed change set into the separate shared Workflows preparation flow, which still requires a distinct user start action.

- [ ] Step 7: Run checks and commit.

  Run: `npm run test`

  Run: `npm run lint`

  Run: `cargo test --manifest-path src-tauri/Cargo.toml`

  Commit message: `feat: add import preview pipeline`

## 8. Task 6: TaskService, Events, Logs, Cancel, Tray, And Notifications

**Files:**

- Modify: `src-tauri/src/tasks/task_service.rs`
- Create: `src-tauri/src/tasks/task_model.rs`
- Create: `src-tauri/src/tasks/task_events.rs`
- Create: `src-tauri/src/tasks/cancellation.rs`
- Create: `src-tauri/src/commands/task_commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Create: `src/components/app/TaskLogDrawer.tsx`
- Create: `src/components/app/TaskActivityButton.tsx`
- Create: `src/types/task.ts`
- Modify: `src/stores/taskStore.ts`

- [ ] Step 1: Write failing tests for task creation, state transitions, progress, log append, cancellation, and persisted task recovery.

  Run: `cargo test --manifest-path src-tauri/Cargo.toml task_service -- --nocapture`

  Expected before implementation: tests fail for task lifecycle and cancellation.

- [ ] Step 2: Implement task lifecycle.

  Acceptance: tasks support `queued`, `running`, `waiting_for_confirmation`, `cancelling`, `cancelled`, `succeeded`, `failed`, and user-visible `interrupted`; when project app state is writable, task state is written under `ProjectLayout.taskStateRoot` (native mapping: `.app/tasks/`) and logs are append-only. Permitted restricted/read-only inventory, Import discovery/extraction/preview, Local Quick Check, and trusted read-only Complete Check runs remain explicitly ephemeral or use backend-owned temporary space, are labeled non-persistent, and never try to create `.app/`. Task ids are stable UUIDs. Persisted workflow tasks also include runtime `project_id`, canonical identity key/revision, kind, scope, baseline, input fingerprint, structured stage/activity, and optional `attempt_of`.

- [ ] Step 3: Implement Tauri event envelope.

  Acceptance: backend emits structured events for task updated, log, completed, failed, cancelled, confirmation requested, project refreshed, wiki changed, graph updated, and Agent output.

- [ ] Step 4: Add task UI.

  Acceptance: bottom status and task drawer show only the active project's tasks, running state, structured progress, secondary logs, cancel actions, failure details, and result links; task updates survive navigating between views. Other projects continue independently but are visible only after switching projects.

- [ ] Step 5: Add tray and notification integration.

  Acceptance: closing the main window follows the configured behavior; default is minimize to tray while tasks continue; completion, failure, and confirmation-needed notifications route to the relevant result, error log, or diff confirmation view.

- [ ] Step 6: Run checks and commit.

  Run: `npm run test`

  Run: `npm run lint`

  Run: `cargo test --manifest-path src-tauri/Cargo.toml`

  Commit message: `feat: add cancellable background tasks`

## 9. Task 7: Wiki Read, Markdown Render, Edit, And Search

**Files:**

- Create: `src-tauri/src/commands/wiki_commands.rs`
- Create: `src-tauri/src/commands/search_commands.rs`
- Create: `src-tauri/src/models/wiki.rs`
- Create: `src-tauri/src/models/search.rs`
- Modify: `src-tauri/src/services/search_service.rs`
- Create: `src-tauri/src/utils/markdown_utils.rs`
- Create: `src/features/wiki/WikiView.tsx`
- Create: `src/features/wiki/WikiTree.tsx`
- Create: `src/features/wiki/MarkdownReader.tsx`
- Create: `src/features/wiki/WikiEditor.tsx`
- Create: `src/features/wiki/RelatedPagesPanel.tsx`
- Create: `src/types/wiki.ts`

- [ ] Step 1: Write failing tests for frontmatter parsing, wikilink parsing, wiki tree scanning, keyword search, type filtering, source filtering, and save conflict detection.

  Run: `cargo test --manifest-path src-tauri/Cargo.toml wiki search markdown -- --nocapture`

  Expected before implementation: tests fail for parsing and search capabilities.

- [ ] Step 2: Implement wiki scanning and page metadata.

  Acceptance: pages from the layout-allowed Wiki read roots are returned as a tree with path, title, inferred type, tags, source metadata, star/bookmark state, file size, modified time, and external modification hash; compatible vaults do not need a native `wiki/` directory.

- [ ] Step 3: Implement Markdown rendering.

  Acceptance: reader supports GFM tables, code highlighting, math, KaTeX, YAML frontmatter display, and clickable `[[wikilinks]]`; missing links are visually distinct and do not crash rendering.

- [ ] Step 4: Implement Milkdown editing and save.

  Acceptance: user can switch read/edit modes, edit WYSIWYG content, save Markdown, and see save state; external modifications trigger a diff confirmation path instead of silent overwrite.

- [ ] Step 5: Implement local keyword search and filters.

  Acceptance: search supports title, full-text keyword, tag, type, and source filters; global search never calls LLM or Agent; selected result opens the page and highlights the match when practical.

- [ ] Step 6: Refresh caches after save.

  Acceptance: after saving a page, index/search metadata refreshes, graph cache is marked stale when persistence is allowed, and the layout-defined log records the user-visible save event when configured.

- [ ] Step 7: Run checks and commit.

  Run: `npm run test`

  Run: `npm run lint`

  Run: `cargo test --manifest-path src-tauri/Cargo.toml`

  Commit message: `feat: add wiki reading and search`

## 10. Task 8: Execution Routes, Update Wiki, And Workflows Surface

**Files:**

- Modify: `src-tauri/src/services/agent_service.rs`
- Modify: `src-tauri/src/services/llm_service.rs`
- Create: `src-tauri/src/services/secret_service.rs`
- Create: `src-tauri/src/commands/agent_commands.rs`
- Create: `src-tauri/src/commands/llm_commands.rs`
- Create: `src-tauri/src/models/agent.rs`
- Create: `src-tauri/src/models/llm.rs`
- Create: `src-tauri/templates/skills/wiki-ingest/SKILL.md`
- Migrate: `src/features/agent/AgentView.tsx` into the Workflows main surface; the final folder/name may become `src/features/workflows/`
- Create: Workflows preparation, task-detail, and history components under the selected feature folder
- Create: `src/features/settings/LlmProviderSettings.tsx`
- Create: `src/types/agent.ts`
- Create: `src/types/llm.ts`

- [ ] Step 1: Write failing tests with fake Agent and fake LLM adapters.

  Run: `cargo test --manifest-path src-tauri/Cargo.toml agent_service llm_service -- --nocapture`

  Expected before implementation: tests fail because CLI detection, task output, provider validation, and fake adapters are missing.

- [ ] Step 2: Implement Agent CLI detection.

  Acceptance: app detects `claude`, `codex`, `openclaw`, and `hermes` from PATH; captures version output; shows installed, missing, failed, and default states; failed detection does not block app startup.

- [ ] Step 3: Implement Agent task spawn through TaskService.

  Acceptance: Agent tasks stream stdout and stderr to task logs and UI events, support cancellation, background operation, and result status; install commands are shown as guidance only and never executed without explicit confirmation.

- [ ] Step 4: Implement BYOK provider configuration without storing secrets in project files.

  Acceptance: provider metadata for OpenAI, Anthropic, Google, Ollama, and Custom can be saved without API keys; API keys are stored and read only through `SecretService`; tests assert every persisted project settings/Agent-config record contains no key material, including native `.app/settings.json` and `.app/agent-config.json`.

- [ ] Step 5: Implement compile request orchestration.

  Acceptance: Update Wiki requires a trusted writable project and reads purpose/schema/page roots from backend-derived `ProjectContext.layout`; for a native project these resolve to root `purpose.md`, `schema.md`, confirmed `wiki/sources/`, and existing `wiki/`. It defaults to changed `sourceId + versionId` Sources; creates the required Git checkpoint before writes; uses the Settings default route or an explicit per-run override without silent fallback; writes candidates through the conflict-safe merge path; updates the layout-defined index/overview/log outputs; never writes Source pages; refreshes search and graph stale state.

- [ ] Step 6: Replace the Agent main panel with Workflows.

  Execution: follow [`docs/superpowers/plans/2026-07-30-workflows-panel-implementation.md`](docs/superpowers/plans/2026-07-30-workflows-panel-implementation.md) batch by batch; do not expose the new route before its three runners are valid or remove legacy launch paths before shared-entry integration passes.

  Acceptance: Workflows provides the fixed `更新 Wiki`, `健康检查`, and `生成内容` rows; adaptive overview; full preparation view instead of a Run Agent dialog; active-project-only serial queue and input-fingerprint dedupe; observable structured pipelines; non-modal waiting confirmation; cancellation, linked retry, interrupted recovery explanation, and project-scoped history. No-project creates no task; restricted blocks external execution; mutations require trusted writable and the declared Git policy; prepare/start both revalidate canonical project identity and access. Raw logs are secondary. Agent/Provider configuration remains in Settings, while Lint and Exports retain their existing result pages.

- [ ] Step 7: Run checks and commit.

  Run: `npm run test`

  Run: `npm run lint`

  Run: `cargo test --manifest-path src-tauri/Cargo.toml`

  Commit message: `feat: add workflows and execution routes`

## 11. Task 9: Graph Build, Cache, And Sigma View

**Files:**

- Modify: `src-tauri/src/services/graph_service.rs`
- Create: `src-tauri/src/commands/graph_commands.rs`
- Create: `src-tauri/src/models/graph.rs`
- Create: `src/features/graph/GraphView.tsx`
- Create: `src/features/graph/GraphControls.tsx`
- Create: `src/features/graph/GraphInspector.tsx`
- Create: `src/types/graph.ts`
- Modify: `src/stores/graphStore.ts`

- [ ] Step 1: Write failing tests for Source/Wiki node extraction, wikilink edges, inferred page types, restricted/read-only memory-only results, trusted cache read/write, partial deep scan, corrupted cache recovery, and CJK path labels.

  Run: `cargo test --manifest-path src-tauri/Cargo.toml graph_service -- --nocapture`

  Expected before implementation: tests fail because graph data and cache handling are missing.

- [ ] Step 2: Implement backend graph data.

  Acceptance: every Markdown document in the layout-allowed Source/Wiki roots becomes one page-level node; no compile prerequisite exists; node type comes from frontmatter or directory; edges come from `[[wikilinks]]` and the MVP multi-signal association model; all edges use relation label `related`. Restricted/read-only projects return bounded in-memory results without cache writes; trusted writable projects cache through `ProjectContext.layout.graphCachePath` (native mapping: `.app/graph-cache.json`).

- [ ] Step 3: Implement layout and community support.

  Acceptance: first/deep build can run as a cancellable background task; partial results carry scanned/pending counts; cached graph opens in seconds for the sample `wiki/wiki/`; Louvain community ids and ForceAtlas2 positions are persisted or reused only when the project access mode permits and inputs have not changed.

- [ ] Step 4: Build sigma.js graph view.

  Acceptance: graph view has full central canvas, filter/search controls, type color and community color toggle, hover neighbor highlight, click-to-open article, zoom, drag, fit-to-screen, reset layout, and right-panel selected node details.

- [ ] Step 5: Verify rendering.

  Acceptance: desktop viewport shows a nonblank graph canvas for the sample wiki, controls do not overlap, labels do not dominate the canvas, and selected node navigation opens the Wiki view.

- [ ] Step 6: Run checks and commit.

  Run: `npm run test`

  Run: `npm run lint`

  Run: `cargo test --manifest-path src-tauri/Cargo.toml`

  Commit message: `feat: add wiki graph`

## 12. Task 10: Chat Sessions, Retrieval, Citations, And Save To Wiki

**Files:**

- Create: `src-tauri/src/commands/chat_commands.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Create: `src-tauri/src/models/chat.rs`
- Modify: `src-tauri/src/services/search_service.rs`
- Modify: `src-tauri/src/services/agent_service.rs`
- Modify: `src-tauri/src/services/llm_service.rs`
- Create: `src/features/chat/ChatView.tsx`
- Create: `src/features/chat/ChatSessionList.tsx`
- Create: `src/features/chat/ChatComposer.tsx`
- Create: `src/features/chat/CitationPanel.tsx`
- Create: `src/types/chat.ts`
- Modify: `src/stores/chatStore.ts`

- [ ] Step 1: Write failing tests for chat JSON persistence, session rename/delete, Source/Wiki retrieval context assembly, typed citation serialization, trust/configuration prerequisites, and save-to-query page access/Git policy.

  Run: `cargo test --manifest-path src-tauri/Cargo.toml chat search llm_service -- --nocapture`

  Expected before implementation: tests fail because chat persistence and retrieval are missing.

- [ ] Step 2: Implement chat session storage.

  Acceptance: when project app state is writable, chat sessions are stored under `ProjectLayout.chatStateRoot` (native mapping `.app/chats/{id}.json`); read-only sessions are explicitly ephemeral. Create, rename, delete, list, and load operations work where persistence is available; corrupted chat files are reported with recoverable errors and do not crash app startup.

- [ ] Step 3: Implement retrieval context.

  Acceptance: Chat uses local SearchService to retrieve relevant readable Source or Wiki pages, includes typed citations, layout-resolved purpose text, and bounded chat history, then sends the request to the explicit Agent or BYOK route. Source-only projects work without compile; restricted projects never send content externally; configuration return preserves the draft and does not auto-send; global search remains keyword-only.

- [ ] Step 4: Implement response and citations UI.

  Acceptance: chat stream or nonstream response shows assistant answer, Source/Wiki citations, snippets, type-aware click-to-open links, running state, cancel action, and clear provider/Agent route.

- [ ] Step 5: Implement save answer to the layout-defined queries root.

  Acceptance: saved answers become Markdown files under the layout-defined queries root (native mapping `wiki/queries/`) with frontmatter, question, answer, typed citations, created timestamp, and source paths; the backend requires writable access and creates a Git checkpoint when writing multiple files or changing existing query pages. Pure Chat does not require Git.

- [ ] Step 6: Run checks and commit.

  Run: `npm run test`

  Run: `npm run lint`

  Run: `cargo test --manifest-path src-tauri/Cargo.toml`

  Commit message: `feat: add cited wiki chat`

## 13. Task 11: Lint Local Rules, Agent Deep Lint, And Fix Flow

**Files:**

- Create: `src-tauri/src/services/lint_service.rs`
- Create: `src-tauri/src/commands/lint_commands.rs`
- Create: `src-tauri/src/models/lint.rs`
- Create: `src-tauri/templates/skills/wiki-lint/SKILL.md`
- Create: `src/features/lint/LintView.tsx`
- Create: `src/features/lint/LintIssueList.tsx`
- Create: `src/features/lint/LintIssueDetails.tsx`
- Create: `src/types/lint.ts`

- [ ] Step 1: Write failing tests for every local deterministic lint rule.

  Run: `cargo test --manifest-path src-tauri/Cargo.toml lint_service -- --nocapture`

  Expected before implementation: tests fail for dead links, isolated pages, missing frontmatter, index drift, empty pages, duplicate filenames, path case problems, and missing resources.

- [ ] Step 2: Implement local lint.

  Acceptance: local lint returns issues with id, severity, issue type, path, range when available, message, evidence, fixability, and suggested action; no LLM or Agent call is made for local lint.

- [ ] Step 3: Implement Agent deep lint task.

  Acceptance: deep lint uses `skills/wiki-lint/SKILL.md`, runs through AgentService and TaskService, displays stdout/stderr, supports cancellation, and returns issues for duplicate topics, weak cross-references, missing sources, schema mismatch, outdated content, and contradictions.

- [ ] Step 4: Implement fix planning and safe apply.

  Acceptance: deterministic auto-fixes create a Git checkpoint first; high-risk delete, overwrite, batch rewrite, and conflict actions return `PendingAction`; completed fixes commit results and refresh wiki/search/graph state.

- [ ] Step 5: Build Lint UI.

  Acceptance: Lint view separates local and Agent issues, groups by severity and type, shows selected issue details, suggested fix, affected paths, checkpoint behavior, and batch-fix confirmation.

- [ ] Step 6: Run checks and commit.

  Run: `npm run test`

  Run: `npm run lint`

  Run: `cargo test --manifest-path src-tauri/Cargo.toml`

  Commit message: `feat: add wiki lint workflow`

## 14. Task 12: HTML, Card, Concept Map, And Project Report Exports

**Files:**

- Modify: `src-tauri/src/services/export_service.rs`
- Create: `src-tauri/src/commands/export_commands.rs`
- Create: `src-tauri/src/models/export.rs`
- Create: `src-tauri/templates/skills/html-beautiful-read/SKILL.md`
- Create: `src-tauri/templates/skills/html-beautiful-read/template.html`
- Create: `src-tauri/templates/skills/html-knowledge-card/SKILL.md`
- Create: `src-tauri/templates/skills/html-knowledge-card/template.html`
- Create: `src-tauri/templates/skills/html-concept-map/SKILL.md`
- Create: `src-tauri/templates/skills/html-concept-map/template.html`
- Create: `src-tauri/templates/skills/html-project-report/SKILL.md`
- Create: `src-tauri/templates/skills/html-project-report/template.html`
- Create: `src/features/exports/ExportsView.tsx`
- Create: `src/features/exports/HtmlPreviewPane.tsx`
- Create: `src/types/export.ts`

- [ ] Step 1: Write failing tests for export target path safety, skill selection, template isolation, output file creation, and iframe preview path.

  Run: `cargo test --manifest-path src-tauri/Cargo.toml export_service -- --nocapture`

  Expected before implementation: tests fail because skill export orchestration is missing.

- [ ] Step 2: Implement export jobs.

  Acceptance: single article beautiful read, knowledge card, concept map, and project report jobs run through the matching `skills/html-*` folder, write outputs only to the export root returned by `ProjectContext.layout` (native projects resolve to `exports/html/`), and never modify schema guidance, Wiki pages, or lint rules as a side effect.

- [ ] Step 3: Implement preview and open-location support.

  Acceptance: exports can be previewed in an iframe-safe app view, opened in the OS file manager, regenerated, and traced to their source page or project snapshot.

- [ ] Step 4: Build Exports UI.

  Acceptance: Exports view lists prior exports, source path, output path, type, created time, generation route, status, preview, regenerate, and open-folder actions.

- [ ] Step 5: Run checks and commit.

  Run: `npm run test`

  Run: `npm run lint`

  Run: `cargo test --manifest-path src-tauri/Cargo.toml`

  Commit message: `feat: add skill-driven exports`

## 15. Task 13: Settings, Secrets, Language, Theme, And Updates

**Files:**

- Modify: `src-tauri/src/services/settings_service.rs`
- Create: `src-tauri/src/services/secret_service.rs`
- Create: `src-tauri/src/commands/settings_commands.rs`
- Create: `src-tauri/src/models/settings.rs`
- Create: `src/features/settings/SettingsView.tsx`
- Create: `src/features/settings/AppearanceSettings.tsx`
- Create: `src/features/settings/LanguageSettings.tsx`
- Create: `src/features/settings/AgentSettings.tsx`
- Create: `src/features/settings/LlmProviderSettings.tsx`
- Create: `src/features/settings/SecuritySettings.tsx`
- Create: `src/features/settings/BackgroundTaskSettings.tsx`
- Create: `src/features/settings/UpdateSettings.tsx`
- Create: `src/types/settings.ts`
- Modify: `src/stores/settingsStore.ts`
- Modify: `src/i18n/index.ts`

- [ ] Step 1: Write failing tests for settings persistence, global versus project settings separation, secret round trip, and no-secret-in-project-files guarantee.

  Run: `cargo test --manifest-path src-tauri/Cargo.toml settings_service secret_service -- --nocapture`

  Expected before implementation: tests fail because settings and credential storage are incomplete.

- [ ] Step 2: Implement settings service.

  Acceptance: project settings write to `ProjectLayout.settingsPath` (native mapping: `.app/settings.json`) when it is writable; global settings write to the app config directory; settings include language, theme, startup behavior, Agent default binding, provider metadata, context window from 4K to 1M tokens, close-window behavior, and update preferences.

- [ ] Step 3: Implement SecretService.

  Acceptance: secrets save, load, delete, and configured-state checks work through OS credential storage or a test fake; full API keys are never returned to UI by default and never written to logs or project files.

- [ ] Step 4: Build settings UI.

  Acceptance: settings groups are General, Appearance, Language, Agent, LLM providers, Security, Background tasks, and Updates; provider testing shows clear success/failure; theme and language changes apply without restart when technically possible.

- [ ] Step 5: Implement update check confirmation flow.

  Acceptance: check update can report current/latest state; download and install require explicit user confirmation; update errors are recoverable and do not block core app use.

- [ ] Step 6: Run checks and commit.

  Run: `npm run test`

  Run: `npm run lint`

  Run: `cargo test --manifest-path src-tauri/Cargo.toml`

  Commit message: `feat: add settings and secrets`

## 16. Task 14: End-To-End MVP Flow

**Files:**

- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/services/mod.rs`
- Modify: `src/app/App.tsx`
- Modify: `src/components/app/AppShell.tsx`
- Modify: `src/stores/projectStore.ts`
- Modify: `src/stores/taskStore.ts`
- Modify: `src/stores/chatStore.ts`
- Modify: `src/stores/graphStore.ts`
- Modify: `src/features/dashboard/DashboardView.tsx`
- Modify: `src/features/import/ImportView.tsx`
- Modify: `src/features/wiki/WikiView.tsx`
- Modify: `src/features/graph/GraphView.tsx`
- Modify: `src/features/chat/ChatView.tsx`
- Modify: the migrated Workflows feature (legacy source: `src/features/agent/AgentView.tsx`)
- Modify: `src/features/lint/LintView.tsx`
- Modify: `src/features/exports/ExportsView.tsx`
- Modify: `src/features/settings/SettingsView.tsx`
- Create: `src/test/fixtures/project-fixtures.ts`
- Create: `src-tauri/tests/mvp_flow.rs`
- Create: `docs/qa/mvp-acceptance.md`

- [ ] Step 1: Create a small multi-format validation fixture.

  Acceptance: fixture covers PDF, DOCX, PPTX, XLSX, CSV, MD, TXT, HTML, URL metadata, and clipboard-like Markdown text; unsupported parser gaps are explicitly represented as partial extraction results rather than hidden.

- [ ] Step 2: Write failing end-to-end tests for the MVP loop.

  Run: `cargo test --manifest-path src-tauri/Cargo.toml --test mvp_flow -- --nocapture`

  Expected before final integration: tests fail at whichever flow is still incomplete.

- [ ] Step 3: Pass the project-to-wiki loop.

  Acceptance: a first-time user sees the full-shell two-action workbench, creates a knowledge base, lands in Import without an automatic picker, imports validation sources, reviews and confirms at least one Source, and can immediately read that committed Source without configuring AI or compiling. The user may then separately start Update Wiki from the shared workflow preparation model, generate `wiki/index.md`, `wiki/overview.md`, `wiki/log.md`, open a page, search it, and see graph partial/stale/built state.

- [ ] Step 4: Pass the sample-wiki loop.

  Acceptance: the app opens `wiki/wiki/` through typed zero-write assessment, scans the page tree, searches pages, renders Markdown, builds or loads graph cache according to access policy, and opens graph within the performance target for 200-500 pages. Fixtures also cover compatible restricted, read-only, ambiguous, ordinary-materials, damaged/recovery, dirty-Git and deep-scan-cancel paths.

- [ ] Step 5: Pass the AI-assisted loop with fakes.

  Acceptance: fake Agent and fake BYOK can execute the applicable Update Wiki, Chat, Health Check, and Generate Content paths without real API keys or real CLI dependency; all workflow launches use the same project-scoped task model.

- [ ] Step 6: Pass safety loop.

  Acceptance: low-risk conflict-free changes auto-apply only when that operation's backend `CheckpointPolicy` is satisfied. Update Wiki and safe repairs that require a checkpoint apply after it succeeds; Health Check never mutates or creates a checkpoint; Generate Content creates a new artifact without a checkpoint, while overwrite requires both checkpoint and confirmation. Destructive operations, source replacement, delete, broad rewrite, and conflict merge wait for explicit confirmation and a required checkpoint; conflict UI offers keep current, use generated, and manual merge choices.

- [ ] Step 7: Document QA evidence.

  Acceptance: `docs/qa/mvp-acceptance.md` records command outputs, fixture paths, sample wiki counts, known parser limitations, and screenshots or descriptions of graph/chat/export/lint flows.

- [ ] Step 8: Run full checks and commit.

  Run: `npm run test`

  Run: `npm run lint`

  Run: `npm run build`

  Run: `cargo test --manifest-path src-tauri/Cargo.toml`

  Commit message: `test: verify mvp flow`

## 17. Task 15: Visual Polish, Accessibility, Packaging, And Release Readiness

**Files:**

- Modify: `src/styles.css`
- Modify: `src/components/app/AppShell.tsx`
- Modify: `src/components/app/TopBar.tsx`
- Modify: `src/components/app/LeftSidebar.tsx`
- Modify: `src/components/app/RightContextPanel.tsx`
- Modify: `src/components/app/BottomStatusBar.tsx`
- Modify: `src/features/dashboard/DashboardView.tsx`
- Modify: `src/features/wiki/WikiView.tsx`
- Modify: `src/features/import/ImportView.tsx`
- Modify: `src/features/graph/GraphView.tsx`
- Modify: `src/features/chat/ChatView.tsx`
- Modify: the migrated Workflows feature (legacy source: `src/features/agent/AgentView.tsx`)
- Modify: `src/features/lint/LintView.tsx`
- Modify: `src/features/exports/ExportsView.tsx`
- Modify: `src/features/settings/SettingsView.tsx`
- Modify: `src-tauri/tauri.conf.json`
- Modify: `src-tauri/Cargo.toml`
- Create: `docs/qa/frontend-visual-checklist.md`
- Create: `docs/qa/platform-checklist.md`

- [ ] Step 1: Verify Codex-like visual checklist.

  Acceptance: every view uses compact panes, lists, toolbars, drawers, log panels, and inspectors. The no-project state preserves the full shell, contains exactly two compact task cards, and has no recent-project grid, template wall, Agent/BYOK setup or third Import action. No landing hero, decorative gradient, bokeh, nested card layout, glossy AI visual, or generic SaaS dashboard styling is present.

- [ ] Step 2: Verify accessibility.

  Acceptance: keyboard can reach all controls; focus rings are visible; icon-only controls have accessible labels and tooltips; dialogs trap focus; toasts and task updates do not steal focus; state is not color-only.

- [ ] Step 3: Verify text fit in Chinese and English.

  Acceptance: all main views fit zh-CN and en labels at the minimum target width of 1120px; under 960px the right panel collapses; under 760px the left sidebar collapses to icons; no button text overlaps.

- [ ] Step 4: Verify platform packaging readiness.

  Acceptance: Windows, macOS, and Linux path styles are covered by tests; CJK filenames pass import, wiki scan, graph, lint, and export tests; Tauri configuration contains app metadata and permissions aligned with actual features.

- [ ] Step 5: Run release checks.

  Run: `npm run test`

  Run: `npm run lint`

  Run: `npm run build`

  Run: `cargo test --manifest-path src-tauri/Cargo.toml`

  Run: `npm run tauri -- build`

  Expected: all checks pass or the exact missing platform dependency is documented in `docs/qa/platform-checklist.md`.

- [ ] Step 6: Commit.

  Commit message: `chore: prepare mvp release`

## 18. Required Review Workflow Per Milestone

After each feature or meaningful fix:

- [ ] Run `npm run test` and `npm run lint`.
- [ ] Launch Review Subagent A with shared context to review design intent, logic, consistency, and integration with existing docs.
- [ ] Launch Review Subagent B with fresh context to review blind spots, missing tests, unclear behavior, and regression risk.
- [ ] Merge both review results into a short findings list.
- [ ] Fix all valid findings.
- [ ] Rerun `npm run test`, `npm run lint`, and any Rust or build checks relevant to the touched files.
- [ ] Add a top entry to root `progress.txt`.

Acceptance:

- If subagents are unavailable, the main agent performs the two reviews manually and says so in the final report for that milestone.
- A milestone is not complete until valid review findings are fixed and checks have rerun from the beginning.

## 19. MVP Acceptance Matrix

Project lifecycle:

- No-project mode keeps the complete desktop shell and exposes exactly New Knowledge Base and Open Existing Knowledge Base.
- New native knowledge bases create the required directory/state files transactionally, default to General, and hand off to Import without an automatic picker.
- Existing LLM Wiki, `nashsu/llm_wiki`-style, Obsidian and Markdown folders open through typed, zero-write assessment with separate format, health, trust, filesystem access and layout.
- Ordinary materials are never initialized, moved, renamed or marked in place; a separate knowledge base is created and confirmed inputs are copied / archived through Import.
- Compatible contexts enforce capabilities derived from independent trust, filesystem access, health and layout; trusted read-only and untrusted read-only are both representable, and `.app/compat` never overwrites root user files.
- A valid latest history entry auto-opens that knowledge base and lands Dashboard; absent history shows the no-project workbench, while an invalid/inaccessible latest entry shows the same workbench plus its path error and never silently falls back to an older project.

Import and extraction:

- PDF, DOCX, PPTX, XLSX, CSV, MD, TXT, HTML, URL, clipboard text, and folders enter preview.
- Preview shows name, type, size, status, error reason, text preview, and pages or word count where available.
- Every successful item commits immutable evidence and a readable Source plus source/version state through the layout-provided roots; failed or unconfirmed items commit neither. Native local/web/assets mappings follow stable source channel and identity rather than media-type folders.
- Exact duplicate content reuses one `sourceId` and records new locators as aliases; refreshed content creates a new immutable `versionId` and preserves user edits through Diff or three-way merge.
- Import never starts Update Wiki; the completion CTA only opens the shared workflow preparation flow.

Wiki and editing:

- Compile creates or updates the layout-defined index, overview and log outputs; native projects map them to `wiki/index.md`, `wiki/overview.md`, and `wiki/log.md`.
- Markdown reader supports GFM, code highlight, math, KaTeX, frontmatter, and wikilinks.
- WYSIWYG editing saves safely and detects external modifications.

Graph:

- Page-level nodes match the readable Source/Wiki Markdown roots within documented exclusions; Source-only projects do not require compile.
- Type coloring, community coloring, ForceAtlas2 layout, Louvain communities, hover neighbors, click navigation, zoom, drag and fit-to-screen are available; cache is written only for trusted writable projects and deep scans expose partial state.

Chat:

- Multiple sessions can be created, renamed, deleted, and persisted.
- Answers use readable Source/Wiki retrieval and show type-aware clickable citations; restricted projects do not send content externally.
- Good answers can be saved to the layout-defined queries root; native projects map it to `wiki/queries/`.

Workflows and execution routes:

- Workflows exposes only Update Wiki, Health Check, and Generate Content in the first release; preparation, queueing, task detail, confirmation, retry, and history use one project-scoped task model.
- Same-project workflows run serially and identical inputs dedupe; task presentation never mixes projects.
- No-project creates no workflow task; external execution requires trust; mutations require writable access and revalidated Git policy.
- Each workflow exposes a structured pipeline before raw logs and handles queued, running, waiting, completed, failed, cancelled, and interrupted states.
- Agent CLI detection shows installed/missing/failed states and versions.
- Agent tasks stream logs, run in background, and cancel.
- Settings owns Agent/BYOK configuration; the selected route never silently falls back.
- BYOK works for applicable Update Wiki, Generate Content, and Chat flows when configured.
- Agent install commands are never run silently.

Lint:

- Local lint catches dead links, isolated pages, missing frontmatter, index drift, empty pages, duplicate filenames, case issues, and missing resources.
- Agent deep lint uses `wiki-lint`.
- Fixes use Git checkpoints and confirmation for high-risk changes.

Exports:

- Single article HTML, knowledge card, concept map, and project report generate under the layout-defined export root; native projects resolve it to `exports/html/`.
- Export preview works inside the app and can open the output folder.
- HTML templates do not mutate Wiki schema, lint rules, or source pages.

Safety and platform:

- Git checkpoints protect destructive, overwrite, batch rewrite, checkpoint-required workflow auto-fix, conflict merge, and source replacement flows. Low-risk conflict-free changes auto-apply only after any policy-required checkpoint; Health Check stays read-only, and a newly created Generate Content artifact needs no checkpoint. High-risk changes wait for confirmation.
- Quick assessment performs zero writes and never initializes Git. Compatible Git initialization is explicit; declining disables checkpoint-required writes, and dirty worktrees are never auto-committed or stashed.
- Global trust is bound to canonical directory identity and invalidates after move/replacement; runtime project registration is not trust.
- Restricted/read-only gates, root symlink containment, external-link non-following and case/Unicode-normalization conflict reporting are tested.
- API keys never appear in project files, logs, exports, or snapshots.
- CJK filenames, Unicode paths, Windows/macOS/Linux path styles, and case-sensitivity cases are tested.
- Long tasks are cancellable, logged, progress-visible, background-safe, and notification-aware.

Final verification commands:

```powershell
npm run test
npm run lint
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
npm run tauri -- build
```
