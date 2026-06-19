---
title: Mirage
created: 2026-05-27
updated: 2026-05-27
type: entity
tags: [tool, open-source, architecture]
sources:
  - raw/articles/2026-05-12-10-new-open-source-github-projects.md
---

# Mirage

## Overview

Mirage 是一个开源的统一虚拟文件系统，专为 AI Agent 设计。它将 Google Drive、Slack、Gmail、Redis、GitHub、Notion、Linear、Trello、Discord、Telegram、MongoDB、SSH 等多种服务统一挂载到同一个虚拟目录树下，让 Agent 只需 ls、cat、grep、cp 等基础 Unix 命令就能跨服务操作。

上线一天突破 1000+ Star。

## 核心特性

- **统一虚拟目录树**：所有后端服务挂载到同一目录结构
- **标准 Unix 命令**：Agent 用 ls/cat/grep/cp 即可操作所有服务
- **多 SDK 支持**：Python SDK、TypeScript SDK、独立 CLI 工具
- **框架适配**：内置 OpenAI Agents SDK、Vercel AI SDK、[[langchain]]、Pydantic AI 等适配层
- **可嵌入**：直接嵌入 FastAPI、Express 或浏览器应用

## 安装

```bash
uv add mirage-ai
npm install @struktoai/mirage-node
```

## 与 MCP 的关系

Mirage 解决的核心痛点是 Agent 访问多后端的统一接口问题，与 [[mcp]] 形成互补：MCP 提供工具调用协议，Mirage 提供统一的文件系统语义。

## 相关链接

- [[mcp]] — 工具调用协议，与 Mirage 互补
- [[mcp-ecosystem]] — MCP 生态更新
- [[langchain]] — Mirage 内置适配的 Agent 框架
