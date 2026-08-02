# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

<!-- Impeccable uses this field for the rendered interface language. Distribution remains a cross-platform Tauri desktop app. -->

## Users

LLM Wiki Desktop primarily serves individual knowledge workers who collect local documents, web sources, notes, and media and want to turn them into a durable, explorable Markdown wiki without first learning an Agent CLI or Git.

Secondary users include researchers who need source-grounded synthesis and technical users who already use Claude Code, Codex, OpenClaw, Hermes, or BYOK model providers and want those capabilities inside a controlled local workflow.

## Product Purpose

LLM Wiki Desktop is a local-first Tauri desktop application for importing personal sources, compiling them into a structured Markdown wiki, exploring the result through reading, search, graph, and chat, maintaining its quality, and producing shareable artifacts.

Success means users retain ownership of ordinary Markdown, JSON, and local files while AI-assisted work remains observable, reviewable, recoverable, and useful without an Agent CLI being mandatory.

## Positioning

The product treats a knowledge-base folder as the durable knowledge asset. Import commits readable Sources first; users explicitly start Update Wiki when they want selected Source versions compiled into linked, versioned Wiki pages. Agent CLIs and BYOK providers are replaceable execution routes behind product-defined workflows rather than the product's primary information architecture.

## Operating Context

- A newly created native knowledge base uses `raw/`, `wiki/`, `.app/`, `exports/`, and `skills/` as its source-of-truth layout. Compatible external vaults retain their existing Markdown layout and receive backend-derived roots/capabilities; app-owned compatibility guidance may appear only under `.app/compat/` after explicit enablement.
- With no project open, the full desktop shell remains visible and the center offers exactly New Knowledge Base and Open Existing Knowledge Base.
- Opening a folder starts with a typed, read-only assessment. Healthy native folders open directly; healthy older LLM Wiki and `nashsu/llm_wiki` folders open in compatible Dashboard mode; untrusted Obsidian or recognizable Markdown vaults open restricted; damaged readable state enters recovery. Format, trust, filesystem access, and health remain independent dimensions.
- Users import sources, update the Wiki, inspect health findings, read and edit Markdown, ask source-grounded questions, explore a graph, and generate HTML or report artifacts.
- Long-running work can continue in the background and must expose progress, cancellation, logs, results, and recovery state.
- Projects, tasks, confirmations, and run history remain isolated by project.
- Multiple entry points may launch the same workflow, but one task system owns preparation, execution, queueing, state, and history.

## Capabilities and Constraints

- Content remains Markdown, JSON, and local files; user Wiki content must not move into a database.
- `raw/sources/` is immutable by default. Replacing or deleting original sources requires explicit confirmation.
- API keys and tokens use OS credential storage and never enter project files, logs, or exports.
- React owns presentation and interaction only. Filesystem, Git, Agent processes, secrets, and background tasks go through Tauri IPC and backend services.
- Agent CLI is an enhancement. After a readable Source exists, BYOK must support core AI organization, Wiki update, and Chat flows.
- Time-to-first-value is a committed, readable Source. Compile, Graph, and Chat are follow-on value and must not block that first success.
- Ordinary materials folders are never initialized, moved, renamed, or marked in place; users create a separate knowledge base and import copies while preserving the originals.
- Trust is global application state bound to canonical folder identity. Compatible app-owned guidance lives under `.app/compat/`; root same-name files remain user content.
- Search is local keyword and filter search. Natural-language answers enter Chat or an explicit AI workflow.
- High-risk deletes, overwrites, broad rewrites, conflict merges, source replacement, and high-risk Agent-generated fixes require a Git checkpoint and explicit confirmation.
- Low-risk, conflict-free Wiki updates may apply automatically after a Git checkpoint and must remain inspectable and recoverable.
- The first workflow surface contains three product-defined workflows: Update Wiki, Health Check, and Generate Content.
- First release workflows are manually started, project-scoped, and serialized within a project.
- First release does not include scheduled triggers, user-authored workflows, custom run instructions, or user-authored output templates.

## Brand Commitments

- Product name: LLM Wiki Desktop.
- Voice: direct, precise, calm, and tool-like; avoid hype and unexplained implementation jargon.
- The interface remains visually close to Codex desktop: compact panes, near-monochrome surfaces, hairline borders, restrained teal accent, dense readable controls, and visible task state.
- Inter, JetBrains Mono, and Source Serif Pro remain the established UI, code/path, and reading type families.
- Chinese and English are both first-class interface languages.

## Evidence on Hand

- Product and feature requirements: `SPEC/PRD.md`, `SPEC/SPEC.md`
- Application flows: `SPEC/APP_flow.md`
- Architecture and service boundaries: `SPEC/TECH_STACK.md`, `SPEC/BACKEND_STRUCTURE.md`
- Frontend behavior and visual constraints: `SPEC/FRONTEND_GUIDELINES.md`, `SPEC/DESIGN.md`
- Confirmed Workflows product and interaction authority: `docs/superpowers/specs/2026-07-30-workflows-panel-redesign.md`
- Confirmed first-run, project-open, trust and recovery authority: `docs/superpowers/specs/2026-07-30-first-run-project-open-workbench-design.md`
- Confirmed Import, Source and media-processing authority: `docs/superpowers/specs/2026-07-24-import-source-media-flow-design.md`
- Incumbent visual reference: `UI-Frontend-design/`
- Legacy implementation baseline pending Workflows migration: `src/features/agent/AgentView.tsx`, `src/features/agent/AgentRightPanel.tsx`, `src/features/agent/RunAgentDialog.tsx`

Future work must not fabricate customer claims, usage benchmarks, model costs, task-duration guarantees, or unsupported Agent capabilities.

## Product Principles

1. Organize around the user's job, not the execution engine.
2. Keep local files authoritative and make every write understandable and recoverable.
3. Make long-running work observable through product stages before exposing raw logs.
4. Preserve user control without turning safe routine work into repeated ceremony.
5. Keep advanced execution detail available but subordinate to outcomes.

## Accessibility & Inclusion

- All controls must be keyboard reachable and expose meaningful labels, focus states, and progress semantics.
- Status cannot rely on color alone.
- Layouts and control labels must accommodate both Chinese and English without truncating essential actions.
- Motion must respect reduced-motion preferences.
