---
title: OpenAI Codex
created: 2026-05-19
updated: 2026-05-21
type: entity
tags:
  - tool
  - agent
  - open-source
sources:
  - https://github.com/openai/codex
  - raw/articles/2026-04-18-multi-ai-sdd-coding-practice.md
---

# OpenAI Codex

## 概述

OpenAI 的 Codex 是基于 GPT 的代码生成 AI 代理，可通过 CLI 运行，支持 Goals 结构化任务管理、沙盒执行、多代理协作。2026 年最新版本支持多模型切换和 benchmark 测量。

## 核心特性

- **CLI 运行**：在终端中执行，与 [[claude-code]] 类似
- **沙盒执行**：代码在隔离环境中运行，确保安全
- **Goals 结构化任务**：将复杂任务分解为结构化的 Goals
- **多模型切换**：支持不同 GPT 模型间的切换

## 在多 AI 协同工作流中的角色

在多 AI SDD（[[spec-driven-development]]）编码实践中，Codex 担任"执行者"角色：

- **Claude**（[[claude-code]]）编写规格文档（Spec）
- **Codex** 按 Spec 执行编码，严格遵循规格
- **Gemini** 负责代码审查和测试

这种"铁三角"模式利用了 Codex 的精确执行能力和 Claude 的架构设计能力。

## CodexMCP 集成

CodexMCP 是将 Codex 与 [[mcp]] 协议集成的工具，使得其他 Agent 可以通过 MCP 协议调用 Codex 的编码能力。这对于 [[multi-agent-collaboration]] 场景特别有价值。

## 相关链接

- [[claude-code]] — Anthropic 的 CLI 编码 Agent，主要竞争者
- [[computer-use-agent]] — 计算机使用 Agent
- [[spec-driven-development]] — Codex 参与的 SDD 工作流
- [[multi-agent-collaboration]] — 多 Agent 协作模式
