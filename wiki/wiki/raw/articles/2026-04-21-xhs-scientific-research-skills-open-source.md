---
title: "📚我把自己的科研方法论开源给了 AI Agent"
url: "https://www.xiaohongshu.com/discovery/item/69e6f4b1000000001a031a69?app_platform=ios&app_version=9.26.1&share_from_user_hidden=true&xsec_source=app_share&type=normal&xsec_token=CBDquaea01XJdSh8CXymDrr-QYgXL7PYPX52BoxtPXtZA=&author_share=1&xhsshare=CopyLink&shareRedId=ODY0REg6R0A2NzUyOTgwNjY0OTc7SUxM&apptime=1776760815&share_id=dcba7d1bb70743c7928f1e25b9c47345"
source: "小红书"
author: "流风回雪 Richard"
fetched: 2026-04-21
status: "success"
tool: "Spider_XHS"
likes: 117
collected: 199
comments: 4
tags: [AI科研, 开源, ClaudeCode, openclaw]
sha256: b27dc08679a8125b
---

# 📚我把自己的科研方法论开源给了 AI Agent

**作者**: 流风回雪 Richard | 👍 117 | ⭐ 199 收藏 | 💬 4 评论

最近想把我日常科研中沉淀下来的方法论整理成结构化的 Skills，供 Agent 直接读取和执行。但我发现现有的开源仓库（如 claude-scientific-skills）大多采用工具目录范式——枚举几百个具体工具，描述每个工具的功能。这种方式工具粒度过细，对单个研究者大部分都是冗余信息。

我觉得一个好的科研 skills 不应是某个工具的用法，而是一些 high-level 的 workflow。比如怎么画一张好看的科研绘图、怎么从一个观察走到 research question 等。

于是我把这些整理成了一个开源的 Skills 库👇

🔧 **工具集成型**
• literature-search — 多引擎自适应文献检索（Semantic Scholar / arXiv / Tavily / Exa / Gemini deep research / AMiner），按查询类型自动选择引擎

• social-media-paper-triage — 解析小红书 / gzh / X 的论文推荐，自动溯源原文、评估相关性

• zotero-management — 基于 collections + tags 的项目化文献库管理

• academic-figure-generation — 基于 PaperBanana 多 agent pipeline 的论文绘图

📋 **纯方法论型（零外部依赖）**
• paper-reading — 三级阅读法（skim → standard read → deep analysis）

• related-work-survey — 系统化调研流程：定义维度 → 多轴检索 → 构建 taxonomy → 识别 gap → 定位 contribution

✅ 跨平台兼容——OpenClaw / Claude Code / Codex 原生支持，无需适配层

✅ 模块化部署——支持按需选装

✅ AI-native onboarding——README内嵌给AI的引导流程：转发仓库链接或这个小红书链接给Agent即可一键配置

项目地址: https://github.com/jxtse/scientific-research-skills
