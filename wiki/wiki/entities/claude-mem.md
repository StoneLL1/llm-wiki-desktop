---
title: claude-mem
created: 2026-05-22
updated: 2026-05-24
type: entity
tags: [tool, agent, open-source]
sources:
  - raw/articles/2026-04-18-github-hot-10-open-source-projects.md
  - raw/articles/2026-04-21-github-top10-weekly-stars.md
---

# claude-mem

## 概述

claude-mem 解决 [[claude-code]] 每次开新会话就失忆的问题，提供自动的跨会话记忆能力。目前 6 万 Star。底层使用 SQLite + Chroma 向量库，完全本地运行。

## 核心特性

- **自动捕捉与压缩**：会话进行时自动捕捉所有操作，用 Claude agent-sdk 做 AI 语义压缩
- **上下文注入**：新会话启动时自动将相关上下文注入回来
- **渐进式披露**：每层记忆调用的 token 成本都标出，不会默默烧 API 费用
- **本地 Web Viewer**：`localhost:37777` 浏览器查看历史记录
- **隐私控制**：`<private>` 标签圈住不想被记住的内容
- **本地存储**：SQLite + Chroma 向量库，数据不出电脑
- **也支持 Gemini CLI**
- **一行安装**：`npx claude-mem install`

## 在记忆系统生态中的定位

claude-mem 是 [[agent-memory-systems]] 在 Claude Code 生态中的具体实现。与 [[letta]]（原 MemGPT）的虚拟内存管理不同，claude-mem 更轻量，作为 Claude Code 的插件运行，专注于自动化的语义压缩和上下文注入。

## 相关链接

- [[agent-memory-systems]] — Agent 记忆系统的设计方案
- [[claude-code]] — claude-mem 的运行平台
- [[letta]] — 另一个 Agent 记忆系统（MemGPT）
