# Workflows

Project-scoped presentation for the three fixed product workflows: Update Wiki, Health Check, and Generate Content.

- `useWorkflowsController.ts` owns typed IPC calls, stale-request guards, and `workflow://updated` event merging.
- `workflowStore.ts` is the project-scoped presentation state; it must be reset on project switches.
- Views render structured workflow DTOs only. They do not invoke arbitrary prompts, shell commands, filesystem writes, Git operations, or provider secrets.
- The legacy Agent surface remains an internal compatibility route until its later removal batch.
