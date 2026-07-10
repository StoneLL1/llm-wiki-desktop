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

### Design alignment (authoritative source: `UI-Frontend-design/` folder)

The entire `UI-Frontend-design/` folder is the design spec — not just `app.css`, but HTML structure, page components, and JS behavior. Do not treat it as app source; do not modify or commit it. Before any UI work, consult:

1. **Page layout & component structure** — `dashboard.html` defines the full shell: left sidebar (3 labeled sections + agent foot), right panel (project info with paths/index/route/tasks), topbar, status bar. Match DOM hierarchy, section labels, and aria roles.
2. **CSS tokens & visual density** — `assets/app.css` is the canonical style reference. Implement in Tailwind v4 + `src/styles.css`:
   - Font sizes in absolute px: UI body 13px, secondary 12px, muted/mono 11px, micro-labels 10.5px, reading 14–15px. Write `text-[13px]`, not `text-sm`.
   - Component heights: topbar 48px, main header 52px, right panel header 52px, status bar 28px, nav items 30px (small 26px), panel header 44px.
   - Section labels: 10.5px, uppercase, `letter-spacing: 0.08em`, muted.
   - Single token source: `src/styles.css` `:root` mirrors `app.css` `:root` (including `--sp-*` spacing, `--text-inverse`); components reference tokens, never hardcode hex.
3. **Fonts** — Inter (UI), JetBrains Mono (code/paths), Source Serif Pro (reading). Bundled via @fontsource, no CDN.
4. **Interaction & JS** — Sidebar nav `aria-current`, language switch, search shortcut hints from the design HTML should carry into React components.
5. **Icons** — Lucide React, sizes matching the design (nav 16px, file 14px, etc.).

## Safety And UX Boundaries

- Normal folder initialization, source replacement, destructive edits, conflict merges, and Agent-generated diffs require explicit user confirmation.
- Show what changed, what paths are affected, and whether a Git checkpoint exists.
- Preserve external Markdown edits; never silently overwrite user changes.
- CJK filenames, Unicode paths, Windows/macOS/Linux path styles, and case-sensitivity edge cases are required test concerns.
- The sample `wiki/wiki/` is validation data, not app source code.

## Required Checks

After every completed task, automatically run the unified local check:

1. `npm run check` - runs tests, lint, build/import resolution, console-log scan, Tauri GUI Rust compile, and Rust no-default-features tests.
2. If the check fails, fix the issue and rerun `npm run check` from the beginning.

If the project is not initialized yet or `npm run check` does not exist, report the exact missing file or script clearly instead of pretending it passed.



## Review Workflow

After each feature or meaningful fix:

1. Run `npm run check`.
2. Launch two review subagents:
   - Subagent A with shared context: review design intent, logic, consistency, and integration with existing docs.
   - Subagent B with fresh context: review with no assumptions, look for blind spots, missing tests, and unclear behavior.
3. Merge both review results.
4. Fix all valid issues.
5. Rerun `npm run check`.
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
