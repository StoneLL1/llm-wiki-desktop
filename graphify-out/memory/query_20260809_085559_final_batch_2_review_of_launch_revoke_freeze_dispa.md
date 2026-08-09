---
type: "query"
date: "2026-08-09T08:55:59.464285+00:00"
question: "Final Batch 2 review of launch revoke freeze dispatch confirmation races"
contributor: "graphify"
outcome: "useful"
source_nodes: ["WorkflowService", "TaskService", "AppState"]
---

# Q: Final Batch 2 review of launch revoke freeze dispatch confirmation races

## Answer

Expanded from project graph workflow and dispatch nodes, then verified current source. One P1 remains: src-tauri/src/lib.rs production runner adapters discard reject_claimed_dispatch errors at lines 41-46 and discard dispatch_claimed_run errors for claimed continuation runs at lines 155-157, 250-252, and 345-347. This violates Batch 2 state-aware finalizer requirement that secondary errors not be swallowed and can strand a claimed run without observable terminal recovery. Launch publication guard is acceptable under the documented conservative whole-call contract; revoke closes under trust lock, cancels, releases locks, then waits, preventing post-return starts. Freeze/rebind aggregation and confirmation tuple/hydration-cancel serialization showed no additional actionable issue.

## Outcome

- Signal: useful

## Source Nodes

- WorkflowService
- TaskService
- AppState