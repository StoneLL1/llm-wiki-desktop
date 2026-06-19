# AGENTS.md

## Project Brief

LLM Wiki Desktop is a local-first, cross-platform Tauri v2 desktop app for turning personal sources into a Markdown wiki with graph, chat, lint, Agent, and HTML export workflows.

Before implementation, read the relevant docs:

- Product and scope: `PRD.md`, `SPEC.md`
- App flows: `APP_flow.md`
- Tech and architecture: `TECH_STACK.md`, `BACKEND_STRUCTURE.md`
- Frontend style: `FRONTEND_GUIDELINES.md`, `DESIGN.md`

## Hard Rules

- Project content stays as Markdown + JSON + local files. Do not introduce a database for user wiki content.
- Use the project folder as the source of truth: `raw/`, `wiki/`, `.app/`, `exports/`, `skills/`.
- Keep `raw/sources/` immutable by default. Replacing or deleting original sources requires explicit confirmation.
- API keys and tokens must use OS credential storage. Never write secrets to project files, logs, or exported artifacts.
- High-risk file operations need Git checkpoints first: delete, overwrite, batch rewrite, Agent auto-fix, conflict merge, source replacement.
- Search is local keyword/filter search only. Natural-language answers must enter Chat / Agent / BYOK flow.
- Agent CLI is an enhancement, not the only path. BYOK API must support core flows.
- Do not silently install or run Agent install commands.
- Long tasks must be cancellable, logged, progress-visible, and safe to run in the background.
- React UI must not own filesystem, Git, Agent process, or secret-storage logic. Use Tauri IPC and backend services.

## Tech Direction

- Frontend: React 19 + TypeScript + Vite, Tailwind CSS v4, shadcn/ui, Lucide React, Zustand, react-i18next.
- Desktop/backend: Tauri v2 + Rust services.
- Editor: Milkdown / ProseMirror WYSIWYG.
- Graph: sigma.js + graphology + ForceAtlas2 + Louvain-style community detection.
- Markdown rendering: remark-gfm, remark-math, rehype-katex, rehype-highlight.
- URL extraction: Readability.js.
- Backend shape: thin Tauri commands -> typed DTOs -> services -> local files / Git / Agent / LLM / OS secrets.
- Use structured data and typed interfaces. Avoid ad hoc string protocols.

## Frontend Style

Build the app to look and feel very close to Codex desktop:

- Compact desktop shell, left sidebar, central work surface, right context panel, bottom status area.
- Quiet near-monochrome palette: white, near-black, gray, hairline borders, sparse teal accent.
- Dense but readable tool UI. Prefer panes, lists, tables, toolbars, drawers, and log panels over marketing cards.
- No landing-page hero, decorative gradients, bokeh blobs, glossy AI visuals, or nested cards.
- Use Lucide icons for controls and tooltips for icon-only buttons.
- All text must fit in Chinese and English.

## Safety And UX Boundaries

- Normal folder initialization, source replacement, destructive edits, conflict merges, and Agent-generated diffs require explicit user confirmation.
- Show what changed, what paths are affected, and whether a Git checkpoint exists.
- Preserve external Markdown edits; never silently overwrite user changes.
- CJK filenames, Unicode paths, Windows/macOS/Linux path styles, and case-sensitivity edge cases are required test concerns.
- The sample `wiki/wiki/` is validation data, not app source code.

## Required Checks

After every completed task, automatically run all available checks:

1. `npm run test` - all tests must pass.
2. `npm run lint` - code style must pass.
3. Confirm no unintended `console.log` remains.
4. Verify all import paths resolve.
5. If any check fails, fix the issue and rerun all checks from the beginning.

If the project is not initialized yet and a command does not exist, report that clearly instead of pretending it passed.



## Review Workflow

After each feature or meaningful fix:

1. Run lint + tests.
2. Launch two review subagents:
   - Subagent A with shared context: review design intent, logic, consistency, and integration with existing docs.
   - Subagent B with fresh context: review with no assumptions, look for blind spots, missing tests, and unclear behavior.
3. Merge both review results.
4. Fix all valid issues.
5. Rerun all checks.
6. Only then deliver.

If subagents are unavailable in the current environment, perform the two reviews manually and say so in the final report.

## Progress And Gotchas Logging

This is mandatory for all agents (main and subagents).

- **`progress.txt`** — Append a record after every important milestone (feature landed, architecture decision, milestone reached, significant fix). Newest on top (reverse chronological). Format: `[YYYY-MM-DD] Module/Task — Summary of what was done — Key decision or open issue`. Only append; never overwrite or edit history.
- **`gotchas.txt`** — Record a single entry whenever an error recurs, is subtle, or is easy to trip over once. Format: `Symptom — Root cause — How to avoid`. When hitting a similar issue later, check here first.

These rules are mirrored in `CLAUDE.md`.

## Delivery Standard

- Keep changes scoped to the task.
- Do not rewrite product decisions without user approval.
- Cite changed files and verification results in the final response.
- If checks cannot run because the app skeleton is not initialized, state the exact missing file or script.


<claude-mem-context>
# Memory Context

# claude-mem status

This project has no memory yet. The current session will seed it; subsequent sessions will receive auto-injected context for relevant past work.

Memory injection starts on your second session in a project.

`/learn-codebase` is available if the user wants to front-load the entire repo into memory in a single pass (~5 minutes on a typical repo, optional). Otherwise memory builds passively as work happens.

Live activity: http://localhost:37777
How it works: `/how-it-works`

This message disappears once the first observation lands.
</claude-mem-context>