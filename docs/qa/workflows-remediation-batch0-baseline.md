# Workflows remediation Batch 0 baseline

Date: 2026-08-09  
Environment: Windows, Vitest + jsdom, Rust `--no-default-features`, deterministic synthetic data  
Scope: test fixtures and measurement only; no production behavior, DTO, IPC, persistence format, or user project content changed

## Measurement rules

- Hard assertions use calls, commits, mounted rows, payload bytes, state transitions, persistence counts, and event counts.
- Absolute wall-clock time is not used as a pass/fail signal in this batch.
- Every large fixture is generated deterministically; a full-content digest (including every event, drawer payload, path, option, attempt, and diff byte) is rebuilt ten times and must be identical.
- Race fixtures use channels at claim, dispatch guard, first-stage start, and worker finish. They do not use sleeps.
- Known pre-fix behavior is recorded as a green witness. Later remediation batches must replace each witness with its positive budget rather than deleting it.

## Fixed fixtures

| Scenario | Fixed size / shape |
| --- | ---: |
| Workflow event burst | 200 events in 1.99 seconds; final event terminal |
| Controller amplification witness | 50 ordinary progress events |
| Closed task drawer burst | 1,000 separate task/log/activity/output store events |
| Markdown inventory | 1,000 CJK-safe project-relative paths |
| Overview route/probe rounds | 3 workflow snapshots; 4 Agent kinds per snapshot |
| Progress persistence | 500 updates over 9.98 seconds |
| Preparation options | 10,000 source/version pairs |
| History attempts | 10,000 runs |
| Decision Diff | 500 files × 20,480 ASCII bytes = 10,240,000 bytes |

## Pre-fix quantitative baseline

| Finding / boundary | Input | Observed baseline |
| --- | --- | --- |
| WF-P01 refresh amplification | Initial load + 200 events over 1.99 seconds, final event terminal | overview `201`, history `201`, detail `0`; the event path synchronously lands the final `completed` payload before queued refreshes settle |
| Five-way controller counters | Initial load + detail + prepare + start | overview `2`, history `2`, detail `1`, prepare `1`, start `1` |
| WF-P02 hidden drawer work | Drawer closed + 1,000 separate events | visible drawer DOM `0`; `1,000` additional `TaskLogDrawer` React commits |
| WF-P03 denied permission path | 200 eligible completion events, permission denied | permission checks `200`, permission requests `200`, sends `0` |
| WF-C07 same-root prepare | Overview moves `revision-a → revision-b` while prepare A is pending | overview remains B, stale preparation A is committed |
| WF-C07 same-root perform | Start A is in flight, then overview/preparation move to B before outcome A resolves | stale run A is selected before reconcile |
| WF-C01 compatible preference witness | Real `WorkflowPreferences::remember/load` against native, compatible, and memory-only `ProjectContext` values | native persists under `.app/workflows`; compatible incorrectly persists there too and leaves `.app/compat/workflows` unused; memory-only creates no `.app` state |
| WF-C06 claim/cancel window | Cancel after claim and before first stage | first-stage start is rejected and the underlying task remains `Cancelling` without a shared finalizer |
| WF-C06 trust/dispatch window | Real `AppState::revoke_project_trust` after next run is claimed and before actual `WorkflowService::dispatch_claimed_run` receives the stale claimed snapshot | authority becomes untrusted and task becomes memory-only `Cancelling`, but pre-fix dispatch still starts the registered runner |
| WF-C06 worker finish window | Cancel after worker finish signal and before terminal transition | terminal transition is rejected and the underlying task remains `Cancelling` without a shared finalizer |
| WF-P05 backend cost counter fixture | Real overview prerequisites over 1,000 Markdown files | RouteCatalog loads `3`, Agent process probes `12`, Markdown enumerations `3`, baseline hashes `3,000` |
| WF-P05 progress counter fixture | 500 real `update_workflow_stage_progress` calls on a persistent run | task persistence writes `500`, EventBus emissions `1,000` (`500` workflow + `500` task updates) |
| WF-P06 Preparation DOM | 10,000 options | mounted option rows `10,000` |
| WF-P06 History DOM | 10,000 attempts | mounted run buttons `10,000` |
| WF-P06 Diff DOM/payload | 500 × 20KB | mounted Diff blocks `500`, text payload `10,240,000` bytes |

## Test ownership

- Frontend fixture source: `src/features/workflows/workflowBaselineFixtures.ts`
- Controller counters and identity witnesses: `src/features/workflows/useWorkflowsController.test.tsx`
- Scale DOM/payload witnesses: `src/features/workflows/workflowBaselineFixtures.test.tsx`
- Closed drawer commits: `src/components/app/TaskLogDrawer.test.tsx`
- Permission counters: `src/services/notificationsBaseline.test.ts`
- Rust fixture/authority support: `src-tauri/tests/support/workflow_baseline.rs`
- Real-path test-only backend counters: `src-tauri/src/services/workflow_service/preparation.rs`, `src-tauri/src/tasks/task_service.rs`
- Controlled race windows: `src-tauri/src/app_state.rs`, `src-tauri/tests/workflow_queue.rs`, `src-tauri/tests/workflow_recovery.rs`
- Rust scale and preference-path witnesses: `src-tauri/tests/workflow_baseline_fixtures.rs`

## Batch 0 acceptance

- Fixtures and counters are test-only and do not enter production IPC or release bundles.
- No test relies on a sleep or an absolute millisecond performance threshold.
- No product behavior or user project file is mutated.
- Focused frontend and Rust results are recorded in `SPEC/progress.txt` after the review and repository gate complete.

## Final verification

- Focused frontend: 4 files, 40 tests passed; the controller-only rerun passed 23 tests.
- Focused Rust: real preparation-cost, progress persistence/EventBus, and AppState revoke/dispatch unit witnesses passed; baseline/queue/recovery integration suites passed 3 + 21 + 9 tests.
- Independent review A: CLEAN after real-path counter, preference, terminal-event, and AppState revoke/dispatch corrections.
- Independent review B: CLEAN after deterministic ProcessRunner, real claim/first-stage channel, legal Agent route, and fixed four-Agent/12-probe contract corrections.
- Final gate: `npm run check` passed from the beginning with 120 frontend files / 891 tests, 65 capability-tool tests, 967 Rust unit tests plus all integration/doc tests, lint, production build, console scan, and Rust GUI check.
- Graphify: `graphify update .` rebuilt 12,574 nodes, 34,831 edges, and 578 communities; it retained 756 fail-closed nodes that left the scan corpus but still exist on disk and reported 243 zero-node source files for future retry.
