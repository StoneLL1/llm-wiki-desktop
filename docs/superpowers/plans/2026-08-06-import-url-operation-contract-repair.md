# Import URL and operation-contract repair plan

**Date:** 2026-08-06
**Scope:** Import V2 URL startup, batch-operation identity, task presentation, login recovery, exact-duplicate finalization, and large-cohort startup observability.

## Problem statement

Batch E/F correctly reduced large imports to one persisted control-plane task, but the cutover encoded the operation kind inside `BackendTask.batchId` as `import-v2-operation:<sessionId>`. That overloaded a grouping field with a session marker, gave every operation in one session the same identity, and forced the frontend to infer behavior from a string prefix. The generic `Import batch (1)` title then leaked into the URL flow. Two adjacent URL paths also remained on legacy behavior: authenticated-login recovery used the 200-item per-task launcher, and batch workers skipped exact-duplicate alias finalization. Finally, frontend startup errors were consumed before the URL form could decide whether to clear its value.

## Invariants

1. One user start action creates exactly one persisted operation task, regardless of item count.
2. The operation identity is unique per start action and is never the import session identity.
3. Operation kind/session/item-count are typed task metadata. Prefix parsing remains only as a read-compatibility fallback for old persisted tasks.
4. A one-item URL operation has a source-aware title; it is not presented as a numbered batch.
5. Item JSON remains the item state machine and is atomically claimed before workers run.
6. Expensive cohort preparation runs off the IPC/UI path and reports a visible preparation phase. Cancellation is observed before workers are enqueued.
7. URL, post-login recovery, and ordinary queued-item startup use the same batch launcher.
8. Exact content duplicates finish by recording the new locator as an alias without creating a second Source.
9. The operation represents background source preparation, not the later user commit: extraction-ready previews and recorded exact duplicates are successful preparation outcomes, while failed items fail the operation and unresolved prerequisites remain waiting.
10. A failed start rejects the frontend action, so URL input is preserved for retry.

## Implementation phases

### 1. Repair the task contract

- Add a backwards-compatible `TaskOperation::ImportBatch` payload to `BackendTask` with `sessionId`, `itemCount`, and optional `sourceLabel`.
- Add a dedicated `TaskService` constructor that assigns the task id as the operation/batch id.
- Centralize Rust and TypeScript operation detection, using typed metadata first and the legacy marker only for recovered tasks.
- Give singleton operations `Import <source>` titles and plural cohorts `Import <n> sources` titles.

### 2. Make startup observable

- Split task creation from cohort validation/binding.
- Return the running operation task immediately, then prepare the cohort on the bounded blocking runtime.
- Publish preparation progress, atomically bind the full cohort, re-check cancellation, and enqueue workers only after a successful bind.
- Surface asynchronous preparation failures on the operation task and leave no workers running.

### 3. Unify URL lifecycle behavior

- Route authenticated-login resumption through the same batch starter, removing the legacy 200-item ceiling.
- Run exact-duplicate finalization for batch workers before aggregating the item outcome.
- Treat exact-duplicate completion as a successful item outcome in batch counts.
- Preserve start errors through the frontend coordinator so the URL form does not clear failed input.

### 4. Correct task presentation and terminal semantics

- Show typed operation tasks as the single user-visible task and exclude them from legacy child-task grouping.
- Keep legacy grouped imports readable for persisted pre-cutover tasks.
- Finish item failures as failed and unresolved login/capability/authorization cohorts as waiting-for-confirmation. A cohort whose items all reached preview-ready or exact-duplicate completion succeeds because the background preparation operation has drained; preview and commit remain explicit Import-page actions.

### 5. Verification and delivery

- Add Rust contract tests for typed metadata, unique operation identity, source-aware title, background preparation failure/cancellation, login batch recovery, exact-duplicate batch completion, and partial terminal states.
- Add frontend tests for typed operation recovery/presentation and URL start-error preservation.
- Run focused tests while iterating, then two independent review passes required by `AGENTS.md`.
- Run `graphify update .`, update `progress.txt`/`gotchas.txt`, run the full `npm run check` gate from the beginning after fixes, and commit only the scoped changes.

## Compatibility and non-goals

- Existing task JSON without `operation` continues to deserialize.
- Existing `import-v2-operation:<sessionId>` task files remain recognizable and cancellable.
- Legacy `start_import_items_v2` remains available for compatibility, but no URL/login path should call it.
- This repair does not change import parsing, media policy, Source schema, or the immutable `raw/sources/` policy.
