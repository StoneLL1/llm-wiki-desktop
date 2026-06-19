---
title: DESIGN.md
created: 2026-05-19
updated: 2026-05-27
type: entity
tags:
  - tool
  - design
  - methodology
sources:
  - https://github.com/nicholasgasior/awesome-design-md
  - raw/articles/2026-05-20-designmd-sh-design-registry.md
  - raw/articles/2026-04-18-vibe-design-frontend-ui.md
  - raw/articles/2026-04-18-58-website-styles-10days-40k-stars.md
---

# DESIGN.md

DESIGN.md 是一种将设计系统导出为 Markdown 规范文件的格式，由 Google Stitch 推广。可被 Claude Code、Cursor 等 AI 编程工具直接消费。awesome-design-md 仓库（由 VoltAgent 团队开发）收录了 58 个知名网站的设计系统，开源 10 天即获得 4 万+ Star。^[raw/articles/2026-04-18-58-website-styles-10days-40k-stars.md]

## DESIGNMD.sh Registry

[DESIGNMD.sh](https://designmd.sh/) 是一个 DESIGN.md 设计规范文件的在线注册中心，提供 20+ 知名品牌（Nike、SpaceX 等）的 DESIGN.md 文件，涵盖配色、字体层级、间距、圆角、布局规则和组件风格等设计要素。^[raw/articles/2026-05-20-designmd-sh-design-registry.md]

- 可通过 `npx designmd.sh add <owner/repo>` 一键拉取到项目中
- 21 种风格已提供可视化展示（https://hermes.aigc.green/designmd-demo/）
- 持续收录中，用于指导 AI Agent 生成一致的 HTML / CSS / UI

## awesome-design-md 仓库

由 VoltAgent 团队开发的 [awesome-design-md](https://github.com/VoltAgent/awesome-design-md) 仓库，收录了 58 个知名品牌网站（Nike、Claude、Notion、Figma 等）的完整设计规范，以 DESIGN.md 格式保存，开源 10 天即获得 4 万+ Star。^[raw/articles/2026-04-18-58-website-styles-10days-40k-stars.md]

### 设计文件内容

每个品牌的设计文件遵循 9 大标准化板块：视觉主题与氛围、调色板与色彩角色、排版规范、组件样式、布局原则、阴影与层级、设计禁忌、响应式规则，以及给 AI Agent 的提示词指南。参数从真实网站 CSS 提取（如 Vercel 的 `box-shadow: 0px 0px 0px 1px rgba(0,0,0,0.08)`），非凭感觉编写。每个品牌文件夹还自带 preview.html 和 preview-dark.html。

### 使用方式

三步即可使用：从仓库中选取品牌文件夹 → 复制 DESIGN.md 到项目根目录 → 告诉 AI 参照该文件生成 UI。兼容 [[claude-code]]、[[cursor]]、[[openai-codex|Codex]]、[[stitch]] 等所有能读取项目文件的 AI 编程工具。

## Vibe Design + DESIGN.md 解决的痛点

Vibe Coding 产出的 UI 常被称为「2015 年 Bootstrap 模板拼出来的」——配色要么太素要么太艳，间距全靠 AI 自由发挥，组件风格不统一。根本原因是：开发者从未告诉 AI UI 应该长什么样、配色用什么色系、按钮圆角多大、间距规则是什么。

DESIGN.md 恰好解决了这个问题——它可以同时承载**精确的参数**（色值、间距、字号）和**模糊的指引**（氛围、风格、原则），而 AI 两种都能处理。Markdown 是目前 LLM 最能理解的文档格式，有结构但不死板，有层级但不复杂，对人可读对机器可解析。^[raw/articles/2026-04-18-vibe-design-frontend-ui.md]

## 相关链接

- [[stitch]]
- [[claude-code]]
- [[claude-design]]
- [[vibe-design]]
- [[typeui-design-md-extractor|TypeUI Design.md Extractor]]


## 实测效果（鲁工实测）

测试方案：Claude Code 生成医疗项目管理看板
- **无 DESIGN.md**：白底黑字，间距偏大，颜色偏素，Tailwind 默认配色，平淡普通
- **加 Linear DESIGN.md**：微妙边框阴影、字体层级清晰、间距规整、hover 效果

关键观察：DESIGN.md 不只是告诉 AI 用什么颜色，而是给了一整套设计决策框架。

### Markdown 为什么适合承载设计系统

- 精确参数（色值、间距、字号）+ 模糊指引（氛围、风格、原则），AI 两种都能处理
- 比 Figma（AI 不好读）、JSON（太碎片化）、Wiki（格式不统一）更适合 LLM 理解
- 与具体工具无关：Claude Code、Cursor、Gemini CLI 都能读

### Sources
- raw/articles/2026-04-18-vibe-design-frontend-ui.md（鲁工）
