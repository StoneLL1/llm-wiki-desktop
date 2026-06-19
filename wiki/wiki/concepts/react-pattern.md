---
title: ReAct Pattern
created: 2026-05-21
updated: 2026-05-21
type: concept
tags: [agent, architecture, methodology]
sources:
  - raw/articles/2026-04-18-build-ai-agent-framework.md
---

# ReAct Pattern

## 定义

ReAct（Reasoning + Acting）是当前 AI Agent 理论中最基础、最具代表性的行为模式。由 Yao 等人于 2022 年在论文《ReAct: Synergizing Reasoning and Acting in Language Models》中提出。其核心思想是将**推理（Reasoning）**和**行动（Acting）**相结合，弥补了纯 Chain-of-Thought 推理缺少外部交互的缺陷。

## 核心循环

ReAct Agent 的运作基于一个迭代循环，包含三个步骤：

1. **推理（Reasoning）**：依赖 LLM 分析当前任务状态，生产内部推理，决定下一步行动。核心思想是 CoT（Chain of Thought）。
2. **执行（Acting）**：根据推理结果执行具体操作——查询信息或调用外部工具（Function Call、[[mcp]]、Shell 命令、代码执行等）。
3. **观察（Observation）**：观察行动结果，将反馈用于下一轮思考；或判断已获得最终答案，整理输出。

## 与其他模式的关系

- ReAct 是 Agent 行为模式的**基础**，后续的 [[plan-and-execute-pattern]] 和 [[reflection-pattern]] 都是在 ReAct 之上的扩展与补充
- ReAct 强调"边做边想"，与 [[plan-and-execute-pattern]] 的"先想好再做"形成对比
- 当 ReAct 中加入自我评估环节时，即演化为 [[reflection-pattern]]

## 工程实现

在 [[agent-loop]] 的工程框架中，ReAct 模式直接映射为：

```
while not done:
    thought = llm_call(context)      # 推理
    action = parse_tool_calls(thought) # 行动决策
    result = execute_tools(action)     # 执行
    context.append(result)             # 观察并更新上下文
```

Anthropic 的 [[claude-code]]、[[openclaw]] 的 Pi Agent 等主流 Agent 框架的核心都遵循这一模式。

## 业界共识

当前主流 Agent 框架虽然有各种演绎与变形，但都离不开 ReAct 的核心思想：**将推理与执行结合起来**。Agent 框架的本质——推理（LLM Call）+ 执行（Tools Call）——没有变化，而连接两者的 [[context-engineering]] 则是智能的核心所在。

## See Also

- [[agent-loop]] — ReAct 模式的工程载体
- [[plan-and-execute-pattern]] — 先规划后执行的替代模式
- [[reflection-pattern]] — 在 ReAct 基础上增加自我反思
- [[codeact]] — 将代码执行作为行动空间的扩展
- [[context-engineering]] — 管理上下文以优化 ReAct 循环质量
- [[multi-agent-collaboration]] — 多个 ReAct Agent 的协作架构
- [[langchain]] — ReAct 模式的标杆框架实现


## 工程实现要点

ReAct 循环的三步工程化：
1. **推理（Reasoning）**：LLM 分析任务状态，生成内部推理（CoT），决定下一步
2. **执行（Acting）**：根据推理调用工具（Function Call、MCP、Shell、代码执行）
3. **观察（Observation）**：获取工具结果，反馈到下一轮思考

与 [[plan-and-execute-pattern]] 的区别：ReAct 边做边想，适合需要动态反馈的任务；Plan-and-Execute 先规划后执行，适合依赖关系明确的长期任务。

### Sources
- raw/articles/2026-04-18-build-ai-agent-framework.md
