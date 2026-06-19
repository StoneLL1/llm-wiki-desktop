---
title: "Horizon"
url: "https://github.com/Thysrael/Horizon"
source: "GitHub"
fetched: 2026-05-25
stars: 4612
forks: 635
language: "Python"
topics: [aggregator, news, llm, python, mcp, openclaw, feishu-bot, webhook]
---

# Horizon

> 📡 Your own AI-powered news radar. Generates daily briefings in English & Chinese. | 用 AI 构建你专属的新闻雷达

**⭐ 4612 stars | 🍴 635 forks | 📅 创建于 2026-02-20**

## 概述

Horizon 是一个 AI 驱动的个人新闻雷达系统。它从 Hacker News、Reddit、Telegram、RSS、Twitter/X、GitHub、OpenBB 等多源聚合新闻，通过 AI 打分、去重、过滤、背景丰富和评论摘要，生成中英双语每日简报。支持 GitHub Pages 发布、邮件订阅、飞书/钉钉/Slack/Discord Webhook 推送和 MCP 集成。

## 关键特性

- **📡 多源聚合** — Hacker News、RSS、Reddit、Telegram、Twitter/X、GitHub、OpenBB 金融新闻
- **🤖 AI 打分过滤** — 支持 Claude、GPT、Gemini、DeepSeek、Doubao、MiniMax、Ollama 等模型对新闻 0-10 评分
- **🔗 智能去重** — 跨平台自动合并同一事件的报道
- **🔍 背景丰富** — 对不熟悉的概念、公司、项目自动搜索补充背景信息
- **💬 评论摘要** — 收集并总结 HN、Reddit 等社区的讨论
- **🌐 中英双语** — 同一源集生成英文和中文每日简报
- **📝 GitHub Pages 发布** — 自动部署每日简报网站
- **📧 邮件订阅** — 自托管 SMTP/IMAP 新闻信，自动处理订阅/退订
- **🔔 多渠道推送** — 飞书、钉钉、Slack、Discord、自定义 Webhook
- **🧩 MCP 集成** — 将 Horizon 管道步骤暴露为 MCP 工具，AI 助手可直接调用
- **🧙 配置向导** — 交互式生成个性化源配置

## 技术栈

- **语言**: Python
- **AI 模型**: 支持 Claude、GPT、Gemini、DeepSeek、Doubao、MiniMax、Ollama 及任何 OpenAI 兼容 API
- **部署**: 本地安装（uv/pip）、Docker Compose、GitHub Actions 自动化
- **集成**: MCP Server、飞书/钉钉/Slack/Discord Webhook、SMTP/IMAP 邮件

## 工作流程

1. **定义** — 通过 JSON 配置源、阈值、模型、语言和投递渠道
2. **抓取** — 并发拉取所有配置源的最新内容
3. **去重** — 跨平台合并指向同一事件的条目
4. **评分过滤** — AI 对条目评分，仅保留超过阈值的
5. **丰富** — 搜索背景信息，收集社区讨论
6. **摘要** — 生成结构化 Markdown 简报
7. **投递** — 发布到 GitHub Pages / 邮件 / Webhook / MCP / 本地文件

## 与 AI Agent 生态的关联

- **MCP 集成**: 将完整的新闻管道暴露为 MCP 工具，可被 Claude Code、OpenClaw 等 AI 助手直接调用
- **OpenClaw 兼容**: 作为 OpenClaw 生态的信息源集成
- **飞书机器人**: 原生支持飞书 Bot 推送每日简报
