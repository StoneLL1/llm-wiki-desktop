---
title: Thariq Shihipar
created: 2026-06-04
updated: 2026-06-04
type: entity
tags: [person, company]
sources:
  - raw/articles/2026-06-03-claude-code-dynamic-workflow-harness.md
  - raw/articles/2026-06-03-claude-workflow-harness-design-patterns.md
---

# Thariq Shihipar

## Overview

**Thariq Shihipar** is an engineer at [[anthropic]] and the primary author of the official Dynamic Workflow blog post *"A harness for every task: Dynamic Workflows in Claude Code"* (co-authored with Sid). He is a key figure in Claude Code's agent orchestration architecture, alongside [[lance-martin]] and [[boris-cherny]].

## Contributions

### Dynamic Workflow

Thariq 是 [[claude-code-dynamic-workflow|Claude Code Dynamic Workflow]] 功能的主要设计者和推广者。他在 2026 年 5 月的官方博客中系统阐述了：

- **单上下文三大顽疾**：Agentic Laziness（偷懒）、Self-preferential Bias（自我偏袒）、Goal Drift（目标漂移）
- **六种编排模式**：Fan-out-and-Synthesize、Adversarial Verification、Classify-and-Act、Generate-and-Filter、Tournament、Loop Until Done
- **核心 API**：`agent()`、`parallel()`、`pipeline()`
- **十种应用场景**：从迁移重构到模型路由
- **克制使用原则**：「这个任务真的需要更多算力吗？——Workflow 是用 token 换可靠性、对抗性和并发规模」

### Session Management

Thariq 此前还提出了 [[claude-code-session-management|Claude Code 会话管理的「五条岔路」决策框架]]：继续对话、Rewind、/clear、Compact、子 Agent——这是在 [[context-engineering]] 领域的系统性实践贡献。

## See Also

- [[claude-code-dynamic-workflow]] — 他主导设计的 Dynamic Workflow 功能
- [[claude-code]] — Claude Code 编程 Agent
- [[anthropic]] — Anthropic 公司
- [[lance-martin]] — Anthropic 同时期 Harness 工程师
- [[boris-cherny]] — Claude Code 创造者
- [[claude-code-session-management]] — 会话管理框架
