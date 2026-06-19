---
title: AI Agent 学习路线
created: 2026-05-23
updated: 2026-05-27
type: concept
tags: [tutorial, methodology, agent]
sources:
  - raw/articles/2026-05-21-datawhale-ai-agent-learning-roadmap.md
---

# AI Agent 学习路线

## 概述

AI Agent 学习路线是由 Datawhale（陈思州）整理的系统性 Agent 学习资源，配套开源仓库 [Agent Learning Hub](https://github.com/datawhalechina/Agent-Learning-Hub)。核心主张：当前更值得投入的不是老式"角色扮演多 agent 框架"，而是更贴近真实生产力的方向。

## 学习阶段

### Part 1: 入门 — Agent 基本功

| 阶段 | 主题 | 关键内容 |
|------|------|---------|
| 0 | 理解 Agent | autonomy、tool use、planning、memory |
| 1 | 最小 Agent Loop | 手写 [[react-pattern]] loop，observe→think→act 循环 |
| 2 | 工具调用/RAG/记忆 | Function Calling、[[rag]]、短期/长期记忆 |

### Part 2: 进阶 — 能跑能上线

| 阶段 | 主题 | 关键内容 |
|------|------|---------|
| 3 | 现代 Agent Harness | [[claude-code]]、[[openclaw]]、[[hermes-agent]] 等 |
| 4 | [[multi-agent-collaboration]] | orchestrator-worker、pipeline、debate 模式 |
| 5 | Skills/协议 | [[mcp]]、A2A、[[skills]] |
| 6 | Browser/CUA | [[browser-use]]、[[computer-use-agent]] |

### Part 3: 工程化 — 真的能用

| 阶段 | 主题 | 关键内容 |
|------|------|---------|
| 7 | 评测/安全 | Eval、Trace、Prompt Injection 防御 |
| 8 | 部署上线 | 容器化、API 化、CI/CD for Agent |

## 推荐项目

- [[claude-code]] — Anthropic CLI 编码 Agent
- [[openclaw]] — 开源多 Agent 平台
- [[hermes-agent]] — NousResearch Agent 平台
- learn-claude-code、claw0、hello-agents、DeerFlow、smolagents 等

## 学习原则

1. 先动手，再深读
2. 宁可做小的可靠 agent，也不做炫的 demo
3. 工具用严格 schema
4. 加 agent 前先加 eval
5. 重要的运行都留 trace
6. 把 multi-agent 当协调问题，不是魔法
7. 危险操作留人在 loop 里
8. 尊重平台规则、版权和数据访问边界

## 相关链接

- [[agent-loop]] — Agent 核心运行机制
- [[react-pattern]] — ReAct 推理+行动模式
- [[multi-agent-collaboration]] — 多 Agent 协作模式
- [[skill-engineering]] — Skill 工程化设计
- [[context-engineering]] — 上下文工程

## 其他学习路线

### awesome-agentic-ai-zh
[[awesome-agentic-ai-zh]] 是另一份系统化的 AI Agent 中文学习路线图，三语对照（繁中/简中/英文），7 阶段 + 2 轨道（CLI Power User / Agent Builder），收录 145 个精选项目和资源，预估 14-19 周。与本路线互补——本路线更偏工程化实践，awesome-agentic-ai-zh 更系统化分轨。
