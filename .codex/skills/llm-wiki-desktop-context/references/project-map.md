# Project Map

Use this reference only after `llm-wiki-desktop-context/SKILL.md` triggers and the current task needs more than the quick map.

## Document Router

| Need | Read |
| --- | --- |
| Product intent, MVP scope, non-goals | `SPEC/PRD.md`, `SPEC/SPEC.md` |
| Current implementation constraints | `SPEC/SPEC.md` section 16 |
| App flows, state files, async guards, failure paths | `SPEC/APP_flow.md` |
| Tech stack and layering | `SPEC/TECH_STACK.md` |
| Tauri backend architecture | `SPEC/BACKEND_STRUCTURE.md` |
| UI density, tokens, layout, interaction | `SPEC/FRONTEND_GUIDELINES.md`, `SPEC/DESIGN.md`, `UI-Frontend-design/dashboard.html`, `UI-Frontend-design/assets/app.css` |
| Recent progress and open decisions | newest entries in `SPEC/progress.txt` |
| Recurring pitfalls | `SPEC/gotchas.txt`, searched by symptom/module |
| Active implementation plans | `docs/plans/`, `docs/superpowers/plans/` (verify status against code and progress) |
| Historical evidence | `docs/audits/` and completed plans; never use as current-state authority without verification |
| App skill templates shipped to wiki projects | `src-tauri/templates/skills/` |

## Code Router

| Area | Frontend | Backend |
| --- | --- | --- |
| Shell/navigation/status panes | `src/components/app/AppShell.tsx`, `WorkspaceController.tsx`, `WorkspaceRouter.tsx`, `ProjectConfirmationController.tsx`; `src/stores/navigationStore.ts` | project/settings commands and services |
| Cross-view AI/task orchestration | `src/hooks/useAiCapabilities.ts`, `src/hooks/useTaskLauncher.ts`, `src/features/agent/useAgentWorkflow.ts`, `src/features/settings/useProviderWorkflow.ts` | task/agent/LLM/settings commands and stable services |
| UI primitives/styles | `src/components/ui`, `src/styles.css`, `src/test/ui-css-contracts.test.ts` | N/A |
| Wiki read/edit | `src/features/wiki`, `src/features/wiki/wikiStore.ts` | `wiki_commands`, `models/wiki.rs`, `file_store`, `wiki_index` |
| Chat/page Ask AI | `src/features/chat`, `src/stores/chatStore.ts` | `chat_service`, chat commands/models |
| Search/index/retrieval | `src/components/app/TopBar.tsx`, `src/types/wiki.ts`, feature consumers | `search_commands`, `search_service`, `wiki_index` |
| Graph | `src/features/graph` | `graph_service`, graph models/cache |
| Import / Source | `src/features/import/`, `src/stores/importStore.ts`, `src/features/wiki/{sourceStore.ts,SourceRightPanel.tsx,SourceLifecycleDialogs.tsx}` | `commands/import_v2*_commands.rs`, `commands/source_commands.rs`, `models/import_v2*.rs`, `models/source*.rs`, `services/import_v2/`; Compile is a separate explicit flow, while `compile_legacy_adapter.rs` is read-only compatibility |
| Agent/BYOK/task flows | `src/features/agent`, task UI/stores | `agent_service`, `llm_service`, `task_service`, `git_service` |
| Exports/HTML skills | `src/features/exports` | `export_service`, `src-tauri/templates/skills` |
| Lint | `src/features/lint` | `commands/lint_commands.rs`, `services/lint_service/` |

## Contract And Registration Router

