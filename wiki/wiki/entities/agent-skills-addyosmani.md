---
title: Agent Skills (Addy Osmani)
created: 2026-05-22
updated: 2026-05-24
type: entity
tags: [tool, engineering, open-source]
sources:
  - raw/articles/2026-04-18-github-hot-10-open-source-projects.md
  - raw/articles/2026-04-21-github-top10-weekly-stars.md
---

# Agent Skills (Addy Osmani)

## 概述

Agent Skills 是 Google Chrome 团队工程 Leader Addy Osmani（著有 *Learning JavaScript Design Patterns*）开源的 AI 编码工程纪律包。将资深工程师的开发规范封装成 AI 可直接执行的 [[skills|Skill]]。目前 1.66 万 Star。

## 核心特性

- **7 个核心 Skill 贯穿全流程**：`/spec`（需求）、`/plan`（拆解任务）、`/build`（增量实现）、`/test`（验证）、`/review`（质量门禁）、`/code-simplify`（简化）、`/ship`（部署）
- **20 个 Skill 按阶段分类**：覆盖从定义需求到上线的完整生命周期
- **工程纪律注入**：解决 AI 编码容易走捷径跳过规范的问题

## 在 Skill 生态中的定位

Agent Skills 是 [[skill-engineering]] 和 [[harness-engineering]] 理念的具体实践——通过结构化的 Skill 将工程纪律"硬塞"给 AI Agent。与 [[andrej-karpathy-skills]] 侧重编码原则不同，Agent Skills 侧重全流程规范。

## 相关链接

- [[skills]] — Skill 模块化能力系统
- [[harness-engineering]] — 构建 AI 模型能力脚手架的方法论
- [[andrej-karpathy-skills]] — 另一个编码规范 Skill 包
