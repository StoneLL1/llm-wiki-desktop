---
title: md2pdf-skill
created: 2026-05-22
updated: 2026-05-22
type: entity
tags:
  - tool
  - typography
  - skill
  - open-source
sources:
  - raw/articles/2026-04-18-beautiful-pdf-typesetting-skill.md
---

# md2pdf-skill (lovstudio/md2pdf)

## 概述

md2pdf 是由 LovStudio（作者 Mark）开发的开源 Claude Code Skill，用于将 Markdown 直接转换为出版级 PDF。核心是一个 817 行的 Python 脚本 `md2pdf.py`，基于 reportlab 构建，零重依赖（仅需 `pip install reportlab`，5MB），可在 Claude Code sandbox 中直接运行。

该项目诞生于 Claude Code 源码泄露事件后的实际需求——将 10 万字技术报告转化为专业 PDF 分发格式，一天内从零完成并经过 136 页 / 7.9 MB 的实战验证。

- GitHub: https://github.com/lovstudio/md2pdf
- 安装: `npx skills add lovstudio/md2pdf`

## 核心能力

### CJK 双层混排

md2pdf 最硬核的特性是 **CJK 字符级精确混排**，解决了 reportlab 不支持 fallback 字体的问题：

- **第一层 `_font_wrap()`**：在 Paragraph 层面逐字符扫描，检测 CJK Unicode 范围（U+4E00–U+9FFF 等），动态插入 `<font face="CJK">` 标签。中文用思源宋体、英文用 Carlito，切换精确到单个字符。
- **第二层 `_draw_mixed()`**：在 Canvas 层面（封面标题、页眉页脚等），实现 CJK/Latin 逐段切换绘制。

这是唯一同时解决 Paragraph 和 Canvas 两个渲染层 CJK 问题的方案（在 6 个 Skill 平台、13,000+ skills 中无第二家）。

### 全链路文档结构

从封面到封底的完整链路，20+ 个 CLI 参数零代码配置：

- 封面 / 扉页 / 目录 / 页眉页脚 / PDF 书签导航 / 水印 / 封底
- 代码块保真（`esc_code()` 保留缩进和换行，30 行自动截断）
- 智能表格列宽（按内容比例分配，保障最低宽度）

### 主题系统

10 种设计风格，每种有独立的**版式语言**（非简单换配色）：学术论文、期刊风格、中式正式、极简水墨等。支持 `--theme` 参数切换和 `--theme-file mytheme.json` 完全自定义。

### AI 生图集成

支持调用 `lovstudio:image-gen`（基于 nano-banana-pro 等模型）根据文档内容自动生成扉页插图，支持品牌标识全链路植入（`--banner`、`--header-title`、`--footer-left` 等参数）。

## 竞品对比

在 Anthropic Skills、SkillsMP.com、LobeHub、OpenClaw、Microsoft Skills、FastMCP.me 六大平台扫描了 12 个 MD→PDF 相关竞品，核心差异：

| 方案 | Markdown 原生 | CJK 支持 | 全链路结构 | 零重依赖 |
|------|:---:|:---:|:---:|:---:|
| **md2pdf** | ✅ | ✅ | ✅ | ✅ |
| Pandoc + LaTeX | ⚠️ | 需配置 | 部分 | ❌ (4GB TeX Live) |
| WeasyPrint | ⚠️ | 重灾区 | ❌ | ❌ (cairo/pango) |
| Puppeteer | ✅ | ✅ | ❌ | ❌ (Chromium 300MB) |
| MiniMax Office Skills | 二等公民 | 不保证 | ❌ | ❌ (Playwright) |

md2pdf 是 **Markdown 原生全功能排版 + CJK 专项支持 + 全链路文档结构 + 零重依赖** 交叉点上的唯一方案。

## 设计哲学

暗合一泽提出的 Skill 设计公式：**Agent 策略哲学 + 最小完备工具集 + 必要的事实说明**：

- **策略哲学**：Markdown 是最自然的中间格式，AI Agent 天然输出 Markdown，让 Markdown 成为一等输入
- **最小完备工具集**：一个 Python 脚本 + `pip install reportlab`，20+ CLI 参数
- **必要的事实说明**：SKILL.md 详细记录每个参数用途、Hard-Won Lessons、主题配色参考

## 相关链接

- [[claude-code]] — 作为 Skill 在 Claude Code 中运行
- [[skills]] — Skill 工程化设计体系
- [[guizang-ppt-skill]] — 另一个内容排版 Skill（PPT 方向）
- [[huashu-skills]] — 花叔的创作 Skill 合集（含 huashu-md-to-pdf）
- [[ppt-master]] — AI 演示文稿生成工具
