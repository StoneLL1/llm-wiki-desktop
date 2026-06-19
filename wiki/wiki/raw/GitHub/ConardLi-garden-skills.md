---
title: "garden-skills"
url: "https://github.com/ConardLi/garden-skills"
source: "GitHub"
fetched: "2026-06-02"
stars: 6994
forks: 956
language: "CSS"
topics: [agent, claude, gpt-image-2, rag, skills, web-design]
---

# garden-skills

> ConardLi 的开源 Agent Skills 合集，包含网页设计、知识检索、图片生成等 production-ready 的 Skills

**⭐ 6,994 stars | 🍴 956 forks | 📅 创建于 2026-04-21**

## 概述

garden-skills 是 ConardLi（code秘密花园）维护的 Agent Skills 合集仓库，面向 Claude Code、Cursor、Codex 及其他 AI 编程 Agent。仓库提供 4 个 production-ready 的 Skill，聚焦于让 Agent 产出从「碰运气」变为「可复现的生产线」。

Skill 的核心理念：提供明确的工作流程（何时问、何时做、何时停止）、明确的质量标准、明确的迭代接口。

## 四个 Skill

### 1. web-video-presentation（网页视频/演示）
- **定位**：将文章、脚本、课程等文字内容转化为网页模拟的视频，录屏即得演示视频
- **技术栈**：Vite + React + TypeScript
- **亮点**：固定 1920×1080 舞台、23 套内置主题、可插拔 TTS（MiniMax/OpenAI/ElevenLabs/edge-tts）、章节/步骤驱动
- **最佳模型**：Opus 4.7
- **版本**：v1.2.1

### 2. web-design-engineer（网页设计）
- **定位**：消除 AI 生成网页的「默认审美」，让 Agent 按专业设计流程产出有经验感的网页
- **亮点**：六步设计流程、设计方向顾问（六大流派）、25 套锚定风格配方（Linear/Aesop/Pentagram/Bloomberg/Stripe Press 等）、反 AI 套路清单
- **适用场景**：官网、落地页、Dashboard、活动页、作品集、交互原型
- **版本**：v1.2.1

### 3. gpt-image-2（图片生成）
- **定位**：面向 GPT Image 2 及 OpenAI 兼容图像 API 的结构化生图 Skill
- **亮点**：三种运行模式（本地/宿主工具/顾问）、18 大类 79 个结构化 Prompt 模板、覆盖生成与编辑工作流
- **关键**：好的图片 Prompt 需同时描述画面目标、主体关系、构图、材质、光线、字体限制、输出尺寸、后续编辑空间
- **版本**：v1.0.3

### 4. kb-retriever（知识检索）
- **定位**：本地知识库检索，支持 Markdown、文本、PDF、Excel 文件
- **亮点**：分层索引文件导航、先学习后处理规则（PDF/Excel）、最多 5 轮搜索、grep/pdftotext/pdfplumber/pandas 工作流
- **版本**：v1.0.0

## 安装方式

5 种安装路径：`npx skills add`（推荐）、Claude Code 插件市场、GitHub Releases 固定版本 .zip、手动复制、Git submodule。

兼容 Claude Code、Claude.ai（Web）、Cursor、Codex CLI、Gemini CLI、OpenCode。

## 技术栈

- 语言：CSS（主要）
- Topics: agent, claude, gpt-image-2, rag, skills, web-design
- 协议：MIT License
- 在线体验：https://mmh1.top/
