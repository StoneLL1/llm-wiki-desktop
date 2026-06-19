---
title: garden-skills
created: 2026-06-03
updated: 2026-06-03
type: entity
tags: [skill, agent, design, presentation, open-source, tool]
sources:
  - raw/GitHub/ConardLi-garden-skills.md
  - raw/articles/2026-06-02-garden-skills-7k-stars.md
---

# garden-skills

garden-skills 是 [[conard-li|ConardLi]]（code秘密花园）开源的 Agent [[skills]] 合集，面向 [[claude-code]]、[[cursor]]、[[openai-codex]] 及其他 AI 编程 Agent。仓库包含 4 个 production-ready 的 Skill，聚焦于让 Agent 产出从「碰运气」变为「可复现的生产线」。

**⭐ 6,994 stars | 🍴 956 forks | 📅 创建于 2026-04-21 | MIT License**

## 核心设计理念

> Skill 的真正价值不在提示词写得多漂亮，而在于把一套可重复稳定工作的方法交给 Agent。

三个要素：
- **明确的工作流程**：何时问、何时做、何时停
- **明确的质量标准**：什么算好、什么算 AI 味太重
- **明确的迭代接口**：不满意时反馈什么、Agent 知道改哪一层

## 四个 Skill

### 1. web-video-presentation（网页视频/演示）

将文字内容（文章、脚本、课程、产品 Demo）转化为基于网页模拟的演示视频。

| 维度 | 详情 |
|------|------|
| 技术栈 | Vite + React + TypeScript |
| 舞台规格 | 固定 1920×1080 |
| 内置主题 | 23 套（bold-signal / terminal-green / newsroom / electric-studio / bauhaus-bold / creative-voltage / neon-cyber / vintage-editorial / split-canvas / dark-botanical / forest-ink 等） |
| TTS 支持 | 可插拔（MiniMax / OpenAI / ElevenLabs / edge-tts / Azure / Google Cloud） |
| 最佳模型 | Opus 4.7 |
| 版本 | v1.2.1 |

**为什么用网页做视频**：网页将视频拆成工程——章节、步骤、旁白、画面、主题、进度控制全可被代码控制，避免了传统 AI 视频的「随机抽卡」和「消耗爆炸」问题。Agent 生成后支持局部修改（"第三章节奏太平了，做得更像发布会 Keynote"）。

在线预览：https://mmh1.top/#/ai-application/web-video-presentation

### 2. web-design-engineer（网页设计）

消除 AI 生成网页的「默认审美」——大渐变、玻璃卡片、发光边框、过度圆角——让 Agent 按专业设计流程产出有经验感的网页。

| 维度 | 详情 |
|------|------|
| 流程 | 六步设计流程：产品类型与受众→视觉方向→信息层级→排版节奏→组件密度→交互细节 |
| 设计顾问 | 六大设计流派方向顾问 |
| 锚定风格 | 25 套（Linear / Aesop / Pentagram / Bloomberg / Stripe Press / Raycast / Tufte / Mailchimp / Headspace / Y2K / Balenciaga 等） |
| 反 AI 套路 | 内置反 AI 套路清单 |
| 适用场景 | 官网、落地页、Dashboard、活动页、作品集、交互原型 |
| 版本 | v1.2.1 |

这个 Skill 与 [[claude-design]] 和 [[stitch]] 的目标一致——让 AI 产出更像专业设计师打磨过的作品，同时与 [[stop-slop]] 和 [[anti-slop-writing]] 形成互补（后者针对文本 AI 味，web-design-engineer 针对视觉 AI 味）。

在线预览：https://mmh1.top/#/ai-application/web-design-engineer

### 3. gpt-image-2（图片生成）

面向 GPT Image 2 及 OpenAI 兼容图像 API 的结构化生图 Skill。

| 维度 | 详情 |
|------|------|
| 运行模式 | 三种：本地模式（直调 API）、宿主工具模式（交给 Agent 自带图像工具）、顾问模式（退化为 Prompt 顾问） |
| 模板体系 | 18 大类、79 个结构化 Prompt 模板 |
| 覆盖场景 | 海报、UI Mockup、产品图、信息图、论文图、技术架构图、漫画、头像、分镜、品牌板、图片编辑 |
| 版本 | v1.0.3 |

**设计核心**：好的图片 Prompt 需同时描述画面目标、主体关系、构图、材质、光线、字体限制、输出尺寸、后续编辑空间——gpt-image-2 把图像任务拆成结构化模板，让 Agent 先拆清任务再生成，减少模型猜测空间。

在线 Playground：https://gpt-image2.mmh1.top/#/playground

### 4. kb-retriever（知识检索）

本地知识库检索 Skill，支持 Markdown、文本、PDF、Excel 文件。

| 维度 | 详情 |
|------|------|
| 索引机制 | 分层索引文件导航 |
| 处理规则 | 先学习后处理（PDF/Excel 特殊处理） |
| 搜索轮次 | 最多 5 轮搜索 |
| 工具链 | grep / pdftotext / pdfplumber / pandas |
| 版本 | v1.0.0 |

## 安装与兼容性

**5 种安装路径**：
1. `npx skills add`（推荐）
2. Claude Code 插件市场
3. GitHub Releases 固定版本 .zip
4. 手动复制
5. Git submodule

**兼容平台**：Claude Code、Claude.ai（Web）、[[cursor]]、[[openai-codex|Codex CLI]]、Gemini CLI、OpenCode。

## 与其他 Skills 合集的对比

| 维度 | garden-skills | [[huashu-skills]] |
|------|-------------|-------------------|
| 作者 | ConardLi | 花叔 |
| Skill 数量 | 4（精选生产级） | 20（全链路覆盖） |
| 定位 | 视觉产出（视频/网页/图片） | 内容创作（选题到发布） |
| 设计理念 | 生产线式稳定工作流 | 全链路细分覆盖 |
| Star | 7K | — |

garden-skills 与 [[guizang-ppt-skill]] 在「用设计经验编码化」理念上一致，与 [[video-use]] 在 AI 视频制作领域互补（web-video 用网页模拟视频，video-use 用 FFmpeg 组装真实视频）。

## 相关项目

- [[conard-li]] — 作者
- [[skills]] — Agent Skills 体系
- [[skill-engineering]] — Skill 工程化设计方法论
- [[huashu-skills]] — 花叔内容创作 Skills 合集
- [[skillopt]] — 微软 Skill 文档自动化优化
- [[claude-design]] — Anthropic AI 设计工具
- [[stitch]] — Google AI 原生 UI 设计平台
- [[stop-slop]] — 反 AI 味写作 Skill
- [[anti-slop-writing]] — 反 AI 味写作概念
- [[ai-slop-design]] — AI 生成 UI 的视觉指纹概念
