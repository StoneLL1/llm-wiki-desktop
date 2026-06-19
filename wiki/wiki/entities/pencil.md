---
title: Pencil
created: 2026-05-23
updated: 2026-05-23
type: entity
tags: [tool, design, open-source]
sources:
  - raw/articles/2026-04-18-figma-vs-pencil-claude-code.md
---

# Pencil

## Overview

Pencil（pencil.dev）是一个 AI 原生的设计工具，专为 AI 编码环境（如 [[claude-code]]）设计。核心理念是「设计文件直接住在代码仓库里，与代码共享 Git 工作流」，与 [[figma]] 的「设计在云端，向代码延伸」路线完全相反。

## 核心设计

### .pen 文件格式

Pencil 使用 `.pen` 开放文件格式存储设计数据：
- 底层是 JSON 结构
- 版本控制友好，可以直接 `git diff` 看到设计变更
- `.pen` 文件放在项目目录里，与 `src`、`package.json` 平级
- 设计变更和代码变更走完全一样的工程化流程（Git 追踪、PR、Code Review）

### MCP Server

Pencil 自带本地 MCP Server，设计数据不会发到云端（只有 AI prompt 会发给 Claude），隐私方面相对可控。主要工具函数：

| 函数 | 功能 |
|------|------|
| `batch_design` | 批量创建、修改、删除界面元素 |
| `get_screenshot` | 后台渲染预览图，让 AI 用视觉能力做 UI 还原度自检 |
| `snapshot_layout` | 分析盒模型布局结构，检测元素重叠和定位异常 |
| `get_variables` / `set_variables` | 读写设计变量，与 CSS 变量双向同步 |

### AI Multiplayer

Pencil 支持同时跑最多 6 个 AI Agent 在画布上并行工作，社区称之为「AI Multiplayer」。可以让几个 Agent 同时探索不同设计方向，然后挑最好的。

### 组件库支持

内置 Shadcn/UI 等主流组件库支持。AI 在 Pencil 里生成 UI 时被约束在组件库范围内，不会自由发挥出奇怪的自定义组件。

## 与 Figma 的对比

| 维度 | [[figma]] | Pencil |
|------|-----------|--------|
| 设计定位 | 云端设计中枢 | 代码仓库内设计 |
| 文件格式 | 专有格式 | .pen（开放 JSON） |
| Git 集成 | 间接（需导出） | 原生（设计=代码的一部分） |
| 协作模式 | 实时多人 | AI Agent 并行 |
| AI 集成 | MCP Server（桥接） | 原生依赖 Claude Code |
| 设计复杂度 | 企业级 | 轻量级 |
| 适合团队 | 有设计师的成熟团队 | 独立开发者/工程师主导小团队 |

## 适用场景

- **独立开发者/小团队**：没有专职设计师，主要靠 [[vibe-coding]] 搞定全栈。`.pen` 文件直接在仓库里，Git 天然管理版本
- **混合路线**：先用 Pencil 快速搭骨架和跑通逻辑，再导入 [[figma]] 做最终视觉打磨

## Relationships

- 与 [[figma]] 形成互补，分别服务不同场景
- 原生依赖 [[claude-code]] 的 AI 能力
- 通过 MCP Server 与 [[mcp]] 协议集成
- 是 [[vibe-design]] 工作流的重要组成部分
- 与 [[design-md]] 互补：DESIGN.md 管宏观设计规范，Pencil 管具体组件结构

## See Also

- [[figma]] — 对比参照，云端设计工具
- [[claude-code]] — Pencil 的 AI 能力来源
- [[vibe-design]] — Pencil 的设计范式背景
- [[design-md]] — 互补的文本约束方案
- [[mcp]] — Pencil 集成 Claude Code 的协议
