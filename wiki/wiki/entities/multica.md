---
title: Multica
created: 2026-05-22
updated: 2026-05-24
type: entity
tags: [tool, multi-agent, open-source]
sources:
  - raw/articles/2026-04-18-github-hot-10-open-source-projects.md
  - raw/articles/2026-04-21-github-top10-weekly-stars.md
---

# Multica

## 概述

Multica 是 Linear + AI Agent 的组合工具，将编程 Agent 变成真正的团队成员。目前 1.47 万 Star。用户像给同事派活一样在看板上分配任务，Agent 自己执行、报告进度、更新状态。

## 核心特性

- **任务全生命周期管理**：Agent 作为团队成员参与任务分配、执行、进度报告和状态更新
- **WebSocket 实时进度流**：实时查看 Agent 工作进展
- **独立隔离**：每个 workspace 独立隔离
- **混合 runtime**：支持本地 daemon 和云端 runtime 混用
- **多 CLI 兼容**：[[claude-code]]、[[openai-codex]]、OpenCode、Gemini、[[cursor]] Agent 等
- **Skill 沉淀**：解决方案自动沉淀成可复用 Skill，团队能力随使用增长

## 在 Agent 协作生态中的定位

Multica 解决的核心问题是：让 [[multi-agent-collaboration]] 从"对着终端复制粘贴"变成真正的项目管理。与 [[archon]] 的 YAML 工作流不同，Multica 采用看板式任务管理，更贴近人类团队协作习惯。

## 相关链接

- [[claude-code]] — Multica 兼容的主要编码 Agent
- [[multi-agent-collaboration]] — 多 Agent 协作范式
- [[archon]] — AI 编码工作流引擎
- [[rowboat|Rowboat]]
