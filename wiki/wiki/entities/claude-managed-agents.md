---
title: Claude Managed Agents
created: 2026-05-22
updated: 2026-05-22
type: entity
tags: [tool, agent, company]
sources:
  - raw/articles/2026-05-11-x-how-to-build-first-ai-agent-10k-plus.md
---

# Claude Managed Agents

## Overview

**Claude Managed Agents** 是 [[anthropic]] 于 2026 年 4 月 8 日推出的 AI Agent 基础设施层，让用户无需管理服务器、编写 [[agent-loop]] 或配置沙盒即可构建、部署和运行完全自主的 AI Agent。

## Core Capabilities

### 开箱即用
- **Cloud-hosted containers**：云端安全容器运行 Agent
- **Pre-built tools**：预置 bash 命令、文件操作、网页浏览、代码执行工具
- **Persistent file systems**：Agent 跨会话记住操作历史
- **Built-in memory**：Agent 随时间自我改进
- **Multi-agent orchestration**：最多 20 个专业 Agent 并行协作处理单个任务

### Multi-Agent Orchestration
2026 年 5 月 6 日（Code with Claude 活动）上线，支持最多 20 个专业化 Agent 并行处理单个问题。

### Dreaming
Agent 在会话之间自我改进，2026 年 5 月 6 日发布。

### Routines
自主调度工作流，目前处于 research preview 阶段。与 [[claude-code]] 的 Routines 功能配合，Agent 可在 [[anthropic]] 云基础设施上定时运行，无需用户电脑开机。

## Agent 构建方法论

Claude Managed Agents 推广了一套 7 步 Agent 构建方法论：

1. **明确单一任务**：Agent 应该只做一件具体、可重复的事情
2. **定义角色（System Prompt）**：像写招聘 JD 一样定义 Agent 的身份、成功标准、边界和异常处理
3. **设置 Agent**：通过 Claude.ai Cowork（零代码）或 Claude API（高级功能）
4. **提供工具**：[[mcp]] 连接器是关键扩展点，可连接 Slack、Google Drive、GitHub 等
5. **测试迭代**：5-10 次迭代从"大致可用"到"可靠优秀"
6. **调度自动化**：设置定时任务让 Agent 7×24 运行
7. **扩展规模**：构建多个 Agent 形成系统

### 初学者三大错误
1. 让 Agent 做太多事情（应该只做一件事）
2. 没给够上下文（System Prompt 太简略）
3. 不迭代（一次失败就放弃）

## 关键时间线

| 日期 | 事件 |
|------|------|
| 2026-04-08 | Claude Managed Agents 发布 |
| 2026-05-06 | Multi-agent orchestration 上线 |
| 2026-05-06 | Dreaming 功能发布 |
| Research Preview | Routines（自主调度工作流） |

## Relationships

- 由 [[anthropic]] 开发和运营
- 基于扩展 [[claude-code]] 和 Claude API 生态
- 使用 [[mcp]] 作为外部服务连接协议
- 代表 [[multi-agent-collaboration]] 的托管化实现
- 与 [[harness-engineering]] 理念一致——平台管理 harness，用户专注 Agent 行为

## See Also

- [[anthropic]] — Claude 生态的创建者
- [[claude-code]] — Anthropic 的 CLI 编码 Agent
- [[multi-agent-collaboration]] — 多 Agent 协作范式
- [[mcp]] — Agent 工具连接协议
- [[agent-loop]] — Agent 的核心运行模式
