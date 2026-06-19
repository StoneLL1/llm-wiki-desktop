---
title: Logo Generator Skill
created: 2026-05-22
updated: 2026-05-22
type: entity
tags:
  - tool
  - design
  - skill
  - open-source
sources:
  - raw/articles/2026-04-18-ai-logo-icon-generation-skill.md
---

# Logo Generator Skill

## 概述

Logo Generator Skill（op7418/logo-generator-skill）是一个开源 AI Agent Skill，帮助开发者快速生成「够用的好 Logo」和专业展示图。核心思路是用 AI（推荐 Gemini）生成可编辑的 SVG 基础 Logo，再用 AI 生成高级展示图，两步结合保证可控性和视觉效果。

- GitHub: https://github.com/op7418/logo-generator-skill
- 推荐在 Gemini CLI 或其他 Gemini 驱动的 Agent 中使用

## 三步工作流

### 第一步：信息收集

Skill 收集四个维度的设计信息：

- **产品名称**
- **行业/类别**（AI、金融科技、设计工具等）
- **核心概念**（连接、流动、安全、简洁等）
- **设计偏好**（极简/复杂、冷色/暖色、专业/友好）

也可以直接将项目介绍发给 AI，让 Skill 自行提取设计要素。核心理念：**好的设计来自理解，而不是随机生成**。

### 第二步：生成 6+ 设计变体

Skill 自动匹配设计模式库，生成至少 6 个不同风格的 SVG Logo：

- 每个变体生成交互式网页，可在浏览器中对比查看
- 不满意的可以要求「换一个」，Skill 会套用其他设计模式
- 可提供具体指导意见进行修改

### 第三步：高级展示图

选好 Logo 后提供两种展示方案：

#### 方案 1：Nano Banana 静态展示（12 种背景）

**暗色系（6 种）**：The Void、Frosted Horizon、Fluid Abyss、Studio Spotlight、Analog Liquid、LED Matrix

**亮色系（6 种）**：Editorial Paper、Iridescent Frost、Morning Aura、Clinical Studio、UI Container、Swiss Flat

需要 AI Studio API 或第三方 Nano Banana API。

#### 方案 2：WebGL 动态背景（6 种交互式）

LED Matrix、Fluid Warping、Fabric Wave、Off-Center Ripple、Holographic Dispersion、Spiral Vortex。

优势：动态交互（鼠标响应）、无限缩放、60 FPS、生成 HTML 代码直接嵌入网页。

## 为什么先生成 SVG 再生成展示图

直接用图片模型生成 Logo 的局限：

1. **控制精度差** — 无法精准控制圆角半径、间距等参数
2. **无法编辑** — 位图无法调整颜色、形状
3. **不是矢量** — 放大模糊，无法做响应式设计

SVG 的优势：代码格式可复制到 [[figma]] 精修、可做设计体系和动效、矢量无损适配、可构成「AI 生成基础 + 人工精修细节」工作流。

## 交付物

- SVG 文件（可编辑矢量）
- PNG 导出（1024×1024、2048×2048 等）
- 展示图（4 种专业背景）
- 交互式网页（随时查看和对比所有变体）

## 使用场景

- Vibe Coding 项目图标（不需要独特性，但要专业干净）
- 创业团队早期品牌（预算有限但需要视觉资产）
- 设计师辅助工具（快速生成多个方案或灵感来源）
- 12 种背景风格可用于网页设计、PPT 背景、产品截图展示

## 相关链接

- [[claude-code]] — Skill 运行平台之一
- [[figma]] — SVG 可导入 Figma 进行精修
- [[vibe-design]] — AI 辅助设计范式
- [[skills]] — Skill 工程化设计体系
