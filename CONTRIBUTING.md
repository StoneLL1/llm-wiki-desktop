# Contributing to LLM Wiki Desktop

Thanks for considering a contribution. This project has unusually strict
boundaries because it manages users' local knowledge bases and their AI
credentials — please read this document before opening a PR.

## Getting started

```bash
npm install
npm run dev          # run the app in development
npm run check:quick  # lint + frontend build + Rust core compile (fast loop)
npm run check        # the full gate (run before requesting review)
```

Prerequisites: Node.js 20+, Rust stable, and the
[Tauri v2 platform dependencies](https://v2.tauri.app/start/prerequisites/).

## Hard boundaries

These are locked by the specifications in `SPEC/`. A PR that quietly violates
them will not be merged:

- **Local-first, no database.** Project content is Markdown + JSON + local
  files only. Never introduce a database or remote storage.
- **File transparency.** Wiki pages are plain Markdown that users may edit in
  Obsidian or any editor. Never invent opaque formats for user content.
- **Keys only in OS credential stores.** API keys never go into project
  files, config JSON, logs, or test fixtures.
- **Path safety.** All project file I/O goes through the backend's
  `ProjectContext` validation; frontend paths are never trusted. Handle
  Unicode/CJK names and Windows/macOS/Linux path styles.
- **Git checkpoints before destructive operations** (bulk replace, raw-source
  removal, agent-driven fixes).
- **Execution paths are explicit.** AI features run via the user-selected
  Agent CLI or BYOK provider; never silently fall back to another provider.
- **Long tasks must be cancelable, backgroundable, and report progress.**

Architecture layering (`React UI → Zustand → thin Tauri IPC → Rust services`)
is specified in `SPEC/TECH_STACK.md` §4 and `SPEC/BACKEND_STRUCTURE.md`.
IPC commands stay thin; business logic lives in services.

## Testing expectations

Scale the gate to your change:

- Docs-only changes: no build gate required.
- Small, local code changes: `npm run check:quick`.
- Features, cross-layer changes, release/CI/security-adjacent code: the full
  `npm run check`, plus the focused test files you touched.

Cross-platform behavior matters: tests run on Ubuntu, Windows, and macOS. If
you add filesystem tests, remember that directory iteration order, symlink
permissions, non-UTF-8 filenames, and line endings all differ per platform
(several past CI failures came from exactly this class).

## Generated files

- `src-tauri/gen/schemas/` is committed; regenerate rather than hand-editing,
  and keep it consistent in CI.
- `capabilities/install-catalog.json` and the embedded capability catalog are
  derived from `capabilities/product-manifest.json` by release tooling —
  don't edit the derived files directly.
- `package-lock.json` and `src-tauri/Cargo.lock` are committed for
  reproducibility.

## Commit style

Conventional commits (`feat:`, `fix:`, `docs:`, `test:`, `chore:`, …), one
logical change per commit. Include the "why" in the body for anything
non-obvious.

## Reporting bugs

Open an issue with the app version, platform, what you expected, and what
happened. Do **not** paste API keys, credentials, or private knowledge-base
content into issues. Security reports follow [SECURITY.md](SECURITY.md)
instead.

## AI-assisted contributions

PRs prepared with AI assistance are welcome; the author is accountable for
the change. Describe what was machine-generated and what you verified.
