---
title: PPT Master
created: 2026-05-19
updated: 2026-05-23
type: entity
tags: [tool, presentation, open-source]
sources:
  - raw/articles/2026-04-18-ppt-master-ai-editable-pptx.md
  - raw/articles/video-use-claude-code-video-editing.md
---

# PPT Master

## Overview

**PPT Master** 是由 Hugo He 开发的开源 AI 演示文稿生成工具（v2.3.0），核心差异点是：**输出原生可编辑的 PPTX 文件**——包含真实的形状、文本框和图表，而非图片截图。任何元素都可以直接点击编辑。

GitHub: [hugohe3/ppt-master](https://github.com/hugohe3/ppt-master) | MIT 协议

## Why PPT Master

大多数 AI 演示工具的问题在于：
- 导出的是**图片或网页截图**，看起来好看但无法编辑
- 生成的只是**裸文本框和项目符号列表**，缺乏设计感
- 需要**月度订阅**，上传文件到服务器，被平台锁定

PPT Master 的不同之处：

| 维度 | PPT Master | 其他 AI PPT 工具 |
|------|-----------|----------------|
| 输出格式 | 原生 DrawingML，可编辑 | 图片/Web 截图 |
| 成本 | 免费开源 + 自选 AI 编辑器（低至 $0.08/套） | 月度订阅 |
| 数据安全 | 本地运行（除 AI 模型通信外） | 上传到服务器 |
| 平台绑定 | 无锁定，支持 Claude Code/Cursor/Copilot | 平台锁定 |
| 编辑自由度 | 全元素可编辑（真实形状+文本框+图表） | 受限或不可编辑 |

## 核心特性

### 多格式输入

支持 PDF、DOCX、URL、Markdown 等多种输入格式：
- 文件放入 `projects/` 目录，在 AI 聊天面板指定即可
- 也支持直接粘贴文本内容
- 微信公众号文章通过 `curl_cffi` 原生支持

### 多风格模板

提供 15 个示例项目、229 页模板，涵盖：
- **Magazine** — 暖色调，照片丰富
- **Academic** — 结构化研究格式，数据驱动
- **Dark Art** — 电影感暗背景，画廊美学
- **Nature Documentary** — 沉浸式摄影，极简 UI
- **Tech / SaaS** — 白色卡片，定价表布局
- **Product Launch** — 高对比度，规格参数突出

### AI 图片生成（可选）

支持多种图片后端：
- Gemini（推荐）、OpenAI、Qwen、智谱、火山引擎、Stability、BFL、Ideogram、SiliconFlow、Fal、Replicate
- 通过 `.env` 文件配置 API Key

### 双输出文件

每次生成保存到 `exports/` 目录：
- **原生形状 .pptx** — 可直接编辑（需 Office 2016+）
- **_svg.pptx** — SVG 快照，用于视觉参考

## 技术细节

- Python 3.10+ 唯一前置依赖
- AI 编辑器推荐：Claude Code（最佳）> Cursor/VS Code Copilot > Codebuddy IDE
- 支持 Claude、GPT、Gemini、Kimi 等多种模型
- 核心工作流通过 `SKILL.md` 定义，AI 失去上下文时可重读恢复

## 创作者背景

Hugo He 是金融从业者（CPA · CPV · 投资咨询工程师），因为日常需要制作和审阅大量投资/咨询演示文稿，对"AI 输出图片而非可编辑幻灯片"这一痛点有切身体会。PPT Master 是他将领域专业知识转化为开源工程的尝试。

## Relationships

- 与 [[video-use]] 管道集成，支持从原始素材生成演示内容
- 作为 [[skills]] 加载到 [[claude-code]] 和 [[openclaw]] 中使用
- 与 [[open-slide]] 定位相似但技术路线不同——PPT Master 输出原生 PPTX
- 与 [[guizang-ppt-skill]] 互补——后者偏杂志风视觉，PPT Master 偏完整 PPTX 工作流
- 设计理念与 [[vibe-design]] 一致——自然语言描述 → 设计输出

## See Also

- [[video-use]] — AI 视频编辑管道
- [[claude-code]] — 推荐的 AI 编辑器
- [[open-slide]] — Web 版 PPT 生成工具
- [[guizang-ppt-skill]] — 杂志风 PPT 生成 Skill
- [[skills]] — SKILL.md 模块化能力框架
- [[openscreen|OpenScreen]]
