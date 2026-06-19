---
title: 前端设计工作流方法论
created: 2026-05-27
updated: 2026-05-27
type: entity
tags: [tool, design, workflow, agent]
sources:
  - raw/articles/2026-05-19-beautiful-practical-frontend-guide.md
---

# 前端设计工作流方法论

## 概述

由 Mav 高未央总结的前端设计工作流，解决核心问题：**如何让缺乏美学直觉的人做出优雅且实用的前端界面**。核心理念是「让风格服务于目的，把美学决策交给 AI，确保设计可复现」。

## 三个观念矫正

1. **优秀的设计不只是美** — 复古风格适合音乐分享 App，但不适合体育网站
2. **降低自由度** — 缺乏美学训练的开发者应把美学决策交给 AI
3. **可复现才是实用** — 靠灵光一现的美不实用，需要可重复的流程

## 工具清单

| 工具 | 用途 |
|------|------|
| Antigravity（或任意支持 agent/skill 的 coding agent） | 工作流执行平台 |
| [[stitch]] | 生成前端设计 |
| ia-planner workflow | 信息架构设计 |
| visual-explorer workflow | 视觉方案探索 |
| ux-designer workflow | 动态部分设计 |
| 即梦（或其他生图模型） | 美术素材生成 |

## 完整四步流程

### 第一步：生成 IA Document（信息架构图）
- 将 PRD 交付给 ia-planner workflow
- ia-planner 理解项目受众、立意、功能设计
- 设计网站/App 的基础架构、页面规划、信息流
- 生成线框图供确认（确认后删除，不限制后续设计发挥）
- 交付物：概述 + sitemap + 用户路径流向 + 核心模块清单

### 第二步：生成静态 Visual Schema（视觉方案）
- 将 PRD 交给 visual-explorer workflow
- 从 PRD 和 IA 向视觉设计方案过渡
- 确定色彩、动态和整体设计调性
- 交付物类似 [[design-md]]

### 第三步：Stitch 完成设计
- 依次将 IA Document 和 Visual Schema 交给 [[stitch]]
- 尝试多个方案，选择最喜欢的设计
- Export → Code to clipboard → 交给 [[claude-code]] 生成

### 第四步：完成动态部分设计
- 调用 ux-designer workflow
- 创意概念动态表达
- 设计音效、冲击力动画、美术素材生图提示词
- 可选：HTML in Canvas 改造（追求高级动态效果）
- 交付完整动态交互方案 + 生图提示词文档

## 设计哲学

一个美观的前端界面 **70% 取决于底图和动效**。工作流的核心思想是「降低自由度、让 AI 做美学决策、确保可复现」——每个 workflow 的子功能（如生成线框图、色彩情绪版）都可以单独做成 [[skills]]。

## Relationships

- 实践 [[vibe-design]] 范式的完整工作流
- 使用 [[stitch]] 作为核心设计工具
- 深度集成 [[claude-code]] / coding agent
- 与 [[design-md]] 方法论互补

## See Also

- [[vibe-design]] — Vibe Design 设计范式
- [[stitch]] — Google AI 原生设计平台
- [[design-md]] — DESIGN.md 设计规范格式
- [[skills]] — 可模块化的能力单元
