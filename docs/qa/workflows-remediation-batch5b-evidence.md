# Workflows Remediation Batch 5B Evidence

Date: 2026-08-10
Scope: Batch 5B only (`WF-P05` cross-request Agent route probe cache). Batch 5C, Batch 6, and UI batches are not included.

## Stop/go dependency

Batch 5A completed with request-scoped inventory/hash/route reuse and recorded a release-profile Agent phase of 59.6% of the 1,000-Markdown overview mean. Its warm overview p95 remained 1,181.474 ms, above the 1-second reference target, so `docs/qa/workflows-remediation-batch5a-evidence.md` explicitly marked Batch 5B **GO**.

Provider availability is not cached across requests. The current Settings/OS-secret services do not expose a reliable secret generation, so Batch 5B is intentionally limited to Agent probes.

## Regression-first evidence

Before production code changed, the 1,000-Markdown overview regression was changed to require a second TTL-warm request to add zero Agent probes while still repeating source inventory, Markdown inventory, and every unique Markdown hash. The old implementation failed with eight cumulative Agent probes instead of four.

The final regressions prove:

- a TTL-warm second overview adds zero Agent probes while content and authority work remains request-fresh;
- provider secret availability changes are visible on the next request without invalidating warm Agent probes;
- cache hits require the same Agent kind, exact resolved spawn target (shim, program, and leading script arguments), cheap identities for every available target file, PATH generation, Agent settings revision, canonical project identity, identity revision, and cache epoch;
- settings, identity, executable replacement, manual refresh, and 30-second TTL expiry force a new probe;
- automatic shell/status capability reads do not invalidate the cache, while explicit user refresh establishes a new cache epoch after its detection completes;
- concurrent misses for the same key are single-flight;
- an invalidation or A-to-B resolver/PATH target change during an in-flight probe discards the old result and retries before returning or caching;
- cold probes for all four Agent kinds still start in parallel.

## Implementation and safety boundary

`AgentService` owns a process-local, 30-second, 128-entry bounded route-probe cache. The cache value is the existing non-secret `AgentInfo`; Agent version remains a value and is never executed to construct a warm key. Same-key misses use a condition-variable single-flight. Before insertion the owner re-resolves the exact spawn target and rebuilds the complete key; an epoch, PATH, resolver, shim, program, leading-script, or file-identity change discards the old result and retries, so a stale result is neither returned nor cached.

Workflow route evaluation still reads provider Settings and OS-secret status on every request. Project identity is recomputed before lookup. Trust, filesystem access, persistence, Git state, content inventory, Markdown reads/hashes, preparation/start freshness, and confirm/apply baselines do not use this cache. DTOs, IPC names, persistence formats, path guards, confirmation behavior, and native/compatible/memory-only semantics are unchanged.

Explicit user Agent refresh and successful Agent/default Settings writes invalidate the route cache. Automatic capability reads remain cache-preserving. External installations and replacements change the warm key through executable discovery and target file identities, with TTL/explicit refresh as the bounded fallback.

## Focused verification

- Regression proof before implementation: failed with eight cumulative Agent probes where four were required.
- AgentService unit group: 27 passed, including exact-target, single-flight, epoch-race, and resolver-switch regressions.
- Batch 5 overview unit group: 4 passed; 1 ignored release reference.
- Workflow integration suites: 29 passed (`workflow_preparation`, `workflow_routes`, `workflow_compatible_layout`).
- Focused capability hook suite: 8 passed, including automatic versus explicit `forceRefresh` ownership.
- Dual review: shared-context and fresh-context reviews both ended `CLEAN` after all findings were fixed and re-reviewed.
- Scoped diff check: passed.
- Final from-scratch `npm run check`: passed in 661.1 seconds (122 frontend files / 946 tests; 988-test Rust unit set with 984 passed and 4 ignored, followed by all Rust integration and doc tests plus the repository's import/capability/lint/build checks).

The first full-gate attempt exposed a stale test-fixture assumption: the cold-parallel fixture allocated one resolver barrier per Agent, while the corrected production path resolves again after probing to revalidate the target. The fixture now barriers only the first resolution of each distinct Agent command; it still proves all four cold probes enter before any release, and both reviewers reconfirmed `CLEAN` after this change. The from-scratch rerun above passed.

## Release performance gate

Reference environment: release profile, Windows x86_64, 24-way reported parallelism, five explicit cold/warm warmups plus 50 explicit cold-then-TTL-warm sample pairs, 1,000 Markdown files.

- TTL-warm total mean 897.624 ms; p95 930.893 ms; CV 2.24%.
- TTL-warm Agent subprocess count: 0 for every measured overview.
- TTL-warm Agent lookup phase mean 129.094 ms; p95 142.133 ms.
- TTL-warm inventory mean 463.902 ms; p95 489.705 ms.
- TTL-warm Markdown read/hash mean 291.453 ms; p95 322.194 ms.
- Cold Agent phase mean 576.907 ms; p95 601.003 ms.
- Cold slowest individual probe mean 576.461 ms; p95 600.587 ms.
- Every cold sample stayed within its slowest individual probe plus 500 ms.

The 1,000-Markdown warm p95 is now below the 1-second reference target, with stable dispersion and no warm Agent subprocesses.

## Graph closure

`graphify update .` completed after the final code/test formatting state. The AST graph now contains 12,865 nodes, 35,939 edges, and 585 communities. The updater retained 756 nodes from 82 still-present files that left the scan corpus under its fail-closed policy; this is unchanged graph-maintenance behavior rather than a Batch 5B correctness risk.
