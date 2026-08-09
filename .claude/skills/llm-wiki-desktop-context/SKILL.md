---
name: llm-wiki-desktop-context
description: Use when working in the llm-wiki-desktop repository, especially before implementation, review, debugging, planning, documentation updates, or when locating current architecture, ownership boundaries, project-switch guards, facade contracts, progress, gotchas, or verification commands.
---

# LLM Wiki Desktop Context

## Overview

Use this as the project onboarding and anti-drift map for LLM Wiki Desktop. Load the smallest context set that establishes the current contract, then trace the real call path before editing.

## First Read

1. Read `AGENTS.md` for active repository rules and required checks.
2. Run `git status --short`; preserve unrelated user changes and untracked files.
3. Read `SPEC/SPEC.md` section 16 for the current implementation alignment.
4. Read the newest entries in `SPEC/progress.txt` for recent work and open decisions.
5. Search `SPEC/gotchas.txt` by module and symptom before debugging or changing a fragile flow.
6. Read `references/project-map.md` for task-specific docs, exact code ownership, contract tests, and focused checks.

## Resolve Conflicting Context

Use this authority order:

1. `AGENTS.md` and approved product/safety constraints.
2. Current-state `SPEC/*.md` sections.
3. Current code and tests for implemented structure and exact symbols.
4. Newest `SPEC/progress.txt` records.
5. Active implementation plans.
6. Historical audits and completed plans.

Do not treat code drift as approval to change product or safety rules. When current-state docs and code disagree, identify the mismatch explicitly before editing either one.

## Project Rules To Keep Active

- Project content remains Markdown, JSON, and local files. Do not introduce a database for user wiki content.
- Treat `raw/`, `wiki/`, `.app/`, `exports/`, and project `skills/` as the source-of-truth model.
- Keep `raw/sources/` immutable by default; replacing or deleting original sources requires explicit confirmation.
- Store API keys and tokens only in OS credential storage. Never write secrets to project files, logs, or exports.
- Require Git checkpoints before high-risk delete, overwrite, batch rewrite, conflict merge, source replacement, Agent auto-fix, or destructive file operations.
- Search is local keyword/filter search only. Natural-language answers go through Chat, Agent, or BYOK flow.
- Agent CLI is optional enhancement. BYOK API must preserve core flows, and Agent install commands must never run silently.
- React UI must not own filesystem, Git, Agent process, or secret-storage logic. Route those through Tauri IPC and Rust services.
- Preserve external Markdown edits. Do not silently overwrite user changes.

## Current Architecture Guardrails

- Preserve `AppShell -> WorkspaceController -> WorkspaceRouter -> lazy feature views`.
- Keep `AppShell` limited to the desktop frame, pane wiring, global shortcuts, and global controllers/overlays.
- Compose cross-view behavior in focused workflows: `useAiCapabilities`, `useTaskLauncher`, `useImportWorkflow`, `useProviderWorkflow`, and `useAgentWorkflow`. Do not replace them with one giant hook/controller.
- Keep `WorkspaceRouter` responsible only for active-view dispatch. Preserve `React.lazy`, `Suspense`, `ViewErrorBoundary`, and type-only imports across bundle boundaries.
- Guard every asynchronous project-scoped presentation commit with the initiating project key (`projectId + rootPath`) and, where requests can supersede each other, an epoch. Guard view state, drawer, navigation, and toast commits independently.
- Always upsert valid backend task records into the global `taskStore`, including after a project switch. Task facts are the explicit scope-guard exception; stale-project results must not open or take over the current project's drawer.
- Preserve Import confirmation order: `confirm_import_preview -> wikiStore.scan -> optional start_wiki_compile`.
- Preserve `commands -> AppState -> stable service facades -> focused use-case modules`. Commands and `AppState` must not depend on private facade submodules.
- Keep `ImportService`, `SearchService`, `LintService`, and `ChatService` as stable facades. Keep `ChatConvenienceService` and `WikiIndex` independent.
- Treat command names, typed DTOs, facade construction, command registration, and Markdown/JSON persistence formats as compatibility contracts unless an approved task explicitly changes them.

## Progressive Disclosure

Use the smallest useful context set:

- Product/scope question: read `SPEC/PRD.md` and `SPEC/SPEC.md`.
- Flow or UX behavior: read `SPEC/APP_flow.md` plus the relevant feature code.
- Backend or IPC change: read `SPEC/TECH_STACK.md`, `SPEC/BACKEND_STRUCTURE.md`, then trace the exact command, model/DTO, `AppState` field, facade, private use-case module, registration, persistence, and contract test.
- Frontend workflow change: trace `AppShell`, `WorkspaceController`, the focused workflow, `WorkspaceRouter`, the lazy view, relevant stores, and async guard tests.
- Frontend visual change: read `SPEC/FRONTEND_GUIDELINES.md`, `SPEC/DESIGN.md`, and the matching files in `UI-Frontend-design/`; do not modify `UI-Frontend-design/`.
- Current status or prior decisions: read the newest `SPEC/progress.txt` entries and relevant `docs/audits` or `docs/plans`.
- Repeated failure or surprising behavior: search `SPEC/gotchas.txt` before inventing a theory.

## Verification

After completed work, run the repository's single authoritative full check from the repository root:

```powershell
npm run check
```

This command owns frontend tests, lint, TypeScript/Vite build, console-log scan, Tauri GUI Cargo check, and Rust no-default-features tests. Focused tests are useful during development but never replace it. If any stage fails, fix the issue and rerun `npm run check` from the beginning. If a running Tauri app locks the default Cargo target, keep the app running and use a separate `CARGO_TARGET_DIR` for the complete rerun.
