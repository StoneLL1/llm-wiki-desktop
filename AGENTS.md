# AGENTS.md

## Project Brief

LLM Wiki Desktop is a local-first, cross-platform Tauri v2 desktop app for turning personal sources into a Markdown wiki with graph, chat, project-scoped Workflows, lint, and HTML export capabilities.

Before implementation, read the relevant docs:

- Product and scope: `SPEC/PRD.md`, `SPEC/SPEC.md`
- App flows: `SPEC/APP_flow.md`
- Tech and architecture: `SPEC/TECH_STACK.md`, `SPEC/BACKEND_STRUCTURE.md`
- Frontend style: `SPEC/FRONTEND_GUIDELINES.md`, `SPEC/DESIGN.md`

## Hard Rules

- Project content stays as Markdown + JSON + local files. Do not introduce a database for user wiki content.
- For a new native knowledge base, use the project folder as the source of truth: `raw/`, `wiki/`, `.app/`, `exports/`, `skills/`. Compatible vaults keep their existing Markdown layout; app-owned guidance lives only under `.app/compat/`.
- Keep `raw/sources/` immutable by default. Replacing or deleting original sources requires explicit confirmation.
- API keys and tokens must use OS credential storage. Never write secrets to project files, logs, or exported artifacts.
- Checkpoint-required file operations need Git checkpoints first: delete, overwrite, batch rewrite, workflow/Agent auto-fix, conflict merge, source replacement.
- Search is local keyword/filter search only. Natural-language answers must enter Chat or an explicit product workflow; Agent / BYOK is the execution route, not the navigation model.
- Agent CLI is an enhancement, not the only path. After a readable Source exists, BYOK API must support core AI organization, Update Wiki, and Chat flows; it is not an Import parser or recovery route.
- Do not silently install or run Agent install commands.
- Long tasks must be cancellable, logged, progress-visible, and safe to run in the background.
- React UI must not own filesystem, Git, Agent process, or secret-storage logic. Use Tauri IPC and backend services.
- A canonical path registered in `ProjectRegistry` is not user trust. External AI/Agent/Skill execution requires a trusted project; mutations require writable access, and commands must revalidate access and any required Git policy in the backend.

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

Import / Source exception: `docs/superpowers/specs/2026-07-24-import-source-media-flow-design.md` is the sole authority for Import and Source product flow, information architecture, state, copy, media actions, login, OCR / ASR, and AI 整理. Legacy `UI-Frontend-design/import*.html` files remain visual-density and structure references only where they do not conflict; do not restore their compile-after-import, Git toggle, or compile-time OCR behavior.

First-run / project-open exception: `docs/superpowers/specs/2026-07-30-first-run-project-open-workbench-design.md` is the sole authority for the no-project workbench, new knowledge-base flow, opening native or compatible knowledge bases, folder assessment, restricted/trusted/read-only modes, compatibility enablement, repair, and the handoff into Import. Legacy launch HTML and completed launch-page plans remain historical visual evidence only where they do not conflict. Do not restore a standalone launch/marketing page, a third “open folder as project” action, in-place initialization of an ordinary materials folder, Agent/BYOK setup on the first screen, or creation that lands on Dashboard.

Workflows exception: `docs/superpowers/specs/2026-07-30-workflows-panel-redesign.md` is the sole authority for the former Agent main surface, Workflows information architecture, built-in workflows, project-scoped queueing, observable pipelines, state, confirmation, copy, and cross-surface launch ownership. Legacy `UI-Frontend-design/agent.html` remains a visual-density reference only where it does not conflict. Do not restore the Agent configuration dashboard, BYOK card grid, four-card launcher, or generic “Run Agent” dialog as the target product.

1. **Page layout & component structure** — `dashboard.html` defines the full shell: left sidebar (3 labeled sections + agent foot), right panel (project info with paths/index/route/tasks), topbar, status bar. Preserve that hierarchy, but the former workflow section label is now `知识处理 / Knowledge Processing`, its Agent item is `工作流 / Workflows` with the Lucide `Workflow` icon, and the existing agent foot remains unchanged.
2. **CSS tokens & visual density** — `assets/app.css` is the canonical style reference. Implement in Tailwind v4 + `src/styles.css`:
   - Font sizes in absolute px: UI body 13px, secondary 12px, muted/mono 11px, micro-labels 10.5px, reading 14–15px. Write `text-[13px]`, not `text-sm`.
   - Component heights: topbar 48px, main header 52px, right panel header 52px, status bar 28px, nav items 30px (small 26px), panel header 44px.
   - Section labels: 10.5px, uppercase, `letter-spacing: 0.08em`, muted.
   - Single token source: `src/styles.css` `:root` mirrors `app.css` `:root` (including `--sp-*` spacing, `--text-inverse`); components reference tokens, never hardcode hex.
