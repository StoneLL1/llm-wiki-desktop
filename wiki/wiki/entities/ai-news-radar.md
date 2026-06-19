---
title: "AI News Radar"
created: 2026-05-26
updated: 2026-05-26
type: entity
tags: [agent, open-source, tool, deployment]
sources:
  - raw/GitHub/LearnPrompt-ai-news-radar.md
---

# AI News Radar

**AI News Radar** 是一个自动更新的 24 小时 AI/技术新闻聚合管线，由 LearnPrompt 开源（441 ⭐，222 🍴）。核心创新是**伯乐Skill（Scout Skill）** — 一套信息源评估和分类系统，在接入之前先判断信源质量。

## 核心理念

不同于"又一个新闻聚合站"，AI News Radar 的核心是**先判断信源再接入**：

> 伯乐Skill（Scout Skill）帮你从一堆信源里选出千里马。哪些源值得长期追踪，哪些源适合做成 RSS/OPML，哪些源只能接付费的 API，哪些源看起来更新很多但实际上 AI 只占 5% 不到。先判断清楚，再接入。

新信源可在独立展示区域试运行一个月，再决定是否正式接入——这种「试用期」机制确保信息质量。

## 管道架构

```
信息源清单 → 伯乐Skill判断信源类型
    ├── 官方 RSS / changelog
    ├── 私人 OPML / RSS
    ├── 公开 GitHub feed / JSON
    ├── 公开页面 / Jina 兜底
    ├── AgentMail 邮箱订阅
    └── 跳过高风险来源
        ↓
    抓取与结构化 → 去重与归一化 → AI强相关过滤 → 源健康与覆盖统计
        ↓
    data/*.json → GitHub Pages 网页
        ↓
    [[openai-codex|Codex]] / [[claude-code|Claude Code]] 继续维护
```

与 [[horizon|Horizon]] 类似，AI News Radar 也是确定性数据管道（非 Agent Loop），但更侧重**信源质量预筛选**而非 AI 打分后筛选。

## 关键特性

- **追踪官方 AI 节点**: OpenAI News、OpenAI Codex Changelog、[[anthropic|Anthropic]]、Google DeepMind、Google AI、Hugging Face、GitHub AI
- **读取高信号 Newsletter**: AI Breakfast 等
- **多公开聚合源**: AI HOT 等，补足官方源盲区
- **OPML/RSS 批量导入**: 支持自定义订阅源
- **AgentMail 邮箱订阅**: 高质量 AI 日报邮箱聚合
- **双视图输出**: `AI 强相关` 和 `全量`
- **中英双语标题 + 站点分组**
- **零 API Key 部署**: 公开版不要求任何 LLM API Key、登录态或 Cookie

## 与 AI Agent 生态的关系

- **Agent 接手**: 内置 Skill 文件（`skills/ai-news-radar/`），[[openai-codex|Codex]]、[[claude-code|Claude Code]]、[[openclaw|OpenClaw]]、[[hermes-agent|Hermes]] 均可直接接手维护
- **[[skills|Skill]] 驱动**: 伯乐Skill 本身是可复用的 Agent 工作流，体现了 [[skill-engineering|Skill 工程化]] 的设计哲学——确定性判断逻辑封装为 Skill，Agent 只做决策引擎
- **飞书兼容**: 可对接 [[feishu|飞书]] 文档，与 WaytoAGI 开源社区联动

## 技术细节

- **语言**: Python (74.1%), JavaScript (15.4%)
- **部署**: GitHub Actions 自动化，每 30 分钟更新一次
- **发布**: GitHub Pages 静态站点
- **许可**: MIT
- **在线页面**: [learnprompt.github.io/ai-news-radar](https://learnprompt.github.io/ai-news-radar/)

## 与其他项目的对比

| 维度 | AI News Radar | [[horizon|Horizon]] | [[gpt-researcher|GPT Researcher]] |
|------|--------------|---------------------|----------------------------------|
| 定位 | AI 专项新闻雷达 | 通用新闻雷达 | 深度研究报告 |
| 信源筛选 | 伯乐Skill 预筛选（接入前评估） | AI 打分后筛选 | 按需搜索 |
| AI 依赖 | 核心功能无需 API Key | 需要 LLM 做打分/摘要 | 需要 LLM 做研究/报告 |
| 输出 | 24h 双视图（AI强相关/全量） | 每日简报（中英双语） | 深度研究报告 |
| 投递渠道 | GitHub Pages | Pages/邮件/飞书/钉钉/Slack/Discord/MCP | 本地文件 |
| 聚焦领域 | AI/技术 | 全领域 | 全领域 |

AI News Radar 的差异化优势在于**信源质量预评估**和**零成本部署**，适合关注 AI 领域动态的个人开发者。[[horizon|Horizon]] 则在投递渠道和多源聚合上更全面，适合需要通用新闻监控的场景。
