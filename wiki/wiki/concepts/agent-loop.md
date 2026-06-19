---
title: Agent Loop
created: 2026-05-21
updated: 2026-05-21
type: concept
tags: [agent, architecture, engineering]
sources:
  - raw/articles/2026-04-18-build-ai-agent-framework.md
---

# Agent Loop

## 定义

Agent Loop 是 AI Agent 框架的核心运行机制——本质上是一个 **While 循环**，每一次迭代是一次 LLM 推理外加工具调用和上下文处理。所有 Agent 行为的发生都在这个循环内，直到任务完成退出。

## 核心流程

```
初始上下文（系统提示词 + 用户请求）
    ↓
[Agent Loop 开始]
    ↓
Agent 读取上下文 → 思考 → 决定行动
    ↓
执行工具/行动 → 获得结果
    ↓
结果追加到上下文
    ↓
[循环继续或结束]
```

### 单次迭代（Turn）

每次 Turn 包含：
1. **LLM Call**：基于当前上下文进行推理
2. **Tool Call 解析**：解析 LLM 响应中的工具调用请求
3. **Tool 执行**：调用对应的工具函数
4. **上下文更新**：将工具结果追加到 messages 列表

## 三大工程要素

Agent Loop 的设计核心是 [[context-engineering]]，由三个部分组成：

### 1. LLM Call
API 管理层，兼容各大 LLM 厂商的 API 实现细节及流式输出。LiteLLM 库是这个领域的佼佼者。

### 2. Tools Call
LLM 使用外部工具的能力，主流形式包括：
- 文件操作（读/写/编辑）
- Shell 命令 / 代码执行
- API / [[mcp]] 调用

### 3. Context Engineering
- **狭义**：提示词工程（Rules、CLAUDE.md、AGENTS.md 等）
- **广义**：包含 Tools 使用（如 [[skills]] 是工具与提示词结合的典范）

## 安全设计

- 设置迭代安全上限（如 MAX_TURNS = 20）
- 终止条件：LLM 不再调用工具时退出
- 超时保护：单次工具执行设 timeout

## 与 Agent 框架的关系

> Agent 框架设计的核心就是在 Agent Loop 这个 While 循环中设计如何管理上下文。

- [[openclaw]] 的 Pi Agent 仅用 4 个核心工具（shell_exec, file_read, file_write, python_exec）即可构建完整的 Agent
- [[claude-code]] 的 [[agent-loop]] 支持更丰富的工具生态和 Subagent 模式
- [[hermes-agent]] 的 [[agent-loop]] 支持多 Profile 隔离

## See Also

- [[react-pattern]] — Agent Loop 的基础行为模式
- [[context-engineering]] — Agent Loop 中上下文管理的系统方法
- [[multi-agent-collaboration]] — 多个 Agent Loop 的协作
- [[skills]] — 扩展 Agent Loop 能力的模块化单元
- [[mcp]] — Agent Loop 中工具调用的标准协议
- [[openclaw]] — 实现了极简 Agent Loop 的开源平台
- [[agent-learning-roadmap|Agent 学习路线]]


## 从零构建视角（yabohe / 腾讯技术工程）

Agent Loop 本质是一个 While 循环，每一次迭代包含：
1. **LLM Call** — API 调用推理
2. **Tools Call** — 解析响应、执行工具
3. **Context Update** — 结果追加到 messages

极简实现仅需 ~279 行代码（Python），核心组件：
- `agent_loop()` 函数：`while turn < MAX_TURNS` 循环
- 4 个基础工具：`shell_exec`、`file_read`、`file_write`、`python_exec`
- `TOOLS` 注册表：`name → {function, OpenAI function schema}`
- `messages` 列表作为上下文载体（OpenAI chat 格式）

关键设计参数：
- `MAX_TURNS = 20`（安全上限）
- 终止条件：LLM 返回无 `tool_calls`
- 使用 DeepSeek `deepseek-chat` 模型（兼容 OpenAI SDK + 支持 Tool Calls）

> 框架之外，上下文工程是智能的核心。框架提供基础工具，上下文工程提供环境，搭配 Skills，Agent 就能发挥巨大潜力。

### Sources
- raw/articles/2026-04-18-build-ai-agent-framework.md（从零开始设计实现一个 AI Agent 框架，yabohe/腾讯技术工程）
