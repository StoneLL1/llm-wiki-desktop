# Schema

> Page types and rules for a personal-growth wiki.

## Page Types

| Type | Folder | Purpose |
|---|---|---|
| entity | `wiki/entities/` | Mentors, authors, communities |
| concept | `wiki/concepts/` | Principles, frameworks, and habits |
| source | `wiki/sources/` | Books, talks, and articles that shaped a view — the verbatim imported original (import-owned) |
| query | `wiki/queries/` | Reflective questions and honest answers |
| synthesis | `wiki/synthesis/` | Themes and patterns across a period of growth |
| comparison | `wiki/comparisons/` | Competing approaches to the same challenge |

## Page Rules

- Date reflective entries in frontmatter so progress is traceable.
- Keep principle pages short and actionable.
- Link setbacks and lessons together with `[[wikilinks]]`.
- `wiki/log.md` records meaningful milestones.
- `wiki/sources/` holds verbatim imported originals and is import-owned: derived pages cite these sources via frontmatter `sources: ["<filename>"]` and a `> Sources:` line, but never create, modify, or summarize a page under `wiki/sources/`.
