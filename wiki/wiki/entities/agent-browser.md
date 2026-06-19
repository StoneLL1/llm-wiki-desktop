---
title: Agent Browser
created: 2026-05-23
updated: 2026-05-23
type: entity
tags: [tool, open-source, agent]
sources:
  - raw/articles/2026-04-18-ai-knowledge-base-tutorial.md
---

# Agent Browser

## 概述

Agent Browser 是 Vercel Labs 发布的免费命令行工具，让 AI Agent 操控实际的 Chrome 浏览器进行网页抓取。GitHub 26K+ 星标。核心优势是让 AI 能够自动打开网页、提取文本内容，直接保存到本地文件系统中。

## 核心特性

- **真实浏览器操控**：下载专用的 Chrome 浏览器实例，AI 可操控完整浏览器行为
- **JS 动态渲染**：能处理 JavaScript 动态加载的网站、需要登录的页面、交互式图表
- **Token 高效**：比 Playwright MCP 节省 82% 的 token，同样一轮对话可抓取 5-6 倍的页面
- **命令行集成**：两条命令即可安装和使用

## 基本用法

```bash
agent-browser open https://some-article.com
agent-browser get text "article"
```

## 应用场景

在 [[llm-wiki-methodology]] 知识库构建中，agent-browser 用于自动化收集网页内容到 raw/ 目录。配合 [[claude-code]] 等 AI 编码工具，实现「看到文章 → AI 抓取 → 存入知识库」的自动化流程。

## 在知识管理中的定位

Agent Browser 是 [[karpathy-knowledge-compilation]] 工作流中 Phase 1（数据摄入）的关键工具，解决了手动复制粘贴的低效问题。与 [[obsidian]] Web Clipper 互补——后者适合剪藏，前者适合自动化批量抓取。

## See Also

- [[llm-wiki-methodology]] — 知识库编译方法论
- [[karpathy-knowledge-compilation]] — compile, don't search 的核心范式
- [[claude-code]] — 配合使用的 AI 编码工具
- [[knowledge-compilation]] — 知识编译的详细阐述
