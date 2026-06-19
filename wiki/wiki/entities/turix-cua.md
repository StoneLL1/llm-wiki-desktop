---
title: Turix CUA
created: 2026-05-19
updated: 2026-05-22
type: entity
tags:
  - tool
  - agent
  - open-source
sources:
  - https://github.com/nicholasgasior/turix-cua
  - raw/articles/2026-04-21-turix-cua-agent-skill.md
---

# Turix CUA

## 概述

Turix CUA 是领先的开源计算机使用代理（Computer Use Agent，CUA）实现，2.3K+ GitHub stars。它采用双模型架构实现桌面自动化，可以集成到 [[hermes-agent]]、[[openclaw]] 等 Agent 框架中，也可以作为独立的桌面自动化工具使用。

## 核心架构

### 双模型设计
- **turix-brain（视觉理解）**：负责屏幕截图分析，理解当前 GUI 状态
- **turix-actor（GUI 操作）**：根据视觉理解结果生成鼠标/键盘操作

### Agent Skill 集成
Turix CUA 可以作为 Skill 集成到支持 [[skill-engineering]] 的 Agent 框架中：
- 与 [[hermes-agent]] 的集成：通过 SKILL.md 定义 CUA 能力
- 与 [[openclaw]] 的集成：通过 CLAWHUB 分发 CUA Skill
- 与 [[claude-code]] 的集成：通过 [[mcp]] 协议连接

## 技术细节

### 视觉理解能力
- 屏幕截图分析：识别窗口、按钮、文本框等 GUI 元素
- 布局理解：理解元素间的空间关系和层级结构
- 状态判断：判断当前界面状态和可执行操作

### 操作执行能力
- 鼠标控制：点击、拖拽、悬停
- 键盘输入：文本输入、快捷键
- 多应用协同：跨应用的复制粘贴和数据传递

## 应用场景

- **RPA 自动化**：自动化桌面应用的重复操作
- **软件测试**：GUI 自动化测试
- **数据采集**：从无 API 的应用中提取数据
- **跨应用工作流**：连接不同桌面应用的工作流
- **微信操作**：自动通过好友验证、查微信指数、自动聊天
- **音乐播放**：操控 QQ 音乐等媒体应用
- **浏览器操作**：GitHub 提 Issue、网页表单填写

## 与 Mano-P 的对比

| 维度 | Turix CUA | Mano-P |
|------|-----------|--------|
| 双模型架构 | turix-brain + turix-actor | GUI-VLA 单模型 |
| 隐私模式 | 支持本地/云端 | 严格本地优先 |
| 基准表现 | 实用验证 | OSWorld #1 (58.2%) |
| Skill 集成 | 直接作为 Skill 装入 Agent | 通过 mano-skill 集成 |

详见：[[mano-p]]

## 相关链接

- [[computer-use-agent]] — CUA 的通用概念
- [[mano-p]] — 另一个纯视觉驱动的 CUA 实现（本地隐私优先）
- [[claude-code]] — 支持计算机使用的编码 Agent
- [[hermes-agent]] — 可集成 Turix CUA 的 Agent 框架
- [[openclaw]] — 可集成 Turix CUA 的多 Agent 平台
- [[mcp]] — 工具集成协议
- [[agent-building-tutorial]] — Agent 构建实战方法论
