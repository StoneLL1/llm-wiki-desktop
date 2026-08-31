# Three-core performance remediation — Batch 9 final acceptance

Batch 9 closes generic task-progress durability and combines the evidence produced by Batches 0–9. The machine-readable result is [2026-08-28-three-core-performance-batch-9.json](results/2026-08-28-three-core-performance-batch-9.json). The result belongs to the commit containing that file and is measured from base `5d85c611` plus the Batch 9 diff.

## Progress durability

`TaskService::update_progress` now shares the revision-aware persistence lane previously exercised by Workflows. Observational task progress remains live on every call but writes a complete snapshot at most once per 500 ms; the trailing flush writes the newest observation after the stream becomes idle. Status boundaries use the same per-task lane, persist before publication, cancel any pending observational generation, and restore the prior in-memory task if the durable write fails.

The deterministic 60-second contract drives 600 Import progress calls at 10 Hz. It observes exactly 600 live events and 120 stream-window task snapshots, or 2 Hz, plus at most one idle trailing flush. A restart after that trailing flush recovers the latest progress. A failure injection through the real Import `finish_running_operation` path proves that a failed durable finish leaves the task Running with no result and emits no terminal event; the successful retry stores the latest progress, result, and Succeeded status before publishing completion. A deterministic paused-writer test also proves that progress serializes before the finish barrier.

## Combined §14 evidence

| Metric | Baseline | Current | Evidence |
| --- | ---: | ---: | --- |
| sync / total commands | 189 / 200 measured in Batch 0 | 136 / 205 overall; 133 legacy non-Import blocking sync commands remain, so the global PureMemory-only target is **not closed** | command-execution inventory and Batch 1 scoped gate |
| sync Import commands | 53 / 55 | 0 / 60 | command-execution inventory and gate |
| no-op session writes | N + summary in recovery baseline | 0 | side-effect-free read/recovery FileStore observers |
| first page item reads | N | at most 200 item reads; at most 202 reads including state/order | 10k SessionStore scale observer |
| one task event publications | 2 | 1; semantic duplicate 0 | task dispatcher/store observers |
| progress publication | up to about 10 Hz through multiple writers | at most 5 Hz visible | dispatcher fake-clock contract |
| Queue mounted rows | 200 then unbounded | at most 80 in the 10k unit scroll contract; 24 in packaged trace | DOM observer |
| worker session loads | about 2N full loads | 0 full load/item | Batch 6 runtime observer |
| history page reads | O(H) | 2 reads for 10k histories, limit 50 | Batch 7 FileStore observer |
| commit lookup | 50,005,000 comparisons at D=10k | 10,000 map probes | Batch 8 operation counter |
| history serialized bytes | cumulative O(D²×S) | 15,410,000 bytes at D=10k, linear per-item receipts | Batch 8 byte counter |
| project A→B lock blocking | one >50 ms wait in baseline | 0 waits over 50 ms | Batch 8 barrier/span |
| task progress persistence | one full snapshot per call | 120 stream-window snapshots for 600 calls over 60 seconds, plus at most one idle trailing flush | Batch 9 task observer |
| WebView >50 ms long task | Pending | 0 during the 30-second packaged scenario | PerformanceObserver |
| window drag / route responsiveness | Pending | 26 route switches, 5.0–15.4 ms; real `task://updated` event-to-next-paint p95 58.8 ms | packaged CDP/native drag trace |

The packaged run used a disposable project containing 10,000 generated Markdown sources of 1,024 bytes each. Its real Import operation was `running` at both the start and end boundary checks while the harness also scheduled a supplemental `start_project_inventory` TaskService workload at 10 Hz through packaged Tauri IPC, switched Import/Wiki routes, and automated 28 seconds of native-window dragging. The fail-closed harness requires zero IPC failures, no pending calls, sufficient real events, event-to-paint p95 ≤100 ms, zero >50 ms JavaScript long tasks, ≤200 DOM rows, route latency ≤150 ms, and the real Import operation to remain queued/running/cancelling at both boundaries. It observed 301 completed/0 failed/0 pending replay calls and 1,806 matching `task://updated` events; p95 was 58.8 ms. The Queue peaked at 24 DOM rows, no JavaScript long task exceeded 50 ms, and 26 route switches stayed below 15.4 ms. Two RAF gaps exceeded 50 ms, with a maximum of 100.1 ms, during native movement; no matching JavaScript long task occurred. Batch 1's packaged spans remain the thread-affinity evidence: measured blocking Import commands entered worker threads distinct from their callers, and all 60 Import commands are async.

The final shared-context and fresh-context reviewer pass completed once, as required. Accepted findings tightened the real-Import fixture, fail-closed event/IPC/budget checks, finish/cancel lane ordering, terminal rollback evidence, and report wording. The final `npm run check` then passed from the beginning in 20 minutes 17.2 seconds: 145 frontend test files / 1,256 tests passed, the Rust library reported 1,212 passed / 4 ignored before all Rust integration and doc-test lanes also completed, and lint, production build, bundle budget, console scan, command execution, release configuration, and capability-tool checks passed. The command authority inventory now accounts for all 205 registered commands. `graphify update .` rebuilt the AST graph to 17,243 nodes, 49,236 edges, and 829 communities.

## Environment and platform status

The completed run used Windows 11 `10.0.26200`, WebView2 `151.0.4129.107`, an AMD Ryzen AI 9 HX 370 (24 logical processors), 31.1 GiB RAM, a 2880×1800 display, and a release Tauri executable. Microsoft Defender antivirus, service, and real-time protection were enabled. An AV-disabled run is explicitly **Pending** because this delegated benchmark did not authorize weakening host security. macOS and Linux are also **Pending** because no runners for those platforms are attached to this task; no cross-platform result is inferred from Windows.

## Scope left after Batch 9

No later remediation batch is implemented here. Batch 9's Windows acceptance and local durability contracts are complete. A same-code macOS run, Linux run, and an explicitly authorized AV-disabled comparison remain external evidence work. Separately, the plan-wide §7.1/§14 PureMemory-only command target remains open because Batch 1 explicitly deferred 133 legacy non-Import blocking synchronous commands; Batch 9 does not conceal or expand that debt.
