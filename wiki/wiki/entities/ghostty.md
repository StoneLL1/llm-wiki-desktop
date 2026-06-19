---
title: Ghostty
created: 2026-05-23
updated: 2026-05-23
type: entity
tags: [tool, engineering, skill]
sources:
  - raw/articles/2026-04-18-xhs-claude-code-terminal-setup.md
---

# Ghostty

## Overview

**Ghostty** 是一款现代终端模拟器，以灵活的分屏和标签页功能著称。在 [[claude-code]] 工作流中被推荐为首选终端方案，特别适合多任务并行开发场景。

来源为小红书用户 CrazyAllen 的终端方案分享帖（542 赞、836 收藏）。

## 核心特性

- **灵活分屏与标签页**：支持多窗口并行，多线作战的神器
- **copy-on-select**：框选文本后自动复制到剪贴板
- **clipboard-trim-trailing-spaces**：自动去掉粘贴文本时结尾的空格
- **clipboard-paste-protection**：粘贴保护机制，检测并提醒敏感信息或高危操作
- **link-url**：按住 ⌘ 能直接点开 Claude Code 返回的文件路径

## 在 Claude Code 工作流中的角色

Ghostty 的分屏能力与 [[claude-code]] 的 Agent 工作流天然契合：
- 一个标签页运行 Claude Code 主会话
- 另一个标签页监控输出或编辑文件
- 粘贴保护防止意外将敏感信息发送给 LLM
- link-url 功能让 CLI 中的文件路径可点击，加速导航

搭配 [[yazi]] 终端文件管理器使用，构成完整的终端开发环境。

## Relationships

- 推荐用于 [[claude-code]] 终端工作流
- 搭配 [[yazi]] 组成终端工具链
- 与 [[context-engineering]] 的终端实践相关

## See Also

- [[claude-code]] — Ghostty 的主要应用场景
- [[yazi]] — 搭配使用的终端文件管理器
