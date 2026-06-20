---
name: wiki-ingest
description: Compile extracted local sources into the project's Markdown wiki.
---

# Wiki Ingest

Read `purpose.md`, `schema.md`, `raw/extracted/`, and the existing `wiki/` tree.

- Work only inside the supplied compile workspace.
- Preserve existing manual content unless the source or schema requires an update.
- Write only Markdown files below `wiki/`.
- Maintain `wiki/index.md`, `wiki/overview.md`, and append a concise entry to `wiki/log.md`.
- Use project-relative Markdown links and `[[wikilinks]]` where useful.
- Do not delete files. If a page appears obsolete, record that in `wiki/log.md` for user review.
