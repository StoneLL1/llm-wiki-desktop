## What

<!-- One-paragraph summary of the change. -->

## Why

<!-- The problem or motivation. Link issues as "Fixes #123" where applicable. -->

## Notes for review

- Spec/design references (`SPEC/…`, `docs/superpowers/specs/…`) that this change follows or updates.
- Cross-platform considerations (Windows / macOS / Linux behavior, CJK paths, line endings).
- Which gates you ran: `npm run check:quick` / full `npm run check` / focused tests.

## Checklist

- [ ] Commit messages follow the conventional-commit style.
- [ ] No secrets, API keys, absolute personal paths, or private knowledge-base content are included.
- [ ] New filesystem behavior was considered across platforms (iteration order, symlinks, line endings, non-UTF-8 names).
- [ ] Generated files (`src-tauri/gen/schemas/`, capability catalogs) were regenerated, not hand-edited.
