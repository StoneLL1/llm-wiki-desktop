---
title: CodeAct
created: 2026-05-21
updated: 2026-05-21
type: concept
tags: [agent, architecture, methodology]
sources:
  - raw/articles/2026-04-18-build-ai-agent-framework.md
---

# CodeAct

## 定义

CodeAct 是一种 Agent 设计架构，由 UIUC 的王星尧博士在 2024 年初论文《Executable Code Actions Elicit Better LLM Agents》中提出。其核心观点是：**通过生成可执行的 Python 代码来统一 LLM Agent 的行动空间**。

## 核心思想

传统 Agent 的 Acting 方式是 Function Call 和 [[mcp]] 工具调用。CodeAct 提出：

> **编程是解决通用问题的一种普适方法。**

Agent 不仅可以通过调用预定义工具完成任务，还可以**生成代码、执行代码**来解决问题。而且效果更好——代码作为一种形式化表达，比自然语言工具调用更精确、更可验证。

## 业界影响

### Manus 的实践

Manus 首席科学家 Peak 公开表示"Actually, Manus doesn't use MCP. Inspired by CodeAct"。Manus 选择 CodeAct 而非 MCP 作为 Agent 的行动方式，引发了广泛讨论。

### Anthropic 的采纳

2025 年 11 月，Anthropic 官方博客发布《Code execution with MCP: Building more efficient agents》，提出将 MCP 服务器作为代码 API（而非直接的工具调用），Agent 可以编写代码与 MCP 服务器交互。这种"代码驱动"的方式与 CodeAct 一脉相承。

### 业界共识

从 CodeAct 的发展可以得出 Agent 工程的两大共识：
1. **使用文件系统作为上下文**（如 OpenClaw 的 SOUL.md/TOOLS.md/MEMORY.md）
2. **编程是解决通用问题的普适方法**（问题→生成代码→执行代码→迭代→直到解决）

## 与其他模式的关系

CodeAct 是对 [[react-pattern]] 中"Acting"环节的**增强**：
- ReAct 的 Acting 可以是 Function Call、MCP 调用
- CodeAct 的 Acting 则是生成并执行代码
- 两者可以结合使用

Shunyu Yao（ReAct 作者）的观点与之呼应："人类最重要的 affordance 是手，而 AI 最重要的 affordance 可能是代码。"

## See Also

- [[react-pattern]] — CodeAct 的基础行为模式
- [[agent-loop]] — CodeAct 执行的运行框架
- [[mcp]] — CodeAct 的替代/互补工具调用协议
- [[context-engineering]] — 管理 CodeAct 过程中的代码上下文
- [[openclaw]] — 实践文件系统作为上下文的 Agent 平台
- [[skill-engineering]] — 将代码能力封装为可复用 Skill
