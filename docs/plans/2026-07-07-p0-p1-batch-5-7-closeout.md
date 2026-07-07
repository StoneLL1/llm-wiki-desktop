# P0/P1 Non-Import Batch 5-7 Closeout

Date: 2026-07-07

## Implemented Scope

- Added internal `wiki-query` Skill template as a read-only, index-first, citation-required query surface.
- Extended chat retrieval with bounded one-hop graph expansion and source-overlap expansion.
- Kept expanded retrieval pages in diagnostics; expanded pages are prompt sources but never persisted citations unless the model cites their `[S#]` marker.
- Added local deterministic lint for page type validity, required derived-page `sources`, source path existence, human-readable source sections, and basic structural page health.
- Updated deep lint prompt layering so the Agent receives the local deterministic baseline and severity rubric, and is told not to duplicate deterministic findings.
- Added context-aware Agent lint normalization that rejects unknown paths, filters deterministic duplicates, and downgrades evidence-free `error` findings.

## Boundaries Preserved

- No import redesign work.
- No database, vector DB, or LanceDB.
- No write API, localhost API, or MCP implementation.
- Normal Search remains local keyword/filter search; natural-language answering remains Chat/Agent/BYOK.
- `wiki-query` documents read-only localhost API/MCP as a future phase only.

## Integration Notes

- CompilePlan and manifest validation remain no-write gates.
- Chat citations remain model-used evidence parsed from final answer markers.
- Retrieval diagnostics now include `expandedPages` alongside retrieval hits, selected pages, omitted pages, invalid citation IDs, and unverified markers.
- Local lint emits deterministic source/schema issues without requiring Agent deep lint.
- Agent deep lint remains heuristic and non-fixable.
