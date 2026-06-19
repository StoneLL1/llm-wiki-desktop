---
title: Yazi
created: 2026-05-23
updated: 2026-05-23
type: entity
tags: [tool, engineering, skill]
sources:
  - raw/articles/2026-04-18-xhs-claude-code-terminal-setup.md
---

# Yazi

## Overview

**Yazi** 是一款终端内的文件管理器，支持直接在当前窗口内快速预览、编辑文件或图片。在 [[claude-code]] 工作流中被推荐为辅助终端工具。

来源为小红书用户 CrazyAllen 的终端方案分享帖（542 赞、836 收藏）。

## 核心特性

- **终端内文件浏览**：无需离开 CLI 即可浏览目录结构
- **快速预览**：支持文件和图片的即时预览
- **内联编辑**：直接在终端窗口中编辑文件
- **极速性能**：用 Rust 编写，响应迅速

## 在 Claude Code 工作流中的角色

Yazi 补充了 [[claude-code]] CLI 环境的文件管理需求：
- 快速检查 Claude 生成的文件内容
- 浏览项目结构辅助上下文理解
- 预览图片文件验证生成结果

搭配 [[ghostty]] 终端模拟器使用，构成完整的终端开发环境。

## Relationships

- 推荐用于 [[claude-code]] 终端工作流
- 搭配 [[ghostty]] 组成终端工具链
- 辅助 [[vibe-coding]] 的文件管理需求

## See Also

- [[claude-code]] — Yazi 的主要应用场景
- [[ghostty]] — 搭配使用的终端模拟器
