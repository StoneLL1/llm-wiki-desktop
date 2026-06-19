---
title: Agent Teams
created: 2026-05-17
updated: 2026-06-10
type: entity
tags:
  - agent
  - multi-agent
sources:
  - raw/articles/2026-04-27-claude-code-agent-teams-best-practices.md
  - raw/articles/2026-04-18-claude-code-hidden-commands.md
  - raw/articles/2026-05-29-claude-opus-48-dynamic-workflow一次性并行上百个subagents.md
  - raw/articles/2026-06-08-claude-code-parallel-agents-comparison.md
---

# Agent Teams

## 概述

Agent Teams 是 [[claude-code]] 的多 Agent 并行协作功能，允许多个 AI Agent 组成团队共同完成复杂任务。这一功能代表了 AI 辅助开发从单 Agent 模式向多 Agent 协作模式的演进。

## 与 Subagents 的对比

| 维度 | Subagents | Agent Teams |
|------|-----------|-------------|
| 通信 | 只向主代理汇报 | 队友之间直接收发消息 |
| 协调 | 主代理统一管理 | 共享任务列表 + 自协调 |
| Context | 自己的窗口，结果回主会话 | 自己的窗口，完全独立 |
| Token 成本 | 较低（结果汇总） | 较高（每个队友是独立 Claude 实例） |
| 适用场景 | 结果导向的专注任务 | 需要讨论和质疑的复杂工作 |

**比喻**：Subagents 是实习生各自给领导交作业，Agent Teams 是小组同事坐一起能互相讨论。

## 前置条件

1. [[claude-code]] v2.1.32 以上
2. 在 `~/.claude/settings.json` 中启用环境变量：
```json
{
  "env": {
    "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS": "1"
  }
}
```

## 触发方式

prompt 中必须**显式**出现以下关键词，Claude 才会走 Agent Teams 分支：
- `agent team`
- `teammate`
- `team lead`
- `shared task list`

⚠️ 注意：使用 `sub-agent`、`subagent`、`delegate` 等词可能被识别为 subagents 模式，不会创建 team。

**判断是否真的开团**：执行后看 `~/.claude/teams/` 目录下有没有新生成的 team 目录（带 `config.json`）。有目录才是真开团。

## 核心机制

### 共享任务列表

任务有三种状态：pending → in progress → completed。可设置依赖关系（blocked by）。两种分配方式：
- Lead 显式指派
- 队友完成自己的任务后自动认领下一个未分配的

### Plan-approval 机制

对高风险任务，在 prompt 里加 `"Require plan approval before any changes"`。被拒绝的队友会留在 plan 模式，按反馈修订重新提交。关键是在 prompt 里给 lead 写清批准准则。

### Hooks 质量门

三个 hook 事件：`TeammateIdle`（队友空闲时）、`TaskCreated`（任务创建时）、`TaskCompleted`（任务完成时）。在 hook 里以 exit 2 退出可阻断行为，stderr 作为反馈传给 teammate。比 prompt 软约束更可靠。

### 显示模式

- **In-process**：所有队友跑在主终端，`Shift+Down` 循环切换
- **Split panes**：每个队友独立窗格，需 tmux 或 iTerm2 支持

启用方式：`claude --teammate-mode tmux` 或在 `~/.claude.json` 设 `"teammateMode": "tmux"`。

## 最强用例

- **研究和审查**：多队友同时调查不同方面，互相分享和质疑发现
- **新模块开发**：每个队友拥有独立部分，不互相干扰
- **带竞争假设的 Bug 调试**：多队友各拿一个理论并行测试，对抗锚定偏见
- **跨层协调**：前端、后端、测试改动同时进行

**不适合**：顺序依赖强的任务、同一个文件多人编辑、依赖关系特别多的工作。

## 最佳实践

- **团队规模 3-5 人**：甜蜜点是 3-5 个队友做 5-6 个任务
- **模型选择**：Lead 用 Opus（最强模型），Teammates 用 Sonnet/Haiku 控制成本
- **任务粒度适中**：独立可完成，有明确输入输出
- **依赖管理显式**：在初始 prompt 中说清哪些任务依赖哪些
- **给队友足够 context**：队友不继承 lead 的对话历史，关键信息写在 prompt 里
- **团队跑完记得清理**：`Clean up the team` 释放资源

## 社区实践（卡尔的AI沃茨）

来自社区的实战经验：

### 创建方式

用 `/agents` 命令用大白话创建自定义子代理，可以配置：
- 使用什么模型驱动（Opus/Sonnet/Haiku）
- 只读 Agent 还是执行 Agent
- 开放全部 Tools 还是部分

### Agent Teams 的协作模式

不只是主 Agent 和临时工的关系，而是真正的多 Agent 团队。一次性创建多个不同角色的 Agent 并行工作：

