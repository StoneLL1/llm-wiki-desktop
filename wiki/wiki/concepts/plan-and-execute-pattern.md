---
title: Plan-and-Execute Pattern
created: 2026-05-21
updated: 2026-05-21
type: concept
tags: [agent, architecture, methodology]
sources:
  - raw/articles/2026-04-18-build-ai-agent-framework.md
---

# Plan-and-Execute Pattern

## 定义

Plan-and-Execute 模式由 LangChain 团队于 2023 年 5 月提出，基于 Lei Wang 等的《Plan-and-Solve Prompting》论文和开源的 BabyAGI 项目。其核心思想是：**让 LLM 先制定完整的分步计划，再按步骤执行**，而非边做边想（[[react-pattern]]）。

## 工作流程

```
Planning → Task1 → Task2 → Task3 → Summary
```

Plan-and-Execute 强调结构化工作流程：
1. **Planning 阶段**：LLM 分析整体任务，制定多步执行计划
2. **Execution 阶段**：按计划逐步执行各个子任务
3. **Summary 阶段**：汇总所有结果，输出最终答案

## 与 ReAct 的对比

| 维度 | [[react-pattern]] | Plan-and-Execute |
|------|-------------------|-------------------|
| 决策方式 | 每步动态决策 | 先整体规划再执行 |
| 灵活性 | 高，可随时调整 | 低，倾向于固定 workflow |
| 适用场景 | 探索性、不确定性高的任务 | 复杂但依赖关系明确的长期任务 |
| 动态调整 | 天然支持 | 需要额外的 re-planning 机制 |

## 优势与局限

**优势**：
- 适合任务依赖关系明确的长期任务
- 执行过程可预测、可审计
- 便于 [[multi-agent-collaboration]] 中的任务分解

**局限**：
- 倾向于 workflow，缺乏动态调整能力
- 当计划与实际执行出现偏差时，可能需要频繁 re-plan
- 初始规划质量直接决定整体效果

## 历史背景

- **Plan-and-Solve Prompting 论文**：提出了让 LLM 先规划后执行的核心理念
- **BabyAGI 项目**：首个流行的任务驱动型自主 Agent，实现了"生成任务列表→执行→再规划"的 [[agent-loop]]
- 两者结合形成了 Plan-and-Execute 模式的基础

## See Also

- [[react-pattern]] — 边推理边行动的替代模式
- [[reflection-pattern]] — 自我反思改进模式
- [[agent-loop]] — Plan-and-Execute 的运行载体
- [[multi-agent-collaboration]] — 编排器模式中应用 Plan-and-Execute
- [[context-engineering]] — 管理规划过程中的上下文
- [[document-first-system]] — 规划驱动的开发方法论