3. **Fonts** — Inter (UI), JetBrains Mono (code/paths), Source Serif Pro (reading). Bundled via @fontsource, no CDN.
4. **Interaction & JS** — Sidebar nav `aria-current`, language switch, search shortcut hints from the design HTML should carry into React components.
5. **Icons** — Lucide React, sizes matching the design (nav 16px, file 14px, etc.).

## Safety And UX Boundaries

- Ordinary materials folders must never be initialized or reorganized in place. Compatibility enablement, repair writes, source replacement, destructive edits, conflict merges, and high-risk workflow-generated changes require explicit user confirmation. Low-risk, conflict-free generated changes may apply automatically only after the required Git checkpoint.
- Show what changed, what paths are affected, and whether a Git checkpoint exists.
- Preserve external Markdown edits; never silently overwrite user changes.
- CJK filenames, Unicode paths, Windows/macOS/Linux path styles, and case-sensitivity edge cases are required test concerns.
- The sample `wiki/wiki/` is validation data, not app source code.

## Required Checks

Use checks proportionally to the change:

1. **Documentation-only work** — Markdown/docs, research notes, plans, reviews, and progress/gotchas logging do not require an npm check unless they also change executable configuration or code.
2. **Ordinary development** — For small or localized code changes, run `npm run check:quick`. It covers lint, the production frontend build/import resolution, the console-log scan, and Rust core compilation.
3. **Large or high-risk completion** — Run the full `npm run check` when finishing a feature, cross-layer change, architecture or dependency/build change, broad refactor, release-facing work, or code that affects filesystem mutation, Git safety, secrets, IPC, concurrency, background tasks, or other critical paths. Also run it whenever the user explicitly requests the full gate.
4. If a required check fails because of the scoped change, fix it and rerun that same gate. When the full gate is required, rerun `npm run check` from the beginning after fixes.

Use judgment rather than treating every edit as a release gate. If the project is not initialized or a required script does not exist, report the exact missing file or script instead of pretending it passed.



## Review Workflow

Review effort must be proportional and is only required when executable code changes:

- **Documentation-only work** — Do not launch review subagents for Markdown/docs, research, plans, reviews, progress logs, or gotchas-only changes.
- **Small localized code changes** — Perform a focused review of the changed code. A review subagent is optional when the change is straightforward and low risk.
- **Features, meaningful fixes, cross-layer changes, or high-risk code** — Launch two review subagents:（Single-turn dialogue should only be used once.）
  - Subagent A with shared context: review design intent, logic, consistency, and integration with existing docs.
  - Subagent B with fresh context: review with no assumptions, looking for blind spots, missing tests, and unclear behavior.

Merge applicable review results, fix valid issues, and run the check level required by the change classification above. If subagents are unavailable when a two-review pass is warranted, perform the equivalent reviews manually and say so in the final report.

## Progress And Gotchas Logging

This is mandatory for all agents (main and subagents).

- **`progress.txt`** — Append a record after every important milestone (feature landed, architecture decision, milestone reached, significant fix). Newest on top (reverse chronological). Format: `[YYYY-MM-DD] Module/Task — Summary of what was done — Key decision or open issue`. Only append; never overwrite or edit history.
- **`gotchas.txt`** — Record a single entry whenever an error recurs, is subtle, or is easy to trip over once. Format: `Symptom — Root cause — How to avoid`. When hitting a similar issue later, check here first.
- **Local-only, never tracked** — Both files live under `SPEC/` (root-level copies of the same names are legacy duplicates, also maintained locally) and are intentionally untracked since 2026-08-31: they do not exist in a fresh clone and `.gitignore` excludes them. The public, machine-detail-free extract for outside contributors is `docs/maintainers/troubleshooting.md`. Never commit secrets to either file; keep machine-specific paths and environment details out of anything that will be distilled into the public guide.

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

## graphify

This project has a knowledge graph at graphify-out/ with god nodes, community structure, and cross-file relationships.

When the user types `/graphify`, use the installed graphify skill or instructions before doing anything else.

Rules:
- For codebase questions, first run `graphify query "<question>"` when graphify-out/graph.json exists. Use `graphify path "<A>" "<B>"` for relationships and `graphify explain "<concept>"` for focused concepts. These return a scoped subgraph, usually much smaller than GRAPH_REPORT.md or raw grep output.
- Dirty graphify-out/ files are expected after hooks or incremental updates; dirty graph files are not a reason to skip graphify. Only skip graphify if the task is about stale or incorrect graph output, or the user explicitly says not to use it.
- If graphify-out/wiki/index.md exists, use it for broad navigation instead of raw source browsing.
- Read graphify-out/GRAPH_REPORT.md only for broad architecture review or when query/path/explain do not surface enough context.
- After modifying code, run `graphify update .` to keep the graph current (AST-only, no API cost).
