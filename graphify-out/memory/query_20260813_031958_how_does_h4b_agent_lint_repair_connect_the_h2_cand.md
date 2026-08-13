---
type: "architecture"
date: "2026-08-13T03:19:58.531039+00:00"
question: "How does H4B Agent lint repair connect the H2 candidate bridge to workflow queue task history recovery, ConfirmationRegistry, app-owned receipts, Compile manifest Diff apply, and Git checkpoint rollback?"
contributor: "graphify"
outcome: "useful"
source_nodes: ["AgentLintRepairRunner", "WorkflowCoordinator", "TaskService", "ConfirmationRegistry", "CompileService", "GitService", "SettingsService", "AgentService"]
---

# Q: How does H4B Agent lint repair connect the H2 candidate bridge to workflow queue task history recovery, ConfirmationRegistry, app-owned receipts, Compile manifest Diff apply, and Git checkpoint rollback?

## Answer

The H4B AgentLintRepair runner is registered as a Health operation subtype and reuses the existing WorkflowCoordinator queue, TaskService persistence/history/recovery, ConfirmationRegistry exact candidate decisions, CompileService manifest classification and checked apply, GitService scoped checkpoints/rollback, and AgentService pinned repair invocation. An app-owned SettingsService attestation binds the project-owned descriptor digest, cumulative pre/post mutation WAL, terminal typed result/status, and final commit. Only deterministic lint evidence closes findings; semantic findings remain unresolved/manual after at most three rounds. Waiting recovery requires both strict descriptor self-consistency and the exact app-owned receipt; terminal/manual results retain attested lazy Diff and Git rollback.

## Outcome

- Signal: useful

## Source Nodes

- AgentLintRepairRunner
- WorkflowCoordinator
- TaskService
- ConfirmationRegistry
- CompileService
- GitService
- SettingsService
- AgentService