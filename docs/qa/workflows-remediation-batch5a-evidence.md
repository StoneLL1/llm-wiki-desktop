# Workflows Remediation Batch 5A Evidence

Date: 2026-08-10
Scope: Batch 5A only (`WF-P05` overview request de-duplication and cold Agent probe parallelism). Batch 5B/5C production changes are not included.

## Regression-first evidence

The 1,000-Markdown public `WorkflowService::project_overview` regression was changed to require the Batch 5A hard gates before production code was modified. The old implementation failed with three route-catalog loads, 12 Agent probes, three Markdown inventories, and 3,000 baseline hashes instead of one, four, one, and 1,000.

The final request-scoped evaluation snapshot now provides one authoritative set of access/identity facts, source versions, resolved sources, Wiki/readable inventory, route catalog, and lazy Markdown hash/resource-reference results to all three overview workflow evaluations. A second overview request doubles every counter, proving that content and authority facts are not cached across requests.

| 1,000-file overview cost | Batch 0 baseline | Batch 5A |
| --- | ---: | ---: |
| Source inventory | 3 workflow evaluations | 1 request inventory |
| Markdown root inventory | 3 | 1 |
| Route catalog loads | 3 | 1 |
| Agent process probes | 12 | 4 |
| Unique Markdown reads/hashes | 3,000 | 1,000 |

Additional regressions compare every shared snapshot against independently evaluated Update Wiki, Quick Health Check, and Generate Content snapshots, and prove that all four cold Agent probes enter before any probe is released. Native Source/Wiki overlap, compatible Mixed/source-only behavior, excludes, DTOs, IPC, persistence formats, trust/writable/Git/path/confirmation semantics, and start-time fresh evaluation remain unchanged.

## Focused verification

- Batch 5A unit group: 3 passed; 1 ignored release reference.
- Workflow integration suites: 29 passed (`workflow_preparation`, `workflow_routes`, `workflow_compatible_layout`).
- Export resource safety regression: 1 passed.
- Persistent progress write/event regression: 1 passed (500 writes, 1,000 events).
- Scoped `rustfmt --check`: passed.
- Full `npm run check`: passed from scratch outside the filesystem sandbox in 501.6 seconds; Rust ran 982 unit tests plus all integration/doc tests, and frontend/contract gates passed. The first sandboxed attempt was invalidated by `spawn EPERM`/native binding restrictions and an AppData write denial, then the entire gate was rerun rather than combining partial results.

## Review closure

Two independent reviews completed. Findings fixed before the final gate:

1. Replaced helper-level inventory accounting with one real all-role Markdown-root walk and derived readable/Wiki views.
2. Restored streaming execution-time Markdown resource parsing so Project Report does not retain every document body.
3. Made route-non-Agent and Agent timing mutually exclusive; reference tests now require and record release profile.
4. Generated the measured Agent-kind label from `AgentKind::ALL` instead of stale hard-coded names.

Both reviewers returned `CLEAN` after the fixes.

## Batch 5B/5C stop-go reference

Reference environment: release profile, Windows x86_64, 24-way reported parallelism, five warmups plus 50 samples, request-fresh snapshots with OS-warm process/filesystem state.

### Batch 5B: GO

1,000-file warm overview:

- total mean 1,119.060 ms; p95 1,181.474 ms; CV 2.88%
- Agent mean 667.459 ms; p95 709.721 ms
- non-Agent route mean 0.473 ms; p95 0.623 ms
- inventory mean 273.107 ms; p95 295.552 ms
- Markdown read/hash mean 170.256 ms; p95 177.636 ms

Agent probing remains the largest phase (about 59.6% of total mean) and the 1,000-file p95 remains above the 1-second target, so Batch 5B should proceed. No Agent TTL cache was implemented in Batch 5A.

### Batch 5C: GO

Persistent workflow progress, 500 updates per sample:

- total mean 1,776.380 ms; p95 1,822.989 ms; CV 1.73%
- atomic task persistence mean 1,177.276 ms; p95 1,199.350 ms
- persistence share 66.27%

Task JSON persistence dominates the measured hot path, so Batch 5C should proceed. No progress-write throttling was implemented in Batch 5A.
