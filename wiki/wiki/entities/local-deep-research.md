---
title: Local Deep Research
created: 2026-05-27
updated: 2026-05-27
type: entity
tags: [tool, open-source, agent, research]
sources:
  - raw/articles/2026-05-17-8-github-open-source-projects.md
---

# Local Deep Research

## 概述

**Local Deep Research** 是一个完全本地运行的深度研究工具，作为 OpenAI Deep Research 的隐私替代方案。它使用 Qwen3.6-27B 模型在单张 RTX 3090 上，SimpleQA 准确率达到 95.7%。

## 核心特性

### 多搜索引擎支持
- **免费源**：arXiv、PubMed、Semantic Scholar、Wikipedia 等 10+ 种
- **付费源**：Google、Brave、Tavily 等可接入
- **自定义源**：本地文档和 [[langchain]] 向量库

### 研究策略
- 20+ 种研究策略可选
- `langgraph-agent`：自主智能体，性能最强
- `focused-iteration`：迭代精炼，准确率最高

### MCP 集成
提供 [[mcp]] Server，可直接从 [[claude-code]] 调用深度研究能力。

## 技术规格

| 指标 | 数值 |
|------|------|
| 基础模型 | Qwen3.6-27B |
| 硬件要求 | 单张 RTX 3090 |
| SimpleQA 准确率 | 95.7% |
| 搜索引擎支持 | 10+ 种 |
| 研究策略 | 20+ 种 |

## Relationships

- 作为 [[gpt-researcher]] 的本地替代方案
- 通过 [[mcp]] 与 [[claude-code]] 集成
- 使用 [[langchain]] 向量库作为搜索后端

## See Also

- [[gpt-researcher]] — 云端 AI 深度研究 Agent
- [[mcp]] — 工具连接协议
- [[claude-code]] — 可调用 MCP Server 的编程 Agent
