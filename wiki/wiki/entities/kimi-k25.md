---
title: Kimi K2.5
created: 2026-05-23
updated: 2026-05-23
type: entity
tags: [model, code, multimodal, open-source]
sources:
  - raw/articles/2026-04-18-stop-vibe-coding-shit-mountain.md
---

# Kimi K2.5

## 概述

Kimi K2.5 是 Moonshot AI（月之暗面）开发的开源视觉编码模型。作为原生多模态模型，K2.5 从训练之初就同时学习视觉和文本，能够将截图、视频或设计稿直接转换为功能性前端代码。

## 核心特性

- **原生多模态**：视觉和文本联合训练，非后期拼接
- **像素级精确**：可从截图生成紧密匹配视觉的前端代码，包括布局、动画、交互、响应式行为
- **开源可用**：通过 Kimi Code 或 Cursor 模型选择器使用
- **前端专长**：在视觉到代码的翻译任务上表现出色

## 在 Vibe Coding 工作流中的定位

Kimi K2.5 在 [[vibe-coding]] 多工具工作流中承担**视觉重的前端实施**角色：

| 阶段 | 工具 | 职责 |
|------|------|------|
| 思考/规划 | Claude | 文档编写、架构规划、产品决策 |
| 视觉前端 | **Kimi K2.5** | 截图→代码、设计稿→实现 |
| 通用实施 | Cursor Agent | 一般编码任务 |
| 调试/完成 | Codex | Bug 修复、代码审查、测试 |

与 [[document-first-system]] 配合使用时，K2.5 读取 FRONTEND_GUIDELINES.md 中定义的设计系统和截 图参考，生成与规范一致的 UI 代码。

## 使用场景

- 将 Figma/Dribbble 截图转换为生产级组件
- 复现特定 UI 风格的精确实现
- 响应式设计的视觉还原
- 与 [[cursor]] 或 Claude Code 配合的多模型工作流

## 成本

Kimi K2.5 是开源模型，通过 Kimi.com 免费访问，API 定价慷慨。降低了 [[vibe-coding]] 的工具成本门槛。

## See Also

- [[vibe-coding]] — 多模型协作的氛围编程范式
- [[document-first-system]] — 先文档后代码的方法论
- [[cursor]] — 支持 K2.5 模型选择的 AI IDE
- [[openai-codex]] — 同类定位的 OpenAI 编码代理
- [[claude-code]] — 架构和文档重的编码 Agent
- [[minicpm5-1b]] — 面壁智能端侧文本基座，端侧模型密度竞赛的领先者


## 在 Vibe Coding 工作流中的角色

klöss 的多工具工作流中，K2.5 定位为**视觉重前端实施专家**：
- 喂截图/设计模型 → 生成像素级匹配的前端代码
- 与 Claude（思考）、Cursor Agent（一般实施）、Codex（调试）互补
- 最佳场景：有 FRONTEND_GUIDELINES.md + 截图参考时的前端实施

### Sources
- raw/articles/2026-04-18-stop-vibe-coding-shit-mountain.md
