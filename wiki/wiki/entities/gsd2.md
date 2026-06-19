---
title: GSD2
created: 2026-05-22
updated: 2026-05-22
type: entity
tags: [tool, agent, open-source]
sources:
  - raw/articles/2026-04-18-gsd2-auto-dev-tool.md
---

# GSD2

## 概述

GSD2（gsd-build/gsd-2）是一个面向独立开发者的 AI 编码工作流工具，基于 Pi Agent 构建。核心工作流是 **Research → Plan → Execute (per task) → Complete → Reassess Roadmap → Next Slice**，由 solo developer 开发，从独立开发者的痛点出发。

## 与之前的方案对比

### Taskmaster 方案的问题
1. GPT 总结出 PRD 文档 → 给 Taskmaster 拆解 tasks → 一步步实现
2. 问题：链路太长、人工介入太多、容易把任务弄散

### GSD2 的改进
- 更像下场干活的团队
- 将 GPT 的流程放在核心
- 自动化程度显著提升
- 每个任务独立执行后重新评估路线图

## 在 AI 编码生态中的定位

GSD2 是 [[vibe-coding]] 从"自动开发"到"有纪律的自动开发"的演进。用户提到之前用 superpowers / GSD（Skill 版本）"自动化差点意思"，GSD2 通过 Pi Agent 底层和迭代式工作流解决了这个问题。用户还提到希望对接 [[openclaw]]。

## 相关链接

- [[vibe-coding]] — 自然语言编程范式
- [[openclaw]] — 用户希望对接的开源多 Agent 平台
- [[archon]] — 另一种 AI 编码工作流引擎
