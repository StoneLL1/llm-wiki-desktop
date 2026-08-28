# Three-core performance remediation — Batch 6 worker focused snapshot

Batch 6 freezes each Import worker's input at claim time and removes per-worker full-session reads. Machine-readable measurements are in [the Batch 6 result](results/2026-08-28-three-core-performance-batch-6.json).

## Runtime contract

- Batch and single-item admission return `ImportWorkItemSnapshot` values containing the item revision, input, subtitle choice, media authorization, authenticated-retry flag, and resource mode.
- `ImportWorkerJob` carries the frozen snapshot. A runtime observer confirms the production snapshot worker performs zero `SessionStore.load` calls; focused item writes use a before-image file transaction plus revision/task checks and return `IMPORT_V2_WORK_ITEM_STALE` if a newer item state exists, including an edit interleaved immediately before the transaction.
- Batch preparation builds the item/authorization lookup once. New-Source target reservations are shared by concurrent operations in the same session and are seeded only from selected, `PreviewReady` canonical items, so workers do not rescan the session for every target.
- Batch workers report exact-duplicate candidates to the operation coordinator. After the cohort drains, one commit operation groups candidates by target Source, records legal aliases idempotently, and writes one batch history with per-item results. Failure facts are persisted in one cohort update; partial success remains truthful when a later alias is cancelled or becomes stale.

## Scale evidence

The synthetic worker admission/preparation contract uses the frozen 100 / 1,000 / 10,000 item fixtures and observes project control-plane reads and writes. Engine execution is deliberately excluded so the measurement isolates the worker control plane rather than parser cost:

| Items | Reads | Writes | Total operations | Growth from prior fixture |
| ---: | ---: | ---: | ---: | ---: |
| 100 | 103 | 102 | 205 | — |
| 1,000 | 1,003 | 1,002 | 2,005 | 9.78× |
| 10,000 | 10,003 | 10,002 | 20,005 | 9.98× |

The observed formula is `2N + 5`, below the test budget of `4N + 16`; each 10× input increase remains below the required 15× operation increase. The final 10,000-item Windows run completed in 209.15 seconds. This timing is diagnostic; the contract is on I/O growth rather than a new wall-clock product budget.

## Focused verification

- frozen snapshot rejects a revision changed before claim;
- claimed worker CAS rejects both a change made after claim and an external edit interleaved between the validated read and transaction, preserving the newer item bytes;
- exact-duplicate cohort commits two aliases with one new batch-history file and no second Source version, preserves completed aliases during cancellation, and filters a stale alias without blocking valid siblings;
- a synthetic 1,000-failure duplicate cohort records all failure facts with one full-session load and one bounded cohort save;
- production job contract verifies worker jobs carry snapshots; a runtime full-session-load observer verifies the exercised production snapshot worker reads no full session;
- 100 / 1,000 / 10,000 scale contract passes with the measurements above.

## Deferred ownership

- Batch 7 owns true History pagination and bounded history receipts.
- Batch 8 owns the general commit lookup/history-byte linearization and project-scoped Import lock registry.
- Batch 9 owns combined installed-app acceptance and progress durability.
