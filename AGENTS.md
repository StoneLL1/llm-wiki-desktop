# AGENTS.md

## Project Overview

LLM Wiki Desktop is a local-first Tauri v2 app that turns personal sources into a Markdown wiki with graph, chat, Workflows, lint, and HTML export. The main stack is React + TypeScript + Tailwind, with thin typed Tauri commands calling Rust services.

Within platform instructions and permissions, follow the user's current request and existing authorization. Complete scoped work with proportionate verification; ask only about material unresolved choices. Preserve unrelated edits. Routine repository code/document edits do not require additional confirmation or a Git checkpoint. This file owns repository check/review policy; `CLAUDE.md` is a compatibility entrypoint and Skills supply task-specific methods.

## Core Boundaries

The following govern application behavior and real knowledge-base data, not extra approval steps for repository development:

- **Local files:** User content stays Markdown + JSON + local files, with no content database. Native projects use `raw/`, `wiki/`, `.app/`, `exports/`, and `skills/`; compatible vaults retain their Markdown layout, with app guidance only under `.app/compat/`. Never initialize or reorganize an ordinary materials folder in place.
- **Data safety:** Preserve original `raw/sources/` and external Markdown edits. Source replacement/deletion, compatibility enablement, repair writes, destructive edits, conflict merges, and high-risk generated changes require explicit user confirmation. Checkpoint-required operations (delete, overwrite, batch rewrite, workflow/Agent auto-fix, conflict merge, source replacement) need a Git checkpoint first. Update Wiki manages private operation refs and a temporary index automatically, including recovery of interrupted writes; it does not require a clean worktree or change the user’s HEAD/index. Low-risk conflict-free generated changes may apply after the required checkpoint. Show affected paths, changes, and checkpoint status.
- **Secrets and access:** Store keys/tokens only in OS credentials, never project files, logs, or exports. Backend commands revalidate project trust, writable access, paths, and required Git policy. Registry membership is not trust; external AI/Agent/Skill execution requires trust.
- **Product flow:** Search is local keyword/filter search. Natural-language answers enter Chat or an explicit workflow. Import produces a readable Source; Wiki compilation is a separate explicit flow. Agent/BYOK are execution routes, not navigation models. BYOK supports core AI organization, Update Wiki, and Chat after Source creation; it is not an Import parser or recovery route. Do not silently switch routes or install Agents.
- **Ownership and tasks:** Filesystem, Git, Agent processes, and secrets belong in Rust services behind typed IPC, not React. Long application tasks must be cancellable, observable, logged, and safe in the background.
- **Validation data:** Test on disposable copies of [sample knowledge bases](docs/testing/sample-knowledge-base.md), never in place. For path-related changes, cover relevant CJK/Unicode, OS path styles, and case-sensitivity behavior.

## Documentation and Design

Read only the relevant sections. The feature authorities below take precedence over general specs, legacy HTML behavior, and historical plans; implementation drift does not change product decisions.

| Topic | Source |
| --- | --- |
| Product and scope | [PRD](SPEC/PRD.md), [Specification](SPEC/SPEC.md) |
| General app flows | [App flow](SPEC/APP_flow.md) |
| Architecture and technology | [Tech stack](SPEC/TECH_STACK.md), [Backend structure](SPEC/BACKEND_STRUCTURE.md) |
| Frontend implementation and visual details | [Frontend guidelines](SPEC/FRONTEND_GUIDELINES.md); [Design](SPEC/DESIGN.md) supplies visual tone |
| Import / Source authority | [Import and Source design](docs/superpowers/specs/2026-07-24-import-source-media-flow-design.md) |
| First-run / project-open authority | [Project-open workbench design](docs/superpowers/specs/2026-07-30-first-run-project-open-workbench-design.md) |
| Workflows authority | [Workflows panel design](docs/superpowers/specs/2026-07-30-workflows-panel-redesign.md) |

Keep the compact Codex-like desktop shell, quiet near-monochrome palette, dense panes/lists/toolbars, Lucide controls, and Chinese/English fit. Use the frontend guidelines for typography, dimensions, fonts, icons, and interactions; components use `src/styles.css` tokens rather than hardcoded values.

`UI-Frontend-design/`, when available locally, is a read-only design reference outside version control, not app source: do not modify or commit it. Consult relevant HTML structure, behavior, and `assets/app.css` tokens for UI work when available, subject to the feature authorities above. Avoid marketing heroes, decorative gradients, and nested cards.

Use an existing graph and working graphify CLI for useful relationship navigation; otherwise continue with `rg` and source reads. After code changes, update an existing graph once if the tool is available; graph maintenance must not block delivery. Detailed modes belong in the graphify Skill.

## Verification and Review

Choose checks by actual scope and risk, including for new features:

| Change | Required verification |
| --- | --- |
| Documentation/instruction prose only | Relevant consistency/link checks; no npm gate or code-review subagents |
| Localized code with limited impact | `npm run check:quick`; focused behavior tests where useful |
| Cross-layer, broad architecture/refactor, dependency/build, release-facing, or critical behavior changes | `npm run check` |

Critical behavior includes filesystem mutation, Git safety, secrets, IPC, concurrency, and background tasks. Run the full gate when the user requests it. Fix scoped failures and rerun the applicable gate; when the full gate is required, rerun it from the beginning after fixes. Reuse passing results while relevant code/configuration is unchanged. For environmental or unrelated failures, identify the cause, complete available verification, and report the unmet check without expanding into unrelated repairs or retrying indefinitely.

The main agent reviews changed code. Use independent reviewers when another perspective materially improves confidence, especially for high-risk or complex cross-layer changes; choose the number and context as needed. If unavailable, review manually. Resolve supported findings and follow up only on affected changes; do not repeat complete reviews for logs, wording changes, or already resolved issues.

## Delivery and Records

- Deliver the requested outcome with changed-file links, relevant verification, and material limitations. Finish when the applicable checks pass and identified substantive issues are resolved; optional improvements do not delay delivery. If blocked, complete independent work and state exactly what remains and what is needed.
- Record durable milestones in `SPEC/progress.txt`: prepend `[YYYY-MM-DD] Module/Task — Summary — Decision or open issue`, preserving history. Add nonduplicate reusable lessons to `SPEC/gotchas.txt`: `Symptom — Root cause — How to avoid`. The main agent owns shared log writes; routine reads/checks need no entry.
- Both logs are local-only and Git-ignored; skip them if absent, creating them only when there is something to record. Do not maintain legacy duplicates or store secrets. Public guidance belongs in [maintainer troubleshooting](docs/maintainers/troubleshooting.md).
