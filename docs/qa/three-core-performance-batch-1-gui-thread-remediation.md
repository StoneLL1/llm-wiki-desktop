# Three-core performance remediation — Batch 1 GUI-thread remediation

Batch 1 moves the current Import command surface and its background synchronous work onto bounded blocking lanes without changing command names, request/response DTOs, session storage, or commit semantics. Machine-readable results are in [the Batch 1 result](results/2026-08-27-three-core-performance-batch-1.json).

## Execution contract

- `BlockingWorkCoordinator` is shared through `AppState`. Metadata I/O admits at most four workers, heavy I/O at most two, and project Git work at most one worker for each canonical project identity.
- Admission is asynchronous. Cancellation is rechecked after admission and again on the worker before user code starts; a panic or join failure returns a typed `BackendError`, suppresses worker panic payloads, and leaves the lane usable.
- The optional `LLM_WIKI_BLOCKING_TRACE_PATH` observer records JSONL spans containing only work class, a closed operation label, caller/worker thread IDs, queue/run durations, outcome, and a sanitized error category. It has no project path, filename, URL, content, secret, or raw payload input.
- All 55 registered Import commands are explicitly classified and async. The two native network-async commands keep network waiting on the async path while their synchronous preflight/finalization phases use the coordinator; the remaining 53 registered wrappers move their complete synchronous command bodies into the coordinator.
- Import agent assistance, capability installation/verification/activation, batch preparation/execution, file scan, migration, and commit background paths no longer execute synchronous filesystem/Git work directly on an async executor. Git mutations acquire project write authority before the per-canonical-project Git lane.

The first command-execution gate freezes all 200 registered commands, rejects missing or stale classifications, rejects a newly synchronous Import command, and prevents increases to the pre-existing non-Import blocking-sync baseline. Batch 1 intentionally does not migrate all non-Import legacy debt.

## Packaged Windows trace

A Tauri CLI release build was started with WebView2 remote debugging and the anonymous blocking span observer. The harness created a disposable native knowledge base outside the repository, invoked all eight commands frozen as P0 plus the session-creation fixture command, and directly validated the generated JSONL field allowlist and thread identities:

| Command | End-to-end promise time | Blocking lane |
| --- | ---: | --- |
| `get_import_frontend_readiness_v2` | 8.5 ms | heavy I/O |
| `create_import_session_v2` | 1754.8 ms | heavy I/O |
| `get_import_session_v2` | 1642.0 ms | heavy I/O |
| `list_import_history_v2` | 6.7 ms | heavy I/O |
| `get_import_history_session_v2` | 6.9 ms | heavy I/O |
| `get_import_preview_content_v2` | 6.1 ms | heavy I/O |
| `set_import_item_resolution_v2` | 1580.4 ms | heavy I/O |
| `authorize_local_asr_v2` | 1573.1 ms | heavy I/O |
| `confirm_import_session_v2` | 1605.5 ms | heavy I/O |

At the harness snapshot, all nine anonymous command spans entered the coordinator on `ThreadId(27)` and ran on `ThreadId(110)`. During the sequence the WebView produced 3897 frame samples, the maximum frame gap was 9.4 ms, and no long task was observed. The harness fails if any sampled heavy-I/O span uses the caller thread, if any field falls outside the seven-field anonymous allowlist, or if the disposable project path appears in the trace. The runtime trace proves the eight frozen P0 paths exercised here; the command inventory and source gate cover all 55 registered Import commands. Nested project-Git spans intentionally run inside an admitted blocking worker after project-write authorization.

The timings are diagnostic observations, not new product latency budgets. Batch 2 still owns read/recovery separation, so `create_import_session_v2` and `get_import_session_v2` can remain slow end to end even though they no longer block the foreground execution lane.

Reproduce after building a Tauri release and starting it with WebView2 remote debugging plus `LLM_WIKI_BLOCKING_TRACE_PATH`:

```powershell
node scripts/run-import-command-packaged-trace.mjs --endpoint http://127.0.0.1:9224 --project-root <new-disposable-project-path> --trace-path <new-jsonl-path>
```

Both paths must not exist; the harness refuses to initialize an existing materials folder or mix a new observation with an old trace.

## Verification

- `npm run check:command-execution`
- `cargo test --manifest-path src-tauri/Cargo.toml --no-default-features blocking_work -- --nocapture`
- `cargo test --manifest-path src-tauri/Cargo.toml --no-default-features capability_installer::tests -- --nocapture`
- `cargo check --manifest-path src-tauri/Cargo.toml`
- `npm run tauri -- build --no-bundle`
- packaged trace harness above

The repository-wide `npm run check` ran the complete 1186-test Rust library suite and the long Import integration/scale lanes successfully. Its default-environment Rust run then hit one sandbox-only `%APPDATA%` access denial; rerunning the complete `npm run test:rust` gate with `APPDATA` bound to a temporary directory passed, including all remaining integration and doc-test lanes. The frontend lane still stops on two pre-existing `test:final-four-redlines` owner-state expectations owned by unrelated release batches. No final-four owner declaration or implementation is changed by Batch 1.

## Deferred ownership

- Batch 2: separate pure reads from recovery writes.
- Batches 4–7: bounded session/history payloads and normalized/virtualized frontend data shapes.
- Batch 8: commit algorithm rewrite and global Import lock sharding.
- Later command-execution work: pay down the frozen non-Import synchronous baseline without expanding this batch.
