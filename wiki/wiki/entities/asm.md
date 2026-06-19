---
title: asm (Agent Skill Manager)
created: 2026-05-23
updated: 2026-05-23
type: entity
tags:
  - tool
  - agent
  - open-source
  - skill
sources:
  - raw/articles/2026-04-18-asm-ai-coding-assistant-manager.md
---

# asm (Agent Skill Manager)

## 概述

**asm**（agent-skill-manager）是一个统一的命令行工具，帮助用户跨多个 AI 编程助手管理 [[skills]]。由 luongnv89 开发，支持 Claude Code、Codex、Cursor、OpenClaw 等 17+ 个主流 AI 编程助手的技能统一管理。

GitHub: https://github.com/luongnv89/asm

## 解决的核心痛点

| 痛点 | asm 解决方案 |
|------|-------------|
| 技能散落各目录 | 统一管理所有 AI 助手 |
| 安装繁琐 | 一键安装，自动验证 |
| 安全隐患 | 内置安全扫描和审计 |
| 开发困难 | 完整的本地开发工作流 |
| 发布复杂 | 自动发布到 Registry |
| 版本混乱 | 重复检测和清理 |

同一个技能可能在 `~/.claude/skills/`、`~/.codex/skills/`、`~/.openclaw/skills/` 和项目级 `.claude/skills/` 各安装一次，asm 通过统一视图解决这一问题。

## 核心特性

### 统一仪表板
TUI 界面提供全局视野，可列出、搜索和过滤所有提供商和范围的技能。同时支持 CLI 模式（`--json` 输出，适合脚本和自动化）。

### 一键安装
`asm install github:user/repo` 自动处理克隆、验证和放置，支持单技能仓库、多技能集合、子文件夹 URL 和 SSH 私有仓库。

### 安全扫描
内置安全扫描功能，安装前标记危险模式：Shell 执行、网络访问、凭证泄露、代码混淆（如 `atob()` 调用、可疑 base64 字符串、十六进制转义序列）。

### 本地开发工作流
```
asm init my-skill          # 创建新技能
asm link ./my-skill -p claude  # 符号链接实时开发
asm audit security my-skill    # 安全审计
asm publish ./my-skill         # 发布到 ASM Registry
```

### 支持的 AI 助手
Claude Code、Codex、OpenClaw、Cursor、Windsurf、Cline、Roo Code、Continue、GitHub Copilot、Aider、OpenCode、Zed、Augment、Amp、Gemini CLI、Google Antigravity、Generic Agents 等。

### ASM Registry
发布流程自动验证 SKILL.md frontmatter、执行安全审计、生成 manifest，通过 `gh` CLI 创建 PR。合并后任何人可按名称安装：`asm install code-review`。

## 技能验证机制

asm 自动评估技能的验证标准，通过的技能获得 verified 徽章：

- 有效的 frontmatter（SKILL.md 必须包含 name 和 description）
- 有意义的内容（Markdown 正文至少 20 字符）
- 无恶意模式
- 正确的目录结构

## 相关链接

- [[skills]] — asm 管理的技能模块
- [[claude-code]] — 主要支持的 AI 编程助手之一
- [[openclaw]] — 支持的开源 Agent 平台
- [[skill-engineering]] — 技能工程化设计方法论
