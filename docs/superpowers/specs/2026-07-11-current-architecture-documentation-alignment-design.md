# Current Architecture Documentation Alignment Design

## Goal

Align the living project specifications with the AppShell workflow-controller refactor and the Rust service use-case modularization currently present on `master`, without changing product scope, runtime behavior, public IPC contracts, or historical evidence.

## Scope

The following living documents will be updated:

- `SPEC/SPEC.md`
- `SPEC/APP_flow.md`
- `SPEC/TECH_STACK.md`
- `SPEC/BACKEND_STRUCTURE.md`
- `SPEC/FRONTEND_GUIDELINES.md`
- affected living roadmap files under `SPEC/roadmap/`
- `SPEC/progress.txt`

The two implementation plans that produced the current architecture will receive a short completion/supersession notice only:

- `docs/superpowers/plans/2026-07-10-app-shell-workflow-controllers.md`
- `docs/superpowers/plans/2026-07-10-service-use-case-modularization.md`

`SPEC/DESIGN.md` is explicitly excluded by user direction and must remain byte-for-byte unchanged. Historical audits, old implementation plans, and prior progress/gotcha entries remain historical records; their original evidence, paths, and conclusions will not be rewritten.

## Source Of Truth

Documentation statements will be derived from the current `master` implementation, with these files as the primary architecture evidence:

- `src/components/app/AppShell.tsx`
- `src/components/app/WorkspaceController.tsx`
- `src/components/app/WorkspaceRouter.tsx`
- `src/components/app/ProjectConfirmationController.tsx`
- `src/hooks/useAiCapabilities.ts`
- `src/hooks/useTaskLauncher.ts`
- `src/features/import/useImportWorkflow.ts`
- `src/features/agent/useAgentWorkflow.ts`
- `src/features/settings/useProviderWorkflow.ts`
- `src-tauri/src/app_state.rs`
- `src-tauri/src/lib.rs`
- `src-tauri/src/services/mod.rs`
- `src-tauri/src/services/{import_service,search_service,lint_service,chat_service}/`
- `src-tauri/tests/service_facade_contracts.rs`

Product requirements and safety constraints continue to come from `SPEC/PRD.md`, `SPEC/SPEC.md`, `AGENTS.md`, and the existing confirmation, task, secret, Git, path, and persistence contracts.

## Documentation Strategy

Use a targeted living-document rewrite rather than either append-only notes or a wholesale product-spec rewrite.

- Preserve confirmed product goals, storage models, security rules, and user-visible behavior.
- Replace statements that describe the repository as uninitialized or the four modularized services as single files.
- Describe implemented boundaries as current facts, not recommendations.
- Retain forward-looking guidance only where the code intentionally leaves an extension point.
- Update living roadmap status and evidence where the refactors changed implementation state or file locations.
- Add a path-migration note instead of rewriting historical audit evidence.

## Frontend Architecture To Document

The stable frontend boundary is:

```text
AppShell
  -> desktop frame, pane sizing, responsive panel behavior, global shortcuts
  -> WorkspaceController
       -> composes project-scoped workflows and modal wiring
       -> WorkspaceRouter
            -> lazy feature-view dispatch
            -> Suspense + ViewErrorBoundary
  -> ProjectConfirmationController
       -> project PendingAction and compile-conflict confirmation
  -> TaskLogDrawer / Toaster
```

The workflow boundaries are:

- `useAiCapabilities`: agent/provider discovery and guarded route updates.
- `useTaskLauncher`: compile, deep-lint, export, and cancel task launches; every returned task is upserted into `taskStore`, and the drawer opens only for the still-current project.
- `useImportWorkflow`: source list, preview, URL/clipboard import, source delete/replace requests, and the fixed confirm -> scan -> optional compile sequence.
- `useProviderWorkflow`: provider metadata, secret commands, and provider tests; raw secrets never enter stores, logs, or toasts.
- `useAgentWorkflow`: Agent dialog state, default-agent updates, skill routing, and navigation.
- `ProjectConfirmationController`: global confirmation and compile-conflict UI orchestration.

