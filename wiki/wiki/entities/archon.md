---
title: Archon
created: 2026-05-22
updated: 2026-05-22
type: entity
tags: [tool, engineering, open-source]
sources:
  - raw/articles/2026-04-18-github-hot-10-open-source-projects.md
---

# Archon

## 概述

Archon 是 coleam00 开源的 AI 编程 harness builder，定位是让 AI 编码变得确定可重复的工作流引擎。目前 1.84 万 Star。

## 核心问题

AI 编程 Agent 每次运行结果不一致——同样一个任务今天跳过 plan 阶段，明天忘了写测试，后天又无视 PR 模板。Archon 用 YAML 把流程固定下来解决此问题。

## 核心特性

- **YAML 工作流定义**：用 YAML 将 AI 编码流程固定化，确保可重复
- **独立 git worktree**：每次工作流运行在独立 git worktree 中，多任务并行互不污染
- **可组合节点**：确定性 bash 脚本、测试节点 + AI 规划、代码生成节点可自由组合
- **17 个默认工作流**：feature 开发、issue 修复、PR review、重构等模板
- **多触发方式**：CLI、Web UI、Slack、Telegram、[[discord]]、GitHub 均可触发
- **团队对齐**：`.archon/workflows/` 目录提交到仓库，所有人流程对齐

## 在 Harness 生态中的定位

Archon 是 [[harness-engineering]] 在 AI 编码领域的典型实践——用结构化的工作流约束 AI Agent 的非确定性行为。它类似于 n8n 在通用自动化中的角色，但专注于 AI 编码场景。

## 相关链接

- [[harness-engineering]] — 构建 AI 模型能力脚手架的方法论
- [[multica]] — 另一种 Agent 任务管理方式（看板式）
- [[skills]] — Skill 模块化能力系统
- [[gsd2|GSD2]]
