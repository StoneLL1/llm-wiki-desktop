---
title: "Horizon"
created: 2026-05-25
updated: 2026-05-25
type: entity
tags: [agent, open-source, tool, deployment]
sources:
  - raw/GitHub/Thysrael-Horizon.md
---

# Horizon

**Horizon** 是一个 AI 驱动的个人新闻雷达系统，由 Thysrael 开源（4612 ⭐，635 🍴）。它从多源聚合新闻，通过 AI 打分、去重、背景丰富和评论摘要，生成中英双语每日简报。

## 核心功能

- **多源聚合**: Hacker News、RSS、Reddit、Telegram、Twitter/X、GitHub Releases、OpenBB 金融新闻
- **AI 打分过滤**: 支持 [[claude-model-family|Claude]]、GPT、Gemini、[[deepseek|DeepSeek]]、Doubao、MiniMax、Ollama 等模型，对每条新闻 0-10 评分
- **智能去重**: 跨平台自动识别并合并同一事件的重复报道
- **背景丰富**: 对不熟悉的概念、公司、项目自动搜索补充背景
- **评论摘要**: 收集并总结 HN、Reddit 社区讨论
- **中英双语**: 同一源集生成英文和中文简报
- **配置向导**: 交互式生成个性化源配置（`horizon-wizard`）

## 管道架构

Horizon 的工作流是 7 步管道：定义配置 → 并发抓取 → 去重 → AI 评分过滤 → 背景丰富 → 摘要生成 → 多渠道投递。

这种管道式设计与 [[react-pattern|ReAct]] 和 [[plan-and-execute-pattern|Plan-and-Execute]] 等 Agent 模式不同——Horizon 是纯数据管道，没有 Agent Loop 的迭代推理环节，而是用确定性的步骤序列处理信息流。

## 投递渠道

| 渠道 | 说明 |
|------|------|
| GitHub Pages | 自动部署每日简报网站 |
| 邮件订阅 | 自托管 SMTP/IMAP，自动处理订阅/退订 |
| Webhook | [[feishu|飞书]]、钉钉、Slack、Discord |
| [[mcp|MCP]] Server | 暴露管道步骤为 MCP 工具 |
| 本地文件 | 保存 Markdown 到 `data/summaries/` |

## 与 AI Agent 生态的关系

- **MCP 集成**: 将完整的新闻管道暴露为 MCP 工具，可被 [[claude-code|Claude Code]]、[[openclaw|OpenClaw]] 等直接调用。这与 [[mcp]] 生态的「工具即服务」理念一致。
- **[[openclaw|OpenClaw]] 兼容**: 作为 OpenClaw Agent 可用的信息源集成
- **飞书原生**: 支持 [[feishu|飞书]] Bot 推送，适合企业场景的每日简报分发

## 技术细节

- **语言**: Python，使用 uv 管理依赖
- **部署**: 本地安装、Docker Compose、GitHub Actions 自动化
- **配置**: 单一 JSON 配置文件管理源、阈值、模型、语言和投递
- **环境变量**: 配置中可用 `${VAR_NAME}` 引用环境变量

## 与其他项目的对比

| 维度 | Horizon | [[ai-news-radar|AI News Radar]] | [[gpt-researcher|GPT Researcher]] |
|------|---------|-------------------------------|----------------------------------|
| 定位 | 通用新闻雷达 | AI 专项新闻雷达 | 深度研究报告 |
| 信源筛选 | AI 打分后筛选 | 伯乐Skill 预筛选（接入前评估） | 按需搜索 |
| AI 依赖 | 需要 LLM 做打分/摘要 | 核心功能无需 API Key | 需要 LLM 做研究/报告 |
| 投递渠道 | Pages/邮件/飞书/钉钉/Slack/Discord/MCP | GitHub Pages | 本地文件 |
| 聚焦领域 | 全领域 | AI/技术 | 全领域 |

Horizon 在投递渠道和多源聚合上更全面，适合通用新闻监控。AI News Radar 的差异化在于**信源质量预评估**和**零成本部署**，更适合关注 AI 领域动态的开发者。GPT Researcher 则专注于按需深度调研。

## See Also

- [[last30days-skill|Last30Days Skill]]
