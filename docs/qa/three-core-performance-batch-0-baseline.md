# Three-core performance remediation — Batch 0 baseline

Batch 0 adds observation and red contracts only. It does not move commands to background lanes, split recovery from reads, normalize stores, virtualize Queue rows, index History, or linearize commit.

The machine-readable evidence is [the Batch 0 result](results/2026-08-27-three-core-performance-batch-0.json). Command classifications live in [the explicit inventory](tauri-command-execution-inventory.json); the check fails for a missing, stale, duplicate, invalid, or signature-mismatched command entry.

## What the baseline proves

| Chain | Calls/publications | I/O or lookup | Thread | Mounted rows |
| --- | ---: | ---: | --- | ---: |
| One backend task event | 1 dispatch, 2 upserts, 2 task-store publications | none | WebView JS main | not applicable |
| 10 Hz task progress for one second | 10 events, 20 task-store publications | none in this observer | WebView JS main | not applicable |
| One Import patch at N=10,000 | 1 Import-store publication, full 10,000-entry array replacement | none in this observer | WebView JS main | Queue starts at 200 |
| Queue load-more fixture | 99 React commits | none | WebView JS main | 200 initially; 10,000 after 49 measured load-more actions; no virtual window |
| Session load at N=10,000 | 1 store call | 10,001 reads, 4,355,832 bytes | caller; current synchronous command callers remain GUI-thread work | not applicable |
| Single item update at N=10,000 | 1 update call | 10,002 reads, 1 atomic write | caller | not applicable |
| Recovery-backed session GET at N=10,000 | 1 recovery call | 10,001 reads and 10,001 atomic writes | current synchronous Import command: GUI command thread | not applicable |
| History scan at H=10,000 | 1 production V2 scan | 10,000 receipt reads, 3,804,024 bytes, 0 writes | current synchronous Import command: GUI command thread | not applicable |
| Commit decision lookup at D=10,000 | 1 validation pass | 50,005,000 item-id comparisons | caller under the Import mutation lock | not applicable |

`readOps` counts successful payload reads through FileStore, not metadata existence checks. `atomicReplaces` counts successful atomic payload publications, including atomic create-new publication. Bytes are exact for the deterministic fixture run and are not portability budgets.

The current command registry contains 200 commands: 189 synchronous, 11 async, 55 Import commands, 53 synchronous Import commands, and 186 synchronous commands classified as blocking work. These values supersede the older plan estimate of 201/190 and are deliberately red; Batch 1 owns execution-lane changes.

## Reproduction

Run the stable automated observers:

```powershell
npm run check:command-execution
npm run test -- src/features/import/importPerformanceBaseline.test.tsx --reporter=verbose
Set-Location src-tauri
cargo test --no-default-features --features performance-observers --test performance_baseline_observers -- --nocapture
cargo test --no-default-features --features performance-observers batch_zero_decision_lookup_counter_records_current_quadratic_shape -- --nocapture
```

For the packaged scenario, launch a packaged Windows build with WebView2 remote debugging, open a disposable project whose Import Queue already has exactly 10,000 items, and start a deterministic 10 Hz progress stream. While the harness samples, continuously drag the native app window and attest that action explicitly:

```powershell
node scripts/run-import-contention-packaged-baseline.mjs --endpoint http://127.0.0.1:9223 --duration-ms 30000 --expected-items 10000 --window-drag-observed yes
```

The harness alternates Import and Wiki through language-independent route identifiers, samples visible progress every 50 ms, and requires an observed 8–12 Hz cadence before it can report `completed`. Its animation-frame loop is constant-time; row counts are maintained from mutations; every observer and animation handle is stopped before diagnostics are collected. It records Queue row counts/mutations, WebView long tasks, and animation-frame gaps, and emits no project content or paths. It fails closed on the wrong fixture size, missing cadence, or missing window-drag attestation. This delegated run had no disposable packaged 10k/10 Hz fixture, so packaged numeric fields remain `null` rather than being estimated; the scenario and capture contract are ready for the packaged run owned by the next execution milestone.

## Ownership left to later batches

- Batch 1 moves blocking commands off the GUI command thread.
- Batch 2 separates pure reads from recovery writes.
- Batch 3 removes duplicate task publication and adds semantic no-op behavior.
- Batches 4–7 add bounded session/history data shapes and virtualized UI.
- Batch 8 linearizes commit and shards the global Import lock.
- Batch 9 performs final packaged Windows tracing and acceptance.
