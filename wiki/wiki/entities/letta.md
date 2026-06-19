---
title: Letta (MemGPT)
created: 2026-05-21
updated: 2026-06-10
type: entity
tags: [tool, agent, methodology, open-source]
sources:
  - raw/articles/2026-05-21-xhs-agent-projects-recommendation.md
  - raw/articles/2026-06-09-agent-context-compression-strategies-comparison.md
---

# Letta (MemGPT)

## 概述

Letta（前身为 MemGPT）是一个开源的 Agent 运行时框架，22k+ Star，Apache 2.0 许可。核心特色是内置了强大的记忆管理系统。它让 AI Agent 能够超越上下文窗口的限制，实现长期记忆和状态持久化。

## 核心设计来源

Letta 的核心设计来自 **MemGPT 论文**：把 LLM 的 context window 当成虚拟内存来管理——就像操作系统管理物理内存一样，自动在有限的上下文窗口中换入换出信息。在 Letta 中，记忆系统是核心而非附加层。

## 核心特性

- **分层记忆系统**：
  - **核心记忆（Core Memory）**：始终在上下文中的关键信息
  - **归档记忆（Archival Memory）**：长期存储的外部记忆，按需检索
  - **召回记忆（Recall Memory）**：历史对话记录的搜索和检索
- **虚拟上下文管理**：类似操作系统的虚拟内存机制，自动在有限上下文窗口中换入换出信息
- **Agent 即服务**：Agent 作为持久化的后端服务运行
- **多模型支持**：支持 OpenAI、Anthropic 等多家 LLM

## 在 Agent 生态中的定位

Letta 填补了 [[agent-loop]] 中记忆管理的空白。大多数 Agent 框架（如 [[hermes-agent]]、[[openclaw]]）关注工具调用和任务执行，而 Letta 专注于解决 Agent 的长期记忆问题。

## 适用场景

- 长期对话 Agent
- 个人助理
- 知识管理 Agent
- 需要跨会话记忆的应用

## 上下文压缩策略

Letta/MemGPT 在上下文压缩方面独树一帜^[raw/articles/2026-06-09-agent-context-compression-strategies-comparison.md]——它不采用渐进压缩，而是将上下文完全按操作系统内存层次建模：

| 层级 | 类比 | 机制 |
|------|------|------|
| Core Memory | RAM | 始终在上下文中的关键信息，Agent 自主维护 |
| Archival Memory | 磁盘 | 外部向量存储，通过 `archival_memory_search` 按需检索换入 |
| Recall Memory | 缓存 | 对话历史搜索，通过 `conversation_search` 检索 |

**关键区别**：换入换出由 Agent 自己通过函数调用决定，不是被动截断。这意味着 Agent 可以主动决定"我现在需要什么信息"，而不是等系统替它裁剪。

**定位差异**：在 [[context-compression-pipeline]] 的六家横向对比中，Letta 是唯一不走"分层渐进压缩"路线的方案。它不解决单会话内的 token 压缩问题（22.5k Star、架构复杂度高、需要外部向量存储），而是专注解决跨会话的长期记忆问题——与 Codex 的 handoff summary、Claude Code 的五段流水线形成互补。详见 [[agent-memory-systems]]。

## 相关链接

- [[context-engineering]] — 上下文管理的系统方法论
- [[multi-agent-collaboration]] — 多 Agent 系统中的记忆共享
- [[harness-engineering]] — 记忆系统是 Harness 的重要组成
