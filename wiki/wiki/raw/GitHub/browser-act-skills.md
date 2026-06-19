---
title: "BrowserAct Skills"
url: "https://github.com/browser-act/skills"
source: "GitHub"
fetched: 2026-06-03
stars: 1573
forks: 39
language: "Python"
topics: [ai-agents, automation, claude-code, claude-code-skills, cursor, no-code, openclaw, web-scraping, codex, codex-cli, codex-skill, stealth-browser, anti-detection]
---

# BrowserAct Skills

> Browser automation CLI built for AI agents. Break through anti-bot walls, hand off to humans across platforms when stuck. Parallel multi-task execution, independent multi-session operation, isolated multi-account browsing.

**⭐ 1,573 stars | 🍴 39 forks | 📅 创建于 2026-02-06**

## 概述

面向 AI Agent 的浏览器自动化 CLI。解决 Playwright 等传统框架在真实互联网环境中的短板：反检测、验证码、Session 管理、人机协作。

## 为什么选 BrowserAct

Agent 需要的浏览器必须做到四件事：
1. **突破反爬** — 环境层（指纹伪装）、执行层（自动解验证码）、人机层（远程协助）
2. **三种浏览器模式** — `chrome`（复用登录态）、`stealth` 隐私（无痕批量）、`stealth` 固定身份（登录态多账号）
3. **零干扰并发** — 跨浏览器并行（独立 Cookie/指纹/代理）、同浏览器多会话（共享登录态独立执行）
4. **为 Agent 推理设计** — 紧凑文本输出、索引化交互（`click 3`）、语义记忆、并发安全

## 两个产品 Skill

| Skill | 用途 | 安装方式 |
|-------|------|----------|
| `browser-act` | 实时浏览器控制 CLI | `uv tool install browser-act-cli --python 3.12` |
| `browser-act-skill-forge` | 网站能力封装为可复用 Skill | 见仓库 |

## 技术栈

- Python 3.12+
- Playwright (浏览器引擎)
- Camoufox (Stealth 反检测)
- Typer (CLI)

## 兼容性

OS: Windows / macOS / Linux
Agent: Claude Code / Cursor / VS Code / OpenCode / OpenClaw / Codex / Gemini CLI

## 本机安装状态

- `browser-act` 已安装 (v0.1.25, via uv, Python 3.12)
- `browser-act-skill-forge` 未安装（按用户要求仅装 browser-act）
- Stealth 功能需 API Key（尚未配置）
