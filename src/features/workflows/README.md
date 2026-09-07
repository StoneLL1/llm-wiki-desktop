# Workflows

Project-scoped presentation for the three fixed product workflows: Update Wiki, Health Check, and Generate Content.

## Ownership

- `useWorkflowsController.ts` owns typed IPC calls, stale-request guards, preparation, and query reconciliation.
- Backend `TaskService` owns task facts; frontend `taskStore.workflowById` accepts canonical summaries from IPC and the global event bridge, including for inactive projects.
- `workflowStore.ts` owns project-scoped queries, drafts, selection, and at most 16 on-demand task details; project identity or backend session changes invalidate stale presentation.
- Preparation/start actions, history queries, result opening, and Workflow notification details load on demand. The controller captures guards and operation locks before loading; stale intents stop before IPC and loading failures release their locks.
- Views render structured workflow DTOs and project-scoped run history. Backend services remain the authority for project identity, access, trust, route selection, queueing, Git policy, confirmation, and mutation.
- Settings owns Agent and Provider configuration. Lint owns repairs. Exports owns generated-artifact records and previews. The generic task drawer owns raw logs.

## Refresh model

- Ordinary running updates are owner/session-filtered, merged by task and revision in the global dispatcher, and committed in a 100ms window. They never pull overview or history.
- Waiting, terminal, queued, continuation, and other semantic boundaries commit immediately and schedule a project-scoped overview reconciliation.
- Overview reconciliation uses one in-flight request and a dirty flag. The backend reads its owner summary index and observed access snapshot without preparation, Markdown scans, Git, Agent probes, or history reads. An old-project response cannot commit or schedule work for the new project.
- History loads independently when its surface is opened, on explicit pagination/refresh, or on a terminal boundary while visible. History errors do not hide the overview. Selected details hydrate on demand; only a newer known revision schedules a follow-up read.
- The global task event bridge always preserves backend task facts. Workflows and right-panel selectors are route-local; inactive Import ownership retains a frozen context for exact resumption without subscribing to background task updates.

## Update Wiki

- Start prepares the current source/route draft automatically. A changed scope, automatically resolved route, or sensitive-content acknowledgement returns the fresh preparation for review before starting.
- Generation reuses CompileService and an isolated candidate workspace. The Agent receives exact approved input filenames; Source originals remain outside its write scope.
- A changed queued baseline enters `review_scope` without a candidate. Re-preparing retains selected source IDs and creates a retry-linked task only after reviewing current versions.
- Candidate confirmation and checked apply preserve Source, checkpoints, user edits, conflict hashes, and current project authority. The result precedes the four visible phases; logs remain collapsed by default.

## Non-goals

- No arbitrary prompts, shell commands, filesystem writes, Git operations, or secret access from React.
- No global or cross-project task launcher, silent execution-route fallback, or fourth built-in workflow.
- No replacement for technical Agent services, Agent types, capability detection, or the unchanged sidebar Agent status foot.

The superseded Agent page, right panel, and generic Run Agent dialog were retired in Workflows Batch 8. Compatibility-only Agent concepts remain under their existing technical names.

## Decision Gate H status

As of 2026-08-13, H3–H5 provide backend-derived Agent Health availability and the guarded Lint repair task surface while preserving the existing queue, confirmation, checkpoint, result, and Diff contracts. H6 final validation remains no-go: the recorded full gate is not green in the current Windows environment, and the complete performance/negative/WebView2 evidence matrix is not closed. Do not mark Decision Gate H or Batch 7 unblocked.
