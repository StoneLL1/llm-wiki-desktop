---
title: hue
created: 2026-05-17
updated: 2026-05-17
type: entity
tags:
  - tool
  - design
  - skill
sources:
  - raw/GitHub/dominikmartn-hue.md
---

# hue

## 概述

hue 是由 dominikmartn 开源的品牌设计 Skill，专为 AI 代码助手打造。它能够从 URL、品牌名称或截图学习品牌特征，并自动生成完整的设计系统，确保设计一致性。

## 核心能力

- **品牌学习**：从 URL、名称或截图提取品牌视觉特征
- **设计系统生成**：
  - 颜色令牌（Color Tokens）
  - 排版规范（Typography）
  - 间距系统（Spacing）
  - 组件样式（Components）
  - 亮暗模式（Light/Dark Mode）
- **设计一致性保证**：生成的所有设计元素遵循统一的品牌语言

## 支持平台

- [[claude-code]]（Claude Code）
- OpenAI Codex

这意味着无论使用哪个主流 AI 编码助手，都可以通过 hue Skill 获得一致的品牌设计能力。

## 设计一致性的重要性

在 AI Agent 大量生成 UI 的时代，品牌一致性成为关键挑战。hue 通过将品牌设计规范编码为可复用的 Skill，确保 Agent 每次生成的界面都符合品牌调性，避免了"每个页面长得不一样"的常见问题。

## 相关链接

- [[claude-code]] — hue 作为 Skill 在 Claude Code 中运行
- [[claude-design]] — AI 辅助设计的方法论与实践
- [[skills]] — hue 是 Skill 工程化的优秀案例
