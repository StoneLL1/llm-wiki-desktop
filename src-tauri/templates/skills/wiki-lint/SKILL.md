---
name: wiki-lint
description: Deep quality review of the project's Markdown wiki beyond deterministic checks.
---

# Wiki Lint

Read `purpose.md`, `schema.md`, and the existing `wiki/` tree.

- Work only inside the supplied lint workspace; do not modify project files.
- Treat the "Local deterministic findings already detected" section in the prompt as baseline truth; do not repeat those exact findings.
- Judge the wiki across exactly these dimensions:
  - `duplicate_topic` — two or more pages cover the same subject and should be merged or cross-linked.
  - `weak_cross_reference` — important relationships between pages are missing a `[[wikilink]]`.
  - `missing_source` — a derived page makes a claim that should cite a source in `wiki/sources/` but does not. (Pages inside `wiki/sources/` are themselves the originals and need no citation.)
  - `schema_mismatch` — a page whose structure violates `schema.md` (missing sections, wrong type).
  - `outdated_content` — content that contradicts newer pages or the stated purpose.
  - `contradiction` — two pages state mutually incompatible facts.
- Prefer specificity over volume; only report issues you can evidence with concrete page paths.
- Severity rubric:
  - `error` — deterministic broken navigation, index consistency, or source-traceability failure with concrete evidence.
  - `warning` — likely duplicate, weak cross-reference, merge/schema/citation quality issue, outdated content, or contradiction with concrete evidence.
  - `info` — suggestion, gap, or low-confidence improvement without direct breakage.
- Do not use `error` without evidence. If evidence is missing or weak, use `warning` or `info`.
- Respond with ONLY a fenced JSON block (```json) containing an array of objects with fields:
  - `issueType` — one of the six dimensions above.
  - `severity` — `error`, `warning`, or `info`.
  - `path` — the project-relative path of the primary affected page (e.g. `wiki/concepts/agent.md`).
  - `message` — a short description of the problem.
  - `evidence` — the phrase, section, or quote that justifies the finding (may be empty).
  - `suggestion` — a concrete recommended fix (may be empty).
- If there are no issues, respond with an empty array `[]`.
- Do not invent page paths that were not provided in the prompt.
