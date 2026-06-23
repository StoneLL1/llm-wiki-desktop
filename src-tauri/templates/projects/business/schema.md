# Schema

> Page types and rules for a business knowledge base.

## Page Types

| Type | Folder | Purpose |
|---|---|---|
| entity | `wiki/entities/` | Companies, people, products, partners |
| concept | `wiki/concepts/` | Market terms, frameworks, and metrics |
| source | `wiki/sources/` | Articles, reports, and internal documents — the verbatim imported original (import-owned) |
| query | `wiki/queries/` | Business questions with cited answers |
| synthesis | `wiki/synthesis/` | Market themes and analyses |
| comparison | `wiki/comparisons/` | Competitive comparisons and tradeoffs |

## Page Rules

- Entity pages record the role (customer, competitor, partner, vendor) in frontmatter.
- Cite market data with a `source` link; never leave a number unsupported.
- Keep decision records dated and linked to the entities they affect.
- Confidential notes stay local; this wiki never leaves the project folder.
- `wiki/sources/` holds verbatim imported originals and is import-owned: derived pages cite these sources via frontmatter `sources: ["<filename>"]` and a `> Sources:` line, but never create, modify, or summarize a page under `wiki/sources/`.