> 「我正在设计一个 CLI 工具，帮助开发者追踪整个代码库里的 TODO。组建一个 Agent 团队：一个负责用户体验，一个负责技术架构，另一个专门负责挑刺。」

Agent 会互相讨论、分享发现，甚至给对方的观点挑毛病。用户可以像项目经理一样提前结束某个队友的对话、二次分配任务，或让它们先出方案等批准再执行。

### 实测案例

**案例一：一句话调研开团**
给一个调研任务 + 角色分工，Claude 自动创建 team lead + 多个 teammates，各跑各的方向，最后 lead 综合输出。

**案例二：tmux 分窗并行**
在 tmux 下启用 Agent Teams，多个 teammates 自动分窗显示，每个队友有完整终端视图。

### 与 Subagent 的区别

- **Subagent**：临时工模式，干完就消失，适合搜资料、翻日志等脏活
- **Agent Teams**：团队协作模式，队友间可直接通信，适合复杂问题的多角度探索
- **建议**：先用好单个 Agent（掌握 `/btw`、Hook、`/loop` 等），再上多 Agent

## 四种并行方案对比汇总

鲁工在 2026-06-08 的文章中，将 [[claude-code]] 的并行 Agent 能力系统梳理为四种方案^[raw/articles/2026-06-08-claude-code-parallel-agents-comparison.md]。Anthropic 官方文档将并行 Agents 标签放在文档最开头（优先级高于 MCP、Skills、Plugins），表明其战略重要性。

### 四种方案对比

| 维度 | Subagents | Agent View | Agent Teams | Dynamic Workflows |
|------|-----------|------------|-------------|-------------------|
| **谁来协调** | Claude 在当前会话里委托并收结果 | 用户自己分派、回头再看 | Claude 当 leader 带一队人 | JavaScript 脚本编排 |
| **通信方式** | 只向主会话汇报 | 只向用户汇报 | 队友间直接通信 | 脚本变量 + 最终汇总 |
| **文件隔离** | 独立 context，不自动隔离文件 | 自动 worktree 隔离 | **不自动隔离**，需手动切分 | 由脚本控制 |
| **规模** | 并发 3-5 个 | 多个后台会话 | 建议 3-5 个队友 | 最多 16 并发、1000 总计 |
| **状态** | 稳定功能 | Research Preview | Experimental（需手动开启） | Research Preview |
| **适用场景** | 结果导向的专注任务（探索、调研） | 互不相关的多任务并行 | 需要讨论/质疑的复杂协作 | 大规模可重复编排（全库扫描、迁移） |

### 三问决策框架

Claude Code 官方建议的判断逻辑：

1. **谁来协调这摊子活？** Claude 在一个对话里委托 → Subagents；甩出去回头看 → Agent View；Claude 当 leader → Agent Teams；写成脚本反复跑 → Dynamic Workflows
2. **干活的之间要不要互相通信？** 只有 Agent Teams 的队友能直接通信。需要协作讨论的任务才上 Agent Teams
3. **会不会动到同一批文件？** 会的话用 worktree 隔离。Subagents 和 Agent View 都能搭配 worktree，但 Agent Teams 不自动隔离，需手动切分任务

### Worktrees 和 /batch 澄清

以下**不是**独立的并行 Agent 方法：
- **Worktrees（git 工作树）**：解决文件冲突，不负责分活。用 `claude --worktree feature-auth` 起隔离会话，是手动开并行的基础设施
- **`/batch`**：本质是 Subagents + worktree 的打包用法（拆成 5-30 个带 worktree 隔离的 subagents，各开一个 PR），算不上新的协调风格
- **后台 bash 命令**：不阻塞对话但没派出 Agent
- **Forked subagent**：让子代理继承当前完整上下文，是派 subagent 的方式
- **Routine**：定时云端跑会话，不在本机并行

## Relationships

- [[claude-code]] — Agent Teams 是 Claude Code 的核心功能之一
- [[anthropic]] — Agent Teams 由 Anthropic 团队设计和维护
- [[multi-agent-collaboration]] — 多 Agent 协作的广义概念
- [[context-engineering]] — 每个 teammate 有独立 context window
- [[claude-code-dynamic-workflow]] — 2026-05 发布的 Dynamic Workflow 是 Agent Teams 的下一阶段演进：从「Claude 逐轮决策小团队」到「JS 脚本编排 100+ 并行 Agents 交叉验证」

## See Also

- [[claude-code]] — 运行平台
- [[claude-code-dynamic-workflow]] — 四种并行方案中的脚本化大规模演进
- [[agent-loop]] — Agent 核心运行机制，并行方案构建在其之上
- [[multi-agent-collaboration]] — 更广义的多 Agent 理论
- [[claude-code-session-management]] — 会话管理策略
