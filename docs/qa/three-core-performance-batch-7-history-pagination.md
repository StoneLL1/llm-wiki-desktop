# Three-core performance remediation — Batch 7 History pagination

Batch 7 replaces Import History's full scan and unbounded list DTO with a rebuildable summary index, independently paged item detail, and bounded per-item receipts. Machine-readable measurements are in [the Batch 7 result](results/2026-08-28-three-core-performance-batch-7.json).

## Runtime contract

- The History list reads its small index manifest and only the index page needed for the requested 50 summaries. `ImportHistoryEntry` now carries counts and at most two sample labels; it no longer carries every item ID.
- History detail has a separate cursor and reads one 50-ID order page plus only those item snapshots. Historical result preview resolves the requested item snapshot instead of loading the full historical session.
- A working batch stores a bounded manifest, 50-ID order pages, one receipt per result, and one snapshot per item. Each result update rewrites only its snapshot and the small manifest; terminal finalization writes the legacy-compatible monolithic batch exactly once.
- Existing monolithic batch records, crash-only working manifests, and legacy task history remain canonical/readable input. Missing or corrupt derived indexes start one canonical-project-root-deduplicated, cancellable durable Import task in the bounded Heavy I/O lane when writes are allowed; restricted/read-only projects keep a bounded compatibility page plus the rebuild warning. Rebuild writes a new generation before publishing its manifest pointer, never changes corrupt canonical bytes, and surfaces `IMPORT_V2_HISTORY_CORRUPT` with an evidence path.

## Scale evidence

The synthetic contract creates 10,000 canonical batch records, rebuilds their derived index, then starts a fresh FileStore observation for page 1:

| Operation | Fixture | Returned | Reads | Writes |
| --- | ---: | ---: | ---: | ---: |
| History list page 1 | 10,000 batches, limit 50 | 50 summaries | 2 | 0 |
| History detail page 1 | 10,000 items, limit 50 | 50 items | 52 | 0 |

At `H=10,000`, the list count is exactly one manifest plus one index page; inspection confirms the read path does not traverse the remaining history. Detail is exactly one manifest, one order page, and 50 item snapshots; neither path writes or loads a full historical session.

## Compatibility and safety verification

- corrupt canonical JSON bytes are byte-for-byte unchanged after rebuild and produce a bounded warning;
- corrupt derived indexes return a bounded read-only page and rebuild warning rather than failing the History surface;
- derived index pages can be rebuilt from old monolithic history, crash-only working manifests, and the existing legacy adapter;
- cancellation after a new generation page is written leaves the previously published manifest and page generation readable;
- compatible layouts read/write only `.app/compat` history/task roots, including a CJK filename fixture;
- a concurrent edit to a working history file is rejected by hash guard and remains unchanged;
- commit failure and cancellation paths write bounded result facts, while successful Source changes remain partial-success safe;
- index preparation is project-authorized, reports incremental TaskService progress, checks cancellation inside records/snapshots/pages, and atomically deduplicates concurrent active requests by canonical project root;
- summary actions continue through bounded detail cursors, so a result after item 50 remains reachable, and the History page refreshes when its rebuild task succeeds;
- the command execution gate classifies all 60 Import commands as async, with index rebuild classified as `LongTaskStart`.

## Deferred ownership

- Batch 8 owns O(1) commit decision lookup, incremental counters, co-transaction of the history receipt with canonical Source/item writes, and the canonical-project/session lock registry.
- Batch 9 owns observational progress persistence and combined installed-app acceptance.
