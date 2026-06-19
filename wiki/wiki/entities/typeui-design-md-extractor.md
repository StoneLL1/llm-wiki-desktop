---
title: "TypeUI DESIGN.md Extractor"
created: 2026-05-25
updated: 2026-05-27
type: entity
tags: [design, tool, open-source, engineering]
sources:
  - raw/GitHub/bergside-design-md-chrome.md
---

# TypeUI DESIGN.md Extractor

**TypeUI DESIGN.md Extractor** 是一个 Chrome 扩展（bergside 开源，1227 ⭐，168 🍴），能从任意网站自动提取样式并生成 `DESIGN.md` 或 `SKILL.md` 文件，供 [[claude-code|Claude Code]]、[[openai-codex|Codex]]、[[stitch|Google Stitch]] 等 AI 工具使用。

## 核心功能

| 操作 | 说明 |
|------|------|
| Auto-extract | 读取当前页面排版（typography）、颜色（colors）、间距（spacing）、圆角（radius）、阴影（shadows）、动效（motion） |
| Generate DESIGN.md | 输出设计系统文档 Markdown |
| Generate SKILL.md | 输出 Agent 可直接加载的 Skill 文件 |
| Refresh | 重新提取当前页面状态 |
| Download | 保存为 `DESIGN.md` 或 `SKILL.md` |
| Explain (?) | 显示文件生成过程及 TypeUI 参考 |

## 生成的文件结构

DESIGN.md 包含 11 个标准章节：

| 章节 | 作用 |
|------|------|
| Mission | 定义设计系统目标 |
| Brand | 捕获品牌上下文、URL、受众、产品面 |
| Style Foundations | 列出推断的视觉 token 和基础 |
| Accessibility | 应用 WCAG 2.2 AA 要求和交互约束 |
| Writing Tone | 设定实现就绪的指导语调 |
| Rules: Do | 必要的实现实践 |
| Rules: Don't | 反模式和禁止行为 |
| Guideline Authoring Workflow | 指南编写步骤 |
| Required Output Structure | 一致的输出章节 |
| Component Rule Expectations | 交互/状态详情要求 |
| Quality Gates | 可测试的质量和一致性检查 |

## 与设计工具生态的关系

TypeUI 是 [[design-md|DESIGN.md]] 格式的实践者之一。它与 [[hue]]（品牌设计 Skill）和 [[kami]]（AI 文档设计系统）定位互补：

| 工具 | 定位 | 输入 | 输出 |
|------|------|------|------|
| TypeUI Extractor | 从现有网站逆向提取设计规范 | 网页 URL | DESIGN.md / SKILL.md |
| [[hue]] | 从品牌信息生成设计系统 | URL/品牌名/截图 | 完整设计系统 |
| [[kami]] | 设计经验编码化为 AI 可用规则 | 设计经验 | SKILL.md |

三者都服务于 [[vibe-design|Vibe Design]] 范式——用自然语言和设计规范驱动 AI 生成一致的 UI。

## 技术细节

- **语言**: JavaScript
- **平台**: Chrome Extension（开发者模式加载）
- **格式**: 基于 [TypeUI DESIGN.md](https://www.typeui.sh/design-md) 开源格式
- **策展**: 精选设计系统可在 [typeui.sh/design-skills](https://www.typeui.sh/design-skills) 浏览
- **测试**: `node tests/run-tests.mjs`
- **许可证**: MIT

## 相关链接

- [[design-md]] — DESIGN.md 格式定义
- [[stitch|Google Stitch]] — AI 原生 UI 设计平台
- [[claude-code]] — 主要集成对象之一
- [[vibe-design]] — Vibe Design 范式
- [[skills]] — Skill 文件标准
