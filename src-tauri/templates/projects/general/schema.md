# Schema

> Defines the page types and structure rules for this wiki.
> Templates only affect `purpose.md` and `schema.md` — not the folder layout.

## Page Types

| Type | Folder | Purpose |
|---|---|---|
| entity | `wiki/entities/` | Named people, organizations, products, or things |
| concept | `wiki/concepts/` | Ideas, definitions, and mental models |
| source | `wiki/sources/` | A single imported reference (article, doc, page) — the verbatim imported original (import-owned) |
| query | `wiki/queries/` | Saved questions and their cited answers |
| synthesis | `wiki/synthesis/` | Multi-source summaries around a theme |
| comparison | `wiki/comparisons/` | Side-by-side analysis of two or more subjects |

## Page Rules

- Each page is a Markdown file with optional YAML frontmatter (`type`, `tags`, `source`).
- Link related pages with `[[wikilinks]]`.
- Keep one topic per page.
- `wiki/index.md` lists the most important entry points.
- `wiki/overview.md` summarizes the whole knowledge base.
- `wiki/sources/` holds verbatim imported originals and is import-owned: derived pages (`entities`, `concepts`, `synthesis`, `comparisons`) cite these sources via frontmatter `sources: ["<filename>"]` and a `> Sources:` line, but never create, modify, or summarize a page under `wiki/sources/`.
