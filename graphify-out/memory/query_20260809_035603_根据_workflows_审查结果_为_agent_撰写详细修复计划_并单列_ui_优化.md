---
type: "query"
date: "2026-08-09T03:56:03.536117+00:00"
question: "根据 Workflows 审查结果，为 Agent 撰写详细修复计划，并单列 UI 优化"
contributor: "graphify"
outcome: "useful"
source_nodes: ["TaskService", "WorkflowService", "ProjectLayout", "AgentService", "WorkflowAccessSnapshot"]
---

# Q: 根据 Workflows 审查结果，为 Agent 撰写详细修复计划，并单列 UI 优化

## Answer

计划按依赖拆为：基线与可计数 fixture；layout-aware preferences、retry ID、Markdown prerequisite 和 preparation cap；cancel/trust/dispatch 状态机；前端 canonical identity guard 与按需 confirmation hydration；事件刷新、隐藏 drawer、notification permission 和 route-local subscription 降载；证据驱动的 overview/progress 后端优化；有界 Preparation/History/Diff；最后独立完成 Overview、Preparation、Pipeline/Result、Right panel/a11y/i18n/responsive UI。Deep Lint Agent 保持 fail-closed，作为 credential/no-tools broker 的独立安全决策门。图谱查询扩展词：workflow, task, queue, event, refresh, performance, identity, trust, compatible, agent, pipeline, review。

## Outcome

- Signal: useful

## Source Nodes

- TaskService
- WorkflowService
- ProjectLayout
- AgentService
- WorkflowAccessSnapshot