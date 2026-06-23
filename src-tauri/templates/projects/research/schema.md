# Schema

> Page types and rules for a research wiki.

## Page Types

| Type | Folder | Purpose |
|---|---|---|
| entity | `wiki/entities/` | Authors, labs, datasets, instruments |
| concept | `wiki/concepts/` | Terms, methods, and theoretical constructs |
| source | `wiki/sources/` | One paper, dataset, or reference (primary citation) — the verbatim imported original (import-owned) |
| query | `wiki/queries/` | Research questions with cited answers |
| synthesis | `wiki/synthesis/` | Findings aggregated across sources |
| comparison | `wiki/comparisons/` | Competing methods, models, or results |

## Page Rules

- Every claim page must link the `source` pages that back it.
- Frontmatter `source` field records the original citation (URL, DOI, or path).
- `wiki/synthesis/` pages cite every source they draw from.
- `wiki/sources/` holds verbatim imported originals (the extracted text of each paper/dataset) and is import-owned: derived pages cite these sources via frontmatter `sources: ["<filename>"]` and a `> Sources:` line, but never create, modify, or summarize a page under `wiki/sources/`.
