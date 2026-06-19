---
title: zero-native
created: 2026-05-27
updated: 2026-05-27
type: entity
tags: [tool, open-source, engineering]
sources:
  - raw/articles/2026-05-12-10-new-open-source-github-projects.md
---

# zero-native

## Overview

zero-native 是 Vercel Labs 推出的桌面应用框架，用 Zig 编写原生 Shell + Web UI，被视为 Tauri 的竞品。产物极小，重建极快。

## 核心特性

### 双引擎支持
- **系统 WebView**：macOS 用 WKWebView，Linux 用 WebKitGTK（体积最小）
- **Chromium (CEF)**：需要一致渲染表现时切换，配置文件里改一行即可

### 原生层
- 用 Zig 编写，编译飞快
- JS 到 Zig 的 bridge 经过大小限制、origin 检查、权限检查

### 前端支持
- Next.js、React、Svelte、Vue 等主流框架
- 用熟悉的 Web 工具链开发

### 安全模型
- WebView 默认被视为不可信
- 原生命令、权限、导航、外部链接都是 opt-in 策略控制

## 相关链接

- [[claude-code]] — AI 编程 Agent 的潜在桌面容器
