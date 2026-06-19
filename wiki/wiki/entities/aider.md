---
title: Aider
created: 2026-05-21
updated: 2026-05-22
type: entity
tags: [tool, code, agent, open-source]
sources:
  - raw/articles/2026-05-21-xhs-agent-projects-recommendation.md
---

# Aider

## 概述

Aider 是一个开源的 AI 结对编程命令行工具，44k+ Star，Apache 2.0 许可。允许开发者在终端中与 LLM 协作编辑代码。它支持直接编辑本地 Git 仓库中的代码，自动生成有意义的 commit message。

## 核心特性

- **终端原生**：完全在命令行中运行，适合开发者工作流
- **Git 集成**：自动管理代码变更，生成规范的 commit
- **多模型支持**：支持 OpenAI、Anthropic 等多家 LLM 提供商的模型
- **多文件编辑**：可以同时理解和编辑多个代码文件
- **仓库映射**：自动索引代码仓库，理解项目结构

## Repo Map 技术亮点

Aider 最值得学习的技术是 **repo map**——用 tree-sitter 解析代码，提取每个文件里的类、函数、关键定义，再用图算法算出哪些符号和当前任务最相关，只把这些塞进 context。这是"为什么不能直接把整个仓库塞给模型"问题最早的开源答案之一。

## 在 Agent 生态中的定位

Aider 是 AI 编码工具领域的重要开源项目，与 [[claude-code]]、[[cursor]] 形成竞争关系。它更注重"结对编程"模式——开发者与 AI 实时协作，而非让 AI 独立完成全部编码任务。

## 适用场景

- 日常编码辅助
- 代码重构
- Bug 修复
- 新功能开发

## 相关链接

- [[claude-code]] — Anthropic 的 CLI 编码 Agent
- [[gpt-researcher]] — 另一个推荐的 Agent 项目（深度研究）
- [[openclaw]] — 开源多 Agent 平台
