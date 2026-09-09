---
name: llm-wiki-desktop-context
description: Use when working in the llm-wiki-desktop repository, especially before implementation, review, debugging, planning, documentation updates, or when locating current architecture, ownership boundaries, project-switch guards, facade contracts, progress, gotchas, or verification commands.
---

# LLM Wiki Desktop Context

## Overview

Use this as the project onboarding and anti-drift map for LLM Wiki Desktop. Load the smallest context set that establishes the current contract, then trace the real call path before editing.

## Load Only Relevant Context

- Read `AGENTS.md` and check `git status --short` once; preserve unrelated changes. `AGENTS.md` owns execution, authorization, checks, reviews, and logging. Do not add confirmation gates from this skill.
- For product/behavior changes, read the applicable feature design authority from `AGENTS.md` and the relevant SPEC sections. Section 16 is an implementation snapshot, not authority over later feature designs.
- Read recent `SPEC/progress.txt` records only when history matters. Search `SPEC/gotchas.txt` for the module/symptom when diagnosing an issue. Both files are optional local context; absence is normal in a clone.
- Use [references/project-map.md](references/project-map.md) when locating ownership, contracts, or focused tests. Known-file instruction/prose edits need no architecture tour.
- If docs and code disagree, identify the mismatch and follow the user's request plus the authoritative product contract. Ask only if resolving a material product decision is outside that authorization; do not pause for ordinary implementation drift.

## Current Architecture Guardrails

- Preserve `AppShell -> WorkspaceController -> WorkspaceRouter -> lazy feature views`.
- Keep `AppShell` limited to the desktop frame, pane wiring, global shortcuts, and global controllers/overlays.
- Compose cross-view behavior in focused workflows: `useAiCapabilities`, `useTaskLauncher`, `useImportWorkflow`, `useProviderWorkflow`, and `useAgentWorkflow`. Do not replace them with one giant hook/controller.
- Keep `WorkspaceRouter` responsible only for active-view dispatch. Preserve `React.lazy`, `Suspense`, `ViewErrorBoundary`, and type-only imports across bundle boundaries.
- Guard every asynchronous project-scoped presentation commit with the initiating project key (`projectId + rootPath`) and, where requests can supersede each other, an epoch. Guard view state, drawer, navigation, and toast commits independently.
- Always upsert valid backend task records into the global `taskStore`, including after a project switch. Task facts are the explicit scope-guard exception; stale-project results must not open or take over the current project's drawer.
- Import / Source flow follows the feature design authority in `AGENTS.md`. Import must form a readable Source; Wiki compilation is a separate explicit flow. Do not restore the historical compile-after-import sequence.
- Preserve `commands -> AppState -> stable service facades -> focused use-case modules`. Commands and `AppState` must not depend on private facade submodules.
- Keep the current `ImportV2Service`, `SearchService`, `LintService`, and `ChatService` as stable facades. Keep `ChatConvenienceService` and `WikiIndex` independent.
- Treat command names, typed DTOs, facade construction, command registration, and Markdown/JSON persistence formats as compatibility contracts unless an approved task explicitly changes them.

## Progressive Disclosure

Use the smallest useful context set:

- Product/scope question: read `SPEC/PRD.md` and `SPEC/SPEC.md`.
- Flow or UX behavior: read `SPEC/APP_flow.md` plus the relevant feature code.
- Backend or IPC change: read `SPEC/TECH_STACK.md`, `SPEC/BACKEND_STRUCTURE.md`, then trace the exact command, model/DTO, `AppState` field, facade, private use-case module, registration, persistence, and contract test.
- Frontend workflow change: trace `AppShell`, `WorkspaceController`, the focused workflow, `WorkspaceRouter`, the lazy view, relevant stores, and async guard tests.
- Frontend visual change: read `SPEC/FRONTEND_GUIDELINES.md`, `SPEC/DESIGN.md`, and `src/styles.css`; consult matching `UI-Frontend-design/` files only when available locally, without modifying that folder.
- Current status or prior decisions: read the newest `SPEC/progress.txt` entries and relevant `docs/audits` or `docs/plans`.
- Repeated failure or surprising behavior: search `SPEC/gotchas.txt` before inventing a theory.

## Verification and Completion

Use `AGENTS.md` as the sole check/review authority. Classify by actual scope and risk, not the feature label; do not add a full gate or fixed reviewer count from this skill.

Run focused tests when they validate changed behavior. After the applicable gate passes, deliver; repeat only after relevant changes or new concerns. If a running Tauri app locks Cargo output, prefer an isolated `CARGO_TARGET_DIR` when feasible. Missing optional tools, logs, or graph outputs must not block unrelated work; report genuine verification limitations precisely.
