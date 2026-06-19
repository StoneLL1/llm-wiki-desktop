---
title: "AI News Radar"
url: "https://github.com/LearnPrompt/ai-news-radar"
source: "GitHub"
fetched: 2026-05-26
stars: 441
forks: 222
language: "Python"
homepage: "https://learnprompt.github.io/ai-news-radar/"
created: "2026-02-21"
updated: "2026-05-26"
---

# AI News Radar

> 24 小时 AI 更新雷达｜伯乐Skill — 帮你从一堆信源里选出千里马

**⭐ 441 stars | 🍴 222 forks | 📅 创建于 2026-02-21**

## 概述

AI News Radar 是一个自动更新的 24 小时 AI/技术新闻聚合管线，由 GitHub Actions 驱动，附带实时 Web UI。核心创新是**伯乐Skill（Scout Skill）** — 一套信息源评估和分类系统，在接入之前先判断信源质量。

这不是"又一个新闻网页"，而是一条轻量新闻 pipeline：**来源判断 → 抓取 → 去重 → AI 强相关过滤 → 信息源健康状态 → 静态网页发布**。上线后不消耗模型额度，公开版无需任何 LLM API Key。

## 关键特性

- **追踪官方 AI 节点**：OpenAI News、OpenAI Codex Changelog、Anthropic、Google DeepMind、Google AI、Hugging Face、GitHub AI 等
- **读取高信号 Newsletter 公开来源**：如 AI Breakfast
- **读取网页自带 feed**：Follow Builders（X builders）、Anthropic Engineering、Claude Blog、AI podcasts
- **多公开聚合源**：AI HOT 等，补足官方源盲区
- **OPML/RSS 批量导入**：支持自定义订阅源
- **AgentMail 邮箱订阅**：高质量 AI 日报邮箱聚合
- **双视图输出**：`AI 强相关` 和 `全量`
- **中英双语标题 + 站点分组**
- **兼容飞书文档**：WaytoAGI 开源社区最近更新
- **GitHub Actions 自动化**：默认每 30 分钟运行一次

## 伯乐Skill — 信源评估哲学

核心思想：先判断信源质量，再接入。

- 评估哪些源值得长期追踪，哪些只是噪音
- 新信源可在独立展示区域试运行一个月，再决定是否正式接入
- 避免堆砌数千条信息的陷阱 — 聚焦信号质量

## 技术架构

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
    Codex / Claude Code 继续维护
```

## 快速开始

```bash
git clone https://github.com/LearnPrompt/ai-news-radar.git
cd ai-news-radar
python3 -m venv .venv
source .venv/bin/activate
pip install -r requirements.txt
python scripts/update_news.py --output-dir data --window-hours 24
python -m http.server 8080
```

自定义 OPML：
```bash
cp feeds/follow.example.opml feeds/follow.opml
# 编辑 feeds/follow.opml 加入自己的订阅源（不提交到仓库）
python scripts/update_news.py --output-dir data --window-hours 24 --rss-opml feeds/follow.opml
```

## 技术栈

- **语言**：Python (74.1%), JavaScript (15.4%), CSS (8.1%), HTML (2.4%)
- **自动化**：GitHub Actions（`.github/workflows/update-news.yml`）
- **发布**：GitHub Pages 静态站点
- **Agent 集成**：内置 Skill 文件，支持 Codex / Claude Code / OpenClaw / Hermes 接手维护
- **许可**：MIT
