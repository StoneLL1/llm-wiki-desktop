---
title: oh-my-codex
created: 2026-05-22
updated: 2026-05-27
type: entity
tags: [tool, multi-agent, open-source]
sources:
  - raw/articles/2026-04-18-11-hot-github-projects-this-week.md
---

# oh-my-codex

## 概述

oh-my-codex 是 [[openai-codex]] 的多 Agent 编排系统，由 [[oh-my-claudecode]] 的同一作者 Yeachan Heo 开发。将多 Agent 编排理念移植到 OpenAI Codex CLI 上。两个月从零冲到 1.4 万 Star。

## 核心特性

- **30 个专业 Agent 角色 + 40+ Skill**：比 oh-my-claudecode 更丰富的 Agent 和 Skill 生态
- **tmux 并行 Worker**：在 tmux 中启动最多 20 个 Worker 并行工作
- **独立 git worktree**：每个 Worker 在独立的 git worktree 中运行，互不干扰
- **混合模型协作**：支持同时使用 Codex 和 Claude 的 Worker，两家模型协作

## 在多 Agent 生态中的定位

oh-my-codex 与 [[oh-my-claudecode]] 是姊妹项目，分别面向 OpenAI Codex 和 Claude Code 生态。它展示了 [[multi-agent-collaboration]] 理念跨平台移植的可行性，以及混合使用不同厂商模型的实践。

## 相关链接

- [[openai-codex]] — OpenAI 的编程 Agent
- [[oh-my-claudecode]] — 同一作者的 Claude Code 多 Agent 编排系统
- [[multi-agent-collaboration]] — 多 Agent 协作范式
