---
type: "codebase"
date: "2026-08-09T21:11:03.152364+00:00"
question: "Batch 5C 中 workflow progress persistence 与 barrier 写入路径在哪里，怎样保持事件、队列与磁盘顺序？"
contributor: "graphify"
outcome: "useful"
source_nodes: ["TaskService::update_workflow_stage_progress", "TaskService::mutate_workflow", "WorkflowStageSink::progress"]
---

# Q: Batch 5C 中 workflow progress persistence 与 barrier 写入路径在哪里，怎样保持事件、队列与磁盘顺序？

## Answer

WorkflowStageSink progress 进入 TaskService::update_workflow_stage_progress，再由 TaskService 的 workflow persistence lane 串行 revisioned snapshot；ObservationalProgress 使用 250ms window 和 trailing flush，Barrier 必须原子持久化成功后才发布事件或返回。

## Outcome

- Signal: useful

## Source Nodes

- TaskService::update_workflow_stage_progress
- TaskService::mutate_workflow
- WorkflowStageSink::progress