---
name: llm-wiki-desktop-context
description: Use when working in the llm-wiki-desktop project, especially at the start of a new conversation, before implementation, review, debugging, documentation updates, or when locating project docs, current progress, gotchas, build commands, or code ownership.
---

# LLM Wiki Desktop Context

## Overview

Use this as the project onboarding map for LLM Wiki Desktop. Load only the context needed for the current task, then follow the repository instructions and current implementation over older plans.

## First Read

1. Read `AGENTS.md` for active project rules.
2. Read `SPEC/SPEC.md`, especially section 16 (current implementation alignment), for current implementation constraints.
3. Read the newest entries in `SPEC/progress.txt` to understand recent work and open decisions.
4. Search `SPEC/gotchas.txt` before debugging, running Rust tests, touching Chat, Graph, Import, Agent, CSS contracts, or Windows-specific behavior.
5. Read `references/project-map.md` when you need task-specific doc routing, code locations, or verification commands.

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

## Progressive Disclosure

Use the smallest useful context set:

- Product/scope question: read `SPEC/PRD.md` and `SPEC/SPEC.md`.
- Flow or UX behavior: read `SPEC/APP_flow.md` plus the relevant feature code.
- Backend or IPC change: read `SPEC/TECH_STACK.md`, `SPEC/BACKEND_STRUCTURE.md`, then `src-tauri/src/commands`, `services`, `models`, and `errors`.
- Frontend/UI change: read `SPEC/FRONTEND_GUIDELINES.md`, `SPEC/DESIGN.md`, and the matching files in `UI-Frontend-design/`; do not modify `UI-Frontend-design/`.
- Current status or prior decisions: read the newest `SPEC/progress.txt` entries and relevant `docs/audits` or `docs/plans`.
- Repeated failure or surprising behavior: search `SPEC/gotchas.txt` before inventing a theory.

## Verification

After completed work, run all available repository checks. If any check fails, fix the issue and rerun the full check sequence from the beginning:

```powershell
npm run test
npm run lint
npm run build
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features
```

Also scan for unintended `console.log` in `src` and `src-tauri/src`. On Windows, default `cargo test` can fail before tests run with `STATUS_ENTRYPOINT_NOT_FOUND`; use `cargo test --manifest-path src-tauri/Cargo.toml --no-default-features` for service/model/task verification unless specifically validating the GUI-linked runtime.
