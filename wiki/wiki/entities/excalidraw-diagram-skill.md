---
title: Excalidraw Diagram Skill
created: 2026-05-22
updated: 2026-05-22
type: entity
tags:
  - tool
  - design
  - skill
  - open-source
sources:
  - raw/articles/2026-04-18-ai-skill-architecture-diagrams.md
---

# Excalidraw Diagram Skill

## 概述

Excalidraw Diagram Skill（coleam00/excalidraw-diagram-skill）是一个开源的 AI Agent Skill，能够将自然语言描述转换为美观实用的 Excalidraw 手绘风格图表。它专注于将复杂的概念和流程可视化，实现「构思图表」和「落地排版」的彻底解耦。

- GitHub: https://github.com/coleam00/excalidraw-diagram-skill
- 兼容 Claude Code、OpenCode、Codex 等主流 AI Agent 工具

## 核心功能

### AI 图表生成

根据自然语言描述自动生成结构清晰、风格一致的图表：

- 流程图
- 组织结构图
- 技术架构图
- 系统设计图
- 其他复杂图表

### 可视化验证

自动检测并修复布局问题：

- 重叠文本检测与修复
- 错位箭头校正
- 不平衡间距调整

### 可定制品牌风格

通过修改 `color-palette.md` 文件调整图表颜色和样式，使其与品牌风格保持一致。

## 使用流程

1. **输入描述**：在 AI 代理中用自然语言描述图表需求（如「创建一个 RAG 流程图」）
2. **生成图表**：AI 自动分析描述并生成 Excalidraw 图表
3. **查看结果**：交付 `.png`（高清图片）+ `.excalidraw`（源文件，可二次编辑）

安装方式：将 Skill 放入 `.claude/skills/` 目录即可自动识别，或通过对话让 Agent 直接安装。

## 依赖

使用 Playwright 将 `.excalidraw` 渲染为 PNG 图片。在某些 AI 客户端（如 QClaw）中可自动复用系统 Chrome，无需额外安装 Playwright。

## 设计价值

解决了 Excalidraw 手绘白板（121K Star）在复杂图表场景下排版混乱、结构调整困难的痛点。将「构思图表」和「落地排版」解耦，配合内置视觉校验，几分钟即可完成从描述到图表的转换。

## 相关链接

- [[claude-code]] — Skill 运行平台
- [[skills]] — Skill 工程化设计体系
- [[ppt-master]] — AI 演示文稿生成（图表可嵌入 PPT）
- [[vibe-design]] — AI 辅助设计范式
