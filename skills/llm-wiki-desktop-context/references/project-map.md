# Project Map

Use this reference only after `llm-wiki-desktop-context/SKILL.md` triggers and the current task needs more than the quick map.

## Document Router

| Need | Read |
| --- | --- |
| Product intent, MVP scope, non-goals | `SPEC/PRD.md`, `SPEC/SPEC.md` |
| Current implementation constraints | `SPEC/SPEC.md` section 16 |
| App flows, state files, failure paths | `SPEC/APP_flow.md` |
| Tech stack and layering | `SPEC/TECH_STACK.md` |
| Tauri backend architecture | `SPEC/BACKEND_STRUCTURE.md` |
| UI density, tokens, layout, interaction | `SPEC/FRONTEND_GUIDELINES.md`, `SPEC/DESIGN.md`, `UI-Frontend-design/dashboard.html`, `UI-Frontend-design/assets/app.css` |
| Recent progress and open decisions | newest entries in `SPEC/progress.txt` |
| Recurring pitfalls | `SPEC/gotchas.txt`, searched by symptom/module |
| Audit/implementation plans | `docs/audits/`, `docs/plans/`, `docs/superpowers/plans/` |
| App skill templates shipped to wiki projects | `src-tauri/templates/skills/` |

## Code Router

| Area | Frontend | Backend |
| --- | --- | --- |
| Shell/navigation/status panes | `src/components/app`, `src/stores/navigationStore.ts`, `src/stores/settingsStore.ts` | project/settings commands and services |
| UI primitives/styles | `src/components/ui`, `src/styles.css`, `src/test/ui-css-contracts.test.ts` | N/A |
| Wiki read/edit | `src/features/wiki`, `src/features/wiki/wikiStore.ts` | `wiki_commands`, `models/wiki.rs`, `file_store`, `wiki_index` |
| Chat/page Ask AI | `src/features/chat`, `src/stores/chatStore.ts` | `chat_service`, chat commands/models |
| Search/index/retrieval | `src/components/app/TopBar.tsx`, `src/types/wiki.ts`, feature consumers | `search_commands`, `search_service`, `wiki_index` |
| Graph | `src/features/graph` | `graph_service`, graph models/cache |
| Import/extraction | `src/features/import` | `import_service`, `extraction_service`, source models |
| Agent/BYOK/task flows | `src/features/agent`, task UI/stores | `agent_service`, `llm_service`, `task_service`, `git_service` |
| Exports/HTML skills | `src/features/exports` | `export_service`, `src-tauri/templates/skills` |
| Lint | `src/features/lint` | `lint_service` |

## Current Implementation Anchors

- Tauri v2 + React 19 + TypeScript + Vite + Tailwind CSS v4 are already initialized.
- Frontend is organized by shell components, UI components, feature views, hooks, stores, services, types, tests, and i18n.
- Backend keeps thin Tauri commands over typed DTOs and service-layer logic in `src-tauri/src`.
- Chat sessions persist as `.app/chats/{id}.json`; Wiki page side Chat uses optional `contextPagePath` and must guard fast page switches.
- Retrieval diagnostics can include search hits, graph neighbors, and source overlap; saved citations should reflect model-used `[S#]` references.
- Search/wiki scanning uses local indexes and local files, not a database.
- Graph visual scale is centralized in `src/features/graph/graphVisualScale.ts`; keep graph changes testable.
- Export list path display uses `src/lib/pathDisplay.ts`; show basename inline and preserve full path in title/tooltip where implemented.
- Sample `wiki/wiki/` is validation data, not app source. `.app` state inside samples may be useful but must be checked for private content before commit.

## Build And Verification

Use these from the repository root:

```powershell
npm run test
npm run lint
npm run build
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features
```

Useful targeted commands:

```powershell
npm run test -- src/features/chat/PageChatPanel.test.tsx
npm run test -- src/test/ui-css-contracts.test.ts
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features services::chat_service
```

`rg.exe` may be blocked in this Windows environment. If it fails with access denied, use:

```powershell
Get-ChildItem -Recurse -File src,src-tauri/src | Select-String -Pattern "needle"
```

For console-log checks:

```powershell
Get-ChildItem -Recurse -File src,src-tauri/src |
  Where-Object { $_.FullName -notlike "*\target\*" } |
  Select-String -Pattern "console\.log"
```

## Before Editing Checklist

- Check `git status --short`; assume unrelated dirty files belong to the user.
- Search `SPEC/gotchas.txt` for the feature/module and exact error text.
- Read only the docs needed for the task, but include `AGENTS.md` and current SPEC alignment when behavior may affect product constraints.
- For UI work, compare against `UI-Frontend-design/` structure and CSS tokens without modifying that folder.
- For filesystem, Git, Agent, secret, task, or source operations, keep logic in backend services and expose typed IPC.
- Add focused tests for changed behavior, then broaden checks when shared services or UI contracts are touched.
