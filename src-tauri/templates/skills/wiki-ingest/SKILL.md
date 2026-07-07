---
name: wiki-ingest
description: Compile the project's Markdown wiki from the imported sources into derived concept/entity pages.
---

# Wiki Ingest

Read `purpose.md`, `schema.md`, the imported originals in `wiki/sources/` (and legacy `raw/extracted/` if present), and the existing `wiki/` tree.

## What sources are

`wiki/sources/` holds the **verbatim extracted originals** of every imported source (Markdown imported as-is; PDF/DOCX shown as their Markdown conversion). These pages are **import-owned**. Treat them as authoritative input: you read and cite them, you never recreate them.

- **Never create, modify, or delete any file under `wiki/sources/`.** This is a hard rule. The application rejects any compile output that touches this subtree.
- Do **not** write one page per source. Do **not** summarize a source into another folder. The original already exists and is browsable.

## What you generate

Build only **derived** pages that synthesize **across** sources:

- `wiki/entities/` — named people, organizations, products, things.
- `wiki/concepts/` — ideas, definitions, mental models.
- `wiki/synthesis/` — themes drawn from multiple sources.
- `wiki/comparisons/` — side-by-side analysis of two or more subjects.

Name each derived page after the **concept** it covers, never after a source filename.

## Decision Rules

- **create** when the sources introduce a genuinely new concept, entity, synthesis, or comparison that is not already covered by an existing derived page.
- **update** when new evidence materially changes, corrects, or extends an existing page.
- **merge** when a new source has the same core thesis as an existing derived page; fold the evidence into that page instead of fragmenting the wiki.
- **see-also** when content spans related but distinct topics; keep the pages separate and add cross-links both ways where useful.
- **conflict** when sources disagree; annotate the disagreement with source-specific evidence instead of smoothing it into one unsupported claim.
- **Cascade** after material changes: scan linked pages, overlapping source pages, `wiki/index.md`, and `wiki/overview.md`; update affected pages before appending the `wiki/log.md` entry.

## Cite sources (two ways, on every derived page)

1. Frontmatter `sources: ["<original-source-filename>"]` — the machine join key the graph uses. List every original the page draws from.
2. A human-readable `> Sources:` line near the top or bottom, with Markdown links to the originals (e.g. `../sources/my-article.md`) or `[[sources/my-article]]`.

## Process

- Work only inside the supplied compile workspace; write only Markdown under `wiki/` (never under `wiki/sources/`).
- Preserve existing manual content unless a source or the schema requires an update.
- Maintain `wiki/index.md`, `wiki/overview.md`, and append a concise entry to `wiki/log.md`.
- **Cascade:** after writing a page, update every other page whose content is materially affected by the new information; refresh `index.md` and `overview.md` accordingly.
- Use project-relative Markdown links and `[[wikilinks]]` where useful.
- Do not delete files. If a page appears obsolete, record that in `wiki/log.md` for user review.
