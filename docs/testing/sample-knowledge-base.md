# Sample knowledge base (external)

This repository ships **without** the sample knowledge base. The sample is a
real, Obsidian-compatible vault of several hundred Markdown pages that the
maintainer uses to validate realistic scale, Obsidian compatibility, and graph
performance. It was removed from version control (owner decision, 2026-09-01)
for two reasons:

1. **Size** — the vault plus its app state made up hundreds of tracked files
   and a significant share of repository history.
2. **Privacy** — a personal vault can accumulate private content and local
   machine paths that do not belong in a public repository.

## What this repository defines instead

- The native knowledge-base layout (purpose/schema, `raw/`, `wiki/`, `.app/`,
  `exports/`, `skills/`) is specified by `SPEC/SPEC.md` §5.
- Compatible vaults keep their existing Markdown layout; the backend's
  `ProjectContext.layout` discovers page and source roots at runtime.

## Working without the sample

- **CI**: `src-tauri/tests/mvp_flow.rs` copies a slice of the sample into a
  temporary project only when a local copy exists; on a fresh clone the
  sample-wiki loop skips itself. No test requires the vault.
- **Local validation at scale**: maintainers keep the vault outside the
  repository (verified backup, 2026-09-01) and open a **copy** of it from the
  app — never in place — following the same rules as any external project.
- **Building your own sample**: create a native knowledge base via the app or
  from `SPEC/SPEC.md` §5, or point the app at a copy of any Obsidian vault.
