---
title: Open-Slide
created: 2026-05-17
updated: 2026-05-17
type: entity
tags:
  - tool
  - presentation
  - open-source
sources:
  - raw/articles/2026-05-07-open-slide-replace-powerpoint.md
---

# Open-Slide

## 概述

Open-Slide 是一个开源的 Web 版 PPT 生成工具，旨在替代传统的 PowerPoint 工作流。它支持 Presentation Model 的定义和编辑，并能导出为 PDF 和 HTML 格式，适合 AI Agent 驱动的演示文稿生成场景。

## 核心特性

- **Presentation Model**：以结构化数据描述幻灯片内容，与视觉样式分离
- **多格式导出**：支持导出 PDF、HTML 等格式
- **Web 原生**：基于 Web 技术构建，无需安装桌面软件
- **开源**：社区驱动，可自由定制和扩展

## DracoVibeCoding 封装

DracoVibeCoding 对 Open-Slide 进行了 Skills 封装，并提供了模板集合，使得 AI Agent（如 [[claude-code]]）能够直接通过 Skill 调用生成高质量的演示文稿。这一封装降低了 Agent 生成 PPT 的门槛。

## Agent 时代的 Taste 讨论

Open-Slide 的出现引发了关于 AI Agent 时代"品味"（Taste）的讨论：当 AI 能够自动生成 PPT 时，人的价值更多体现在审美判断和内容编排上，而非机械的排版操作。[[claude-design]] 所代表的审美能力将成为核心竞争力和差异化要素。

## 相关链接

- [[claude-code]] — 通过 Skills 封装，Claude Code 可直接调用 Open-Slide 生成 PPT
- [[claude-design]] — Agent 时代的审美与设计能力
- [[skills]] — Open-Slide 的 Skill 封装示例
