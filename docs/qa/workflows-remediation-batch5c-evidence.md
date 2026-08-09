# Workflows remediation Batch 5C evidence

Date: 2026-08-10

Scope: Batch 5C only (bounded persistence for non-critical workflow progress). Batch 6 and all other batches are excluded.

## Entry gate

Batch 0–4, 5A, and 5B were complete before implementation. The Batch 5A evidence explicitly marked Batch 5C `GO`: for 500 persistent workflow progress updates, total mean was 1,776.380 ms, atomic task persistence mean was 1,177.276 ms, and persistence represented 66.27% of the measured hot path.

No plan stop condition or unresolved product decision was encountered. The on-disk task schema, DTO/IPC contracts, queue ownership, trust/write/Git/confirmation rules, and frontend event-coalescing ownership remain unchanged.

## Regression-first evidence

The first Batch 5C regression changed the 500-update persistence expectation before implementation. It failed because all 500 updates wrote task snapshots.

The final review-driven regressions were also observed red before their fixes:

- persistence upgrade returned `Persistent` while the new task JSON did not exist;
- an idle crash after a suppressed final progress recovered the previous item instead of the latest item;
- a generic workflow barrier left an old trailing generation able to write again;
- the initial stale-writer test did not overlap the old writer with a newer barrier;
- rollback through a replaced task-state parent did not yet have a dedicated fail-closed helper/test.

## Implemented contract

- `Barrier` covers workflow create/mutation boundaries, stage transitions, confirmation/pending action, cancellation/terminal state, persistence transition, and generic persisted workflow facts.
- `ObservationalProgress` covers current item/count progress and uses a 250 ms persistence window plus a revision-aware trailing flush.
- Every workflow has an isolated serial persistence lane. A lane owns revision allocation, pending progress, trailing generation, and recoverable persistence failure state.
- Barrier order is snapshot -> per-task serial atomic persistence -> event/return. A failed barrier rolls memory back and publishes no barrier event.
- A failed observational write remains live in memory, records a pending persistence error, and is retried by the next barrier or trailing flush.
- Serialization and atomic file I/O occur after the global task map lock is released.
- A successful barrier cancels any older trailing generation, so an old callback cannot duplicate a write or lower the persisted revision.
- Authority upgrade is durable before `Persistent` is returned or published. Multi-workflow rollback restores typed snapshots through the normal project-scoped safe writer; rollback deletion revalidates project containment, parent identity, regular-file identity, and task binding immediately before deletion.
- Cancellation checks are unchanged and are not persistence-throttled. Different projects do not share a writer lane. Backend progress events remain uncoalesced; Batch 4 frontend logic remains the sole event-coalescing owner.

## Focused verification

- `cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --lib tasks::task_service::tests`: 59 passed, 1 ignored release reference.
- `cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --test workflow_queue`: 24 passed.
- `cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --test workflow_recovery`: 9 passed.
- `cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --test workflow_compatible_layout`: 4 passed.

The TaskService suite covers progress/barrier interleaving, idle trailing crash/recover, failed progress retry, terminal write failure and rollback, authority rebind and rebind failure, cancellation, confirmation, deterministic stale-writer overlap, failed rollback versus a concurrent log, different-project writer isolation, CJK paths, and parent-alias rollback safety.

Hard deterministic gates:

- 500 updates over the simulated 10-second fixture stay at or below `41 + barrierCount` task snapshot writes;
- ten identical fixed fixtures produce identical write counts;
- same-task persistence writer peak is 1;
- task snapshot I/O while holding the global task write lock is 0;
- all 500 workflow and 500 task progress events are still emitted to the live event bus.

## Release reference

Environment: release profile, Windows x86_64, 24-way reported parallelism, OS-warm same-volume tempdirs, five warmups plus 50 samples. Each sample aggregates ten identical 500-update fixtures and reports per-fixture latency.

- total mean: 177.451 ms
- total p95: 187.483 ms
- total CV: 3.65%
- atomic persistence mean: 114.743 ms
- atomic persistence p95: 124.997 ms
- persistence share: 64.66%

Compared with the Batch 5A 1,776.380 ms total mean, the final Batch 5C total mean is about 90.0% lower. Persistence remains the largest part of the reduced path, but its bounded call count satisfies the explicit Batch 5C hard gate.

A preliminary one-fixture-per-sample measurement showed 46.07% CV and was rejected as unstable. The final reference preserves the required 50 samples while aggregating ten fixed fixtures per sample and asserts CV below 15% before accepting timing conclusions.

## Reviews and repository gates

The shared-context review initially found missing trailing persistence and a non-durable persistence upgrade; a later pass found generic barriers did not cancel old trailing generations and rebind rollback needed side-effect-time path revalidation. All were fixed with regressions, and the final shared-context pass is `CLEAN`.

The fresh-context review initially found the same two P1 issues plus an inadequate stale-writer overlap test and a wall-clock-only trailing test. Deterministic writer gates, cross-project overlap, failed rollback concurrency, and generation-specific trailing completion notification were added. The final fresh-context pass is `CLEAN`.

`npm run check:quick` passed outside the sandbox. The sandbox-only attempt failed to load native Tailwind/Vite bindings (`spawn EPERM` / invalid UTF-8) and was not counted as product evidence.

After both final reviews and graph synchronization, the required from-scratch `npm run check` passed in 668.5 seconds. It included 122 frontend files / 946 tests, capability-tool tests, lint, production build, console scan, Rust GUI/core compilation, the complete Rust unit/integration/doc-test set, and the repository Import/Source contract checks.

`graphify update .` completed with 12,927 nodes, 36,234 edges, and 581 communities. It retained the repository's pre-existing fail-closed 756 nodes from 82 still-present files that left the scan corpus; no Batch 5C source relation remains stale.

## Residual risk

The 250 ms window deliberately allows loss of at most one observational progress window on abrupt process termination. No barrier fact may be lost. Path-based atomic replacement still has the repository's documented narrow OS-level replacement window, but every Batch 5C write/rollback path revalidates at the same safety boundary as the existing task writer and fails closed on observed alias or identity drift.
