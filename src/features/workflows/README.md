# Workflows

Project-scoped presentation for the three fixed product workflows: Update Wiki, Health Check, and Generate Content.

## Ownership

- `useWorkflowsController.ts` owns typed IPC calls, stale-request guards, and `workflow://updated` event merging.
- `workflowStore.ts` is the project-scoped presentation state; it must be reset on project switches.
- Views render structured workflow DTOs and project-scoped run history. Backend services remain the authority for project identity, access, trust, route selection, queueing, Git policy, confirmation, and mutation.
- Settings owns Agent and Provider configuration. Lint owns repairs. Exports owns generated-artifact records and previews. The generic task drawer owns raw logs.

## Non-goals

- No arbitrary prompts, shell commands, filesystem writes, Git operations, or secret access from React.
- No global or cross-project task launcher, silent execution-route fallback, or fourth built-in workflow.
- No replacement for technical Agent services, Agent types, capability detection, or the unchanged sidebar Agent status foot.

The superseded Agent page, right panel, and generic Run Agent dialog were retired in Workflows Batch 8. Compatibility-only Agent concepts remain under their existing technical names.
