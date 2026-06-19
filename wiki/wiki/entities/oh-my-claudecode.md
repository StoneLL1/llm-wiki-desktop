---
title: oh-my-claudecode
created: 2026-05-22
updated: 2026-05-27
type: entity
tags: [tool, multi-agent, open-source]
sources:
  - raw/articles/2026-04-18-11-hot-github-projects-this-week.md
---

# oh-my-claudecode

## 概述

oh-my-claudecode 是 [[claude-code]] 的多 Agent 编排系统，提供 19 个专业化 AI Agent，包括架构师、规划师、执行者等角色，自动将任务拆解分派给合适的 Agent 处理。目前 1.1 万 Star。由 Yeachan Heo 开发。

## 核心特性

- **19 个专业 Agent**：架构师、规划师、执行者等专业化角色分工
- **Team Mode**：最推荐的模式，一句话启动完整开发流水线——需求分析→代码生成→测试验证
- **智能模型路由**：简单任务自动用 Haiku 省钱，复杂推理用 Opus，节省 30-50% Token
- **Skill 学习系统**：从开发过程自动提取调试知识和模式，下次遇到类似问题自动注入上下文
- **三步安装**：通过 Claude Code 插件命令即可完成安装

## 在多 Agent 生态中的定位

oh-my-claudecode 是 [[multi-agent-collaboration]] 在 Claude Code 生态中的实践之一。与 [[openclaw]]、[[hermes-agent]] 等独立平台不同，oh-my-claudecode 作为 Claude Code 的插件运行，在不离开 Claude Code 生态的情况下实现多 Agent 协作。其姊妹项目 [[oh-my-codex]] 将类似理念移植到了 OpenAI Codex CLI。

## 相关链接

- [[claude-code]] — Anthropic 的 CLI 编码 Agent
- [[oh-my-codex]] — 同一作者的 OpenAI Codex 多 Agent 编排系统
- [[multi-agent-collaboration]] — 多 Agent 协作范式
- [[skills]] — Skill 学习系统与 Claude Code 原生 Skill 的关系