| Contract | Primary locations |
| --- | --- |
| Project trust boundary | `src-tauri/src/app_state.rs`, `ProjectRegistry`, `AppState::resolve_project_context` |
| Stable service exports | `src-tauri/src/services/mod.rs`; facade `mod.rs` files under `import_service/`, `search_service/`, `lint_service/`, `chat_service/` |
| Command DTO and thin IPC | `src-tauri/src/commands/`, `src-tauri/src/models/` |
| Command registration | `src-tauri/src/lib.rs` |
| Facade compatibility | `src-tauri/tests/service_facade_contracts.rs` |
| Task facts and events | `src/stores/taskStore.ts`, `src/hooks/useTaskEvents.ts`, `src/hooks/useTaskLauncher.ts`, `src-tauri/src/tasks/` |
| Project-switch invalidation | focused workflow tests, `src/stores/projectScope.ts`, store reset helpers |
| Bundle boundaries | `src/components/app/WorkspaceRouter.tsx`, `src/test/app-shell-architecture.test.ts` |

## Current Implementation Anchors

- Tauri v2 + React 19 + TypeScript + Vite + Tailwind CSS v4 are already initialized.
- Frontend call flow is `AppShell -> WorkspaceController -> WorkspaceRouter -> lazy feature views`; global confirmation, task drawer, and toast controllers remain mounted at shell level.
- Cross-view workflows remain focused and separate. Each project-scoped presentation commit must validate the initiating project key; supersedable requests also use an epoch. Valid backend task records are always upserted globally, while stale-project drawer, navigation, toast, and view-state commits are suppressed.
- Backend call flow is `commands -> AppState -> stable service facades -> focused use-case modules` over typed DTOs.
- `ImportV2Service`, `SearchService`, `LintService`, and `ChatService` are stable facade boundaries. Import capability/runtime and connector-session services remain typed Import V2 collaborators; `ChatConvenienceService` and `WikiIndex` remain independent.
- Chat sessions persist as `.app/chats/{id}.json`; Wiki page side Chat uses optional `contextPagePath` and must guard fast page switches.
- Retrieval diagnostics can include search hits, graph neighbors, and source overlap; saved citations should reflect model-used `[S#]` references.
- Search/wiki scanning uses local indexes and local files, not a database.
- Graph visual scale is centralized in `src/features/graph/graphVisualScale.ts`; keep graph changes testable.
- Export list path display uses `src/lib/pathDisplay.ts`; show basename inline and preserve full path in title/tooltip where implemented.
- The sample knowledge base is maintained outside the repository (see `docs/testing/sample-knowledge-base.md`); it is validation data, not app source. `.app` state inside samples may be useful but must be checked for private content before commit.

## Build And Verification

Use this authoritative full check from the repository root:

```powershell
npm run check
```

If any stage fails, fix it and rerun `npm run check` from the beginning. The script already includes frontend tests, lint, build/import resolution, console-log scan, Tauri GUI Cargo check, and Rust no-default-features tests.

Useful targeted commands:

```powershell
npm run test -- src/features/chat/PageChatPanel.test.tsx
npm run test -- src/test/ui-css-contracts.test.ts
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features services::chat_service
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --test service_facade_contracts
```

`rg.exe` may be blocked in this Windows environment. If it fails with access denied, use:

```powershell
Get-ChildItem -Recurse -File src,src-tauri/src | Select-String -Pattern "needle"
```

Focused commands accelerate iteration; they do not satisfy final verification.

## Before Editing Checklist

- Check `git status --short`; assume unrelated dirty files belong to the user.
- Search `SPEC/gotchas.txt` for the feature/module and exact error text.
- Read only the docs needed for the task, but include `AGENTS.md` and current SPEC alignment when behavior may affect product constraints.
- For UI work, compare against `UI-Frontend-design/` structure and CSS tokens without modifying that folder.
- For filesystem, Git, Agent, secret, task, or source operations, keep logic in backend services and expose typed IPC.
- For workflow changes, list every `await` and every following UI commit; verify the project key/epoch before state, drawer, navigation, or toast changes.
- For backend facade changes, trace command -> DTO -> `AppState` -> facade -> private module -> persistence -> registration -> contract test before editing.
- Add focused tests for changed behavior, then broaden checks when shared services or UI contracts are touched.