All project-scoped asynchronous commits must compare both `projectId` and `rootPath`, or an equivalent stable project key. Superseded preview/test requests also use epochs. Hooks stay separated by domain; future work must not recreate one aggregate workflow hook.

Heavy feature views remain behind `React.lazy`; `WorkspaceRouter` keeps `Suspense` and `ViewErrorBoundary` colocated. Dashboard remains the deliberate eager/default view. Controller imports must not pull Graph, editor, Markdown renderer, or Readability code back into the initial bundle.

## Backend Architecture To Document

`AppState`, Tauri command names, request/response DTOs, service facade type names, persistence formats, and safety boundaries remain stable. The four previously large services are now directory modules with focused `impl Service` blocks:

- `import_service/`: classification, source catalog, artifacts, promotion, preview, confirmation, source actions, and test support.
- `search_service/`: catalog/page lifecycle, query, excerpts, and test support.
- `lint_service/`: deterministic rules, ignores, reports/history, deep analysis, checkpoint-protected fixes, and test support.
- `chat_service/`: sessions, citations, retrieval/prompt assembly, saved answers, and test support.

Each directory exposes the same facade (`ImportService`, `SearchService`, `LintService`, `ChatService`) consumed by commands and `AppState`. `services/mod.rs` remains the crate-facing re-export boundary. Submodules use the narrowest practical visibility; tests move with the behavior they cover and facade contracts remain protected by `src-tauri/tests/service_facade_contracts.rs`.

`chat_convenience_service.rs` remains independent from `chat_service/`, and `wiki_index.rs` remains an independent shared index used through `SearchService`. Documentation must not imply either was folded into a modularized facade.

## Flow And Safety Statements To Preserve

- Import order is preview -> confirm -> Wiki scan -> optional compile.
- Every backend task continues through the shared task model, events, `taskStore`, and task drawer.
- Project switches suppress stale UI state, navigation, drawer opens, and error toasts without discarding tasks that belong to another project.
- Provider secrets go directly through Tauri IPC to OS credential storage and are never persisted in frontend state.
- High-risk operations remain backend-owned `PendingAction` flows with revalidation and Git checkpoints.
- Search remains local keyword/filter search; Chat/Agent/BYOK own natural-language answers.
- Commands remain thin and command registration remains explicit in `src-tauri/src/lib.rs`.
- Markdown, JSON, and local files remain the persistence source of truth; no database is introduced.

## Roadmap Alignment

Living roadmap files will be updated only where this architecture work changed status or evidence:

- `SPEC/roadmap/README.md`: add the current architecture/path migration note.
- `SPEC/roadmap/shell-dashboard.md`: update AppShell responsibilities, WorkspaceController/Router ownership, responsive/lazy/error-boundary evidence, and obsolete AppShell line references.
- `SPEC/roadmap/agent.md`: update the now-implemented Agent workflow/dialog/task-launch wiring and file ownership.
- `SPEC/roadmap/import.md`: update Import workflow ownership and sequencing evidence.
- `SPEC/roadmap/chat.md`, `lint.md`, `wiki.md`, and `cross-cutting.md`: replace deleted monolith paths with current module paths and update confirmation/task/controller evidence where applicable.

Unrelated feature gaps stay unchanged. Historical audit documents are not converted into living roadmaps.

## Validation

The documentation update is complete only when all of the following hold:

1. No living specification claims the repository is uninitialized.
2. No living specification treats `import_service.rs`, `search_service.rs`, `lint_service.rs`, or `chat_service.rs` as the current implementation file.
3. AppShell is consistently documented as layout/global wiring, not feature workflow ownership.
4. The Import sequence, project-switch guards, secret boundary, unified task path, and lazy/error boundaries match code.
5. Facade, AppState, commands, DTO, persistence, `chat_convenience_service`, and `wiki_index` boundaries are described consistently.
6. `SPEC/DESIGN.md` has no diff.
7. Two independent reviewers inspect intent/consistency and fresh-context accuracy.
8. `npm run check`, `git diff --check`, and the final tracked-worktree status succeed.

## Non-Goals

- No runtime code changes.
- No UI redesign or product behavior change.
- No API, DTO, command, persistence, or dependency change.
- No rewrite of historical audit evidence.
- No cleanup of user-owned untracked files.
