---
title: huashu-skills
created: 2026-05-22
updated: 2026-05-22
type: entity
tags:
  - tool
  - design
  - typography
  - skill
  - open-source
sources:
  - raw/articles/2026-04-18-20-ai-creation-skills.md
---

# huashu-skills

## 概述

huashu-skills 是花叔（alchaincyf）开源的一套 **20 个内容创作 Skills**，覆盖选题、调研、写作、审校、配图、排版、发布的完整内容创作链路。源自 Claude Code 生态，同时兼容 WorkBuddy 等国产 AI 客户端。

- GitHub: https://github.com/alchaincyf/huashu-skills
- 20 个 Skill 的 README 总字数超过 4 万字，文档完成度极高
- 15 个纯文档型 Skill（零依赖零配置），5 个带脚本依赖

## 七大类 Skill

### 端到端工作流（4 个）

| Skill | 功能 | 亮点 |
|-------|------|------|
| `huashu-slides` | PPT 生成 | 5 阶段自动推进，18 种设计风格，输出标准 PPTX，两条技术路径（HTML 可控 / AI 视觉） |
| `huashu-data-pro` | 数据分析报告 | Excel → 带图表的交互式 HTML 报告，5 套报告风格库（FT/McKinsey/Economist/Goldman/Swiss） |
| `huashu-design` | 设计配图 | 20 种设计哲学 + 5 大流派 → 推荐方向 → 并行生成 Demo → 5 维度专家评审 |
| `huashu-douyin-script` | 短视频脚本 | 竞品下载 → AI 分析提炼爆款公式 → 脚本+分镜 → 审校 |

### 写作与审校（4 个）

- **huashu-proofreading** — 三遍审校：事实核查+逻辑链 → AI 腔识别改写 → 节奏打磨。经审后 AI 检测率可压到 30% 以下
- **huashu-material-search** — 1800+ 条个人素材库，写作时检索真实经历和观点，自动改写适配长文
- **huashu-topic-gen** — 选题生成，每个方案含标题/大纲/优劣分析/工作量评估
- **huashu-article-to-x** — 长文浓缩为短内容（适配 X/微博/小红书），按平台逻辑重写

### 选题与调研（3 个）

- **huashu-research** — 结构化调研，搜一轮存一轮，成果实时落盘
- **huashu-info-search** — 信息搜索，自动过滤过时信息，按官方>科技媒体>社区优先级排序

### 视频创作（3 个）

- **huashu-video-check** — 用 MrBeast 策略框架检查标题和封面（5 种强对比公式）
- **huashu-video-outline** — 快速出大纲方案（2-3 个方案带优劣对比）
- **huashu-script-polish** — 书面脚本改口播版，标停顿和重音

### 配图和文档工具（5 个）

- **huashu-wechat-image** — 公众号配图
- **huashu-xhs-image** — 小红书配图
- **huashu-md-to-pdf** — Markdown 转苹果设计风格专业 PDF（书籍级排版，自动封面/目录/页眉页脚）
- **huashu-speech-coach** — 基于 MIT 教授 Patrick Winston 的演讲方法论
- **huashu-prompt-save** — 自动分类保存 prompt（5 大分类带索引）

## 脚本依赖

仅 5 个 Skill 需要额外安装依赖：

| Skill | 依赖 |
|-------|------|
| huashu-slides | `python-pptx` |
| huashu-md-to-pdf | `markdown` + `weasyprint` |
| huashu-data-pro | `openpyxl` + `pptxgenjs` |
| huashu-douyin-script | `yt-dlp` |
| huashu-image-upload | 零依赖 |

核心提示词逻辑全在 SKILL.md 中，脚本跑不起来仍能完成 80% 的工作。

## 注意事项

3 个 Skill 原始依赖 Gemini API 生图（huashu-wechat-image、huashu-xhs-image、huashu-douyin-script）。可通过在 SKILL.md 顶部加声明去除 API 依赖，改为输出文生图提示词。

## 生态定位

花叔的 huashu-skills 侧重**内容创作全链路**，与宝玉（微信生态发布）和归藏（工程协作）的 Skill 合集不冲突，可同时安装使用。其 SKILL.md 文档完整度极高，错误处理、降级策略、边界 case 全覆盖。

[[garden-skills]]（ConardLi，7K Star）是另一个值得关注的 Skills 合集，侧重**视觉产出**（视频/网页/图片生成），与 huashu-skills 的内容创作定位形成互补——前者解决「产出什么视觉」，后者解决「写什么内容」。

## 相关链接

- [[claude-code]] — Skill 原始运行平台
- [[skills]] — Skill 工程化设计体系
- [[guizang-ppt-skill]] — 歸藏的杂志风 PPT Skill
- [[md2pdf-skill]] — LovStudio 的 PDF 排版 Skill（对比 huashu-md-to-pdf）
- [[ppt-master]] — AI 演示文稿生成工具
- [[skill-engineering]] — Skill 工程化设计方法论
