---
title: Rowboat
created: 2026-05-22
updated: 2026-05-22
type: entity
tags: [tool, multi-agent, open-source]
sources:
  - raw/articles/2026-04-18-github-hot-10-open-source-projects.md
---

# Rowboat

## 概述

Rowboat 是 YC 孵化的开源多 Agent 系统可视化 IDE，目前 1.2 万 Star。提供 Copilot 辅助生成 Agent，用户用自然语言描述即可自动搭建多 Agent 工作流。

## 核心特性

- **可视化 IDE**：图形化界面搭建多 Agent 系统
- **Copilot 辅助**：自然语言描述需求，自动生成 Agent 工作流
- **AI 模拟测试**：搭建完成后可在 AI 模拟场景中测试
- **MCP + 工具集成**：支持 MCP server 和各种工具的接入
- **多服务对接**：Slack、Linear、Jira、GitHub、ElevenLabs、Exa 等
- **双集成方式**：Python SDK 和 HTTP API 均可集成到自有产品
- **底层基于 OpenAI Agents SDK**

## 在多 Agent 生态中的定位

Rowboat 降低了 [[multi-agent-collaboration]] 的门槛——无需编写代码即可搭建多 Agent 系统。与 [[oh-my-claudecode]]、[[oh-my-codex]] 等编码 Agent 编排工具不同，Rowboat 更面向通用业务场景（AI 客服、自动化调研、内部工作流）。

## 相关链接

- [[multi-agent-collaboration]] — 多 Agent 协作范式
- [[mcp]] — Model Context Protocol 工具集成
- [[openclaw]] — 另一个多 Agent 平台
