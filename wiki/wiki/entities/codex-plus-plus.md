---
title: Codex++
created: 2026-05-27
updated: 2026-05-27
type: entity
tags: [tool, open-source]
sources:
  - raw/articles/2026-05-12-10-new-open-source-github-projects.md
---

# Codex++

## Overview

Codex++ 是 OpenAI Codex App 的增强补丁工具，通过 Chromium DevTools Protocol 注入脚本，解决 Codex App 的两个核心痛点：

1. **API Key 模式下插件不可用** — 解锁 API Key 模式下的插件功能，支持特殊插件强制安装
2. **只能归档不能删除会话** — 添加会话删除按钮，优先走服务端删除，不行退回本地 SQLite 删除，删前可确认和撤销

## 架构

- **非侵入式**：不修改 Codex App 安装目录，通过外部 launcher 启动
- macOS 安装后生成 `Codex++.app`
- 支持 Windows 和 macOS 双平台

## 相关链接

- [[openai-codex]] — Codex++ 所增强的平台
- [[claude-code]] — 对比的编程 Agent 平台
