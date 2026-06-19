---
title: Obsidian
created: 2026-05-17
updated: 2026-05-25
type: entity
tags:
  - tool
  - knowledge-management
sources:
  - raw/articles/2026-05-08-obsidian-coding-agent-long-term-memory.md
  - raw/articles/2026-05-24-jason-liu-getting-most-out-of-codex.md
  - raw/articles/2026-05-24-openai-codex-tips-jason-liu.md
---

# Obsidian

## 概述

Obsidian 是一款强大的知识管理工具，基于本地 Markdown 文件的笔记系统。在 AI 编程 Agent 的语境下，Obsidian 扮演了一个关键角色：**作为 Coding Agent 的长期记忆库**，弥补了大语言模型上下文窗口的固有限制。

## Obsidian CLI

Obsidian 官方提供了命令行工具（Obsidian CLI），使得 [[claude-code]] 等 AI 编程 Agent 可以直接从终端控制 Obsidian。这打通了 Agent 与知识管理系统之间的桥梁：

- Agent 可以读取 Obsidian 库中的笔记和知识
- Agent 可以创建和更新笔记
- Agent 可以搜索和组织知识库

通过 CLI 集成，Obsidian 从一个被动的知识存储变成了 Agent 工作流中的活跃组件。

## 三层记忆架构

借鉴认知科学中的记忆模型，可以构建一个三层记忆架构来增强 Coding Agent 的能力：

### 第一层：入口层

- **AGENTS.md / [[claude-md]]（CLAUDE.md）**：项目的入口配置文件
- 定义项目规则、约束和关键上下文
- 相当于 Agent 的"第一印象"，每次会话都会读取

### 第二层：长期记忆层

- **Obsidian 项目笔记**：结构化的项目知识库
- 存储设计决策、技术选型理由、经验教训等持久化知识
- Agent 按需查询，而非每次全部加载

### 第三层：会话日志层

- **sessions/YYYY-MM-DD.md**：按日期组织的会话日志
- 记录每次交互的关键信息和决策
- 支持跨会话的上下文恢复

## 认知科学模型

这一架构直接借鉴了认知科学的记忆模型：

| 认知概念 | Agent 对应 | 容量 | 持续性 |
|---------|-----------|------|--------|
| 工作记忆 | 上下文窗口 | 有限（128K-200K tokens） | 单次会话 |
| 长期记忆 | Obsidian 知识库 | 近乎无限 | 永久 |

上下文窗口相当于工作记忆——容量有限但访问速度极快；Obsidian 相当于长期记忆——容量近乎无限但需要主动检索。通过两者的配合，Agent 可以实现既高效又持久的知识管理。

## 解决的核心问题

### 跨会话失忆

大语言模型天然是无状态的——每次新会话都从零开始。Obsidian 作为长期记忆库，使得 Agent 可以在会话之间保留和恢复关键上下文，解决跨会话失忆问题。

### 上下文压缩失真

当上下文窗口接近容量上限时，[[harness-engineering]] 中的 Compaction 机制会压缩历史信息，但这可能导致信息失真。Obsidian 的长期记忆提供了一个可靠的"真相来源"，Agent 可以随时回溯原始记录。

## 自定义命令封装

可以将 Obsidian 操作封装为 [[claude-code]] 的自定义命令：

- **/init-memory**：初始化项目的 Obsidian 记忆结构，创建必要的目录和模板
- **/save-memory**：将当前会话的关键信息保存到 Obsidian 知识库

这种封装使得 Agent 与 Obsidian 的交互变得标准化和可复用。

## 相关概念

- [[claude-code]] — Anthropic 的 CLI 编程 Agent，与 Obsidian CLI 集成
- [[claude-md]] — Claude Code 的配置文件机制，三层记忆架构的入口层
- [[knowledge-compilation]] — 知识编译，将隐性经验转化为显性知识的过程
- [[openai-codex]] — Codex 官方推荐 Obsidian 作为 Shared Memory 存储方案
- [[jason-liu]] — Codex 团队，提出用 Obsidian vault 作为 Agent 持久工作记忆
