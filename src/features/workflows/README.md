# Workflows

Project-scoped presentation for the three fixed product workflows: Update Wiki, Health Check, and Generate Content.

## Ownership

- `useWorkflowsController.ts` owns typed IPC calls, stale-request guards, and `workflow://updated` event merging.
- `workflowStore.ts` is the project-scoped presentation state; it must be reset on project switches.
- Views render structured workflow DTOs and project-scoped run history. Backend services remain the authority for project identity, access, trust, route selection, queueing, Git policy, confirmation, and mutation.
- Settings owns Agent and Provider configuration. Lint owns repairs. Exports owns generated-artifact records and previews. The generic task drawer owns raw logs.

## Refresh model

- Ordinary running updates are identity-filtered, merged by task, and committed in a 100ms window. They never pull overview or history.
- Waiting, terminal, queued, continuation, and other semantic boundaries commit immediately and schedule a project-scoped overview reconciliation.
- Overview reconciliation is single-flight per project, and each logical wave permits one first pass plus at most one trailing pass. A boundary arriving during that trailing pass starts a separate bounded wave after the current wave releases ownership; it never extends the current wave or overlaps an invoke. An in-flight request from an old project may finish, but it cannot commit or schedule work for the new project.
- History loads on the initial snapshot, explicit pagination, manual refresh while History is visible, or a terminal boundary while History is visible. It is not a progress-event side effect.
- The global task event bridge always preserves backend task facts. Workflows and right-panel selectors are route-local; inactive Import ownership retains a frozen context for exact resumption without subscribing to background task updates.

## Non-goals

- No arbitrary prompts, shell commands, filesystem writes, Git operations, or secret access from React.
- No global or cross-project task launcher, silent execution-route fallback, or fourth built-in workflow.
- No replacement for technical Agent services, Agent types, capability detection, or the unchanged sidebar Agent status foot.

The superseded Agent page, right panel, and generic Run Agent dialog were retired in Workflows Batch 8. Compatibility-only Agent concepts remain under their existing technical names.
