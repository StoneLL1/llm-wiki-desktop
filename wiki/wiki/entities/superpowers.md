---
title: Superpowers (Claude Code Skill)
created: 2026-05-22
updated: 2026-05-22
type: entity
tags:
  - tool
  - agent
  - open-source
sources:
  - raw/articles/2026-04-18-everything-claude-code-plugin-library.md
  - raw/articles/2026-04-18-claude-code-10-more-worthwhile-skills.md
  - raw/articles/2026-04-20-10-claude-code-best-practices.md
---

# Superpowers

## 概述

Superpowers 是 [[claude-code]] 社区最完整的多 agent 开发工作流 skill 集合，由 **Jesse Vincent** 创建。GitHub 131k Star。

安装方式：
```
npx skills add obra/superpowers@using-superpowers -g -y
```

## 核心理念

Superpowers 的核心不在于提供多少工具，而在于定义了一套完整的**软件开发方法论**。它将软件开发的完整生命周期拆分为可衔接的步骤：

1. **需求发散** — brainstorm 阶段，明确方向
2. **规格确认** — plan 阶段，锁定范围
3. **计划拆分** — 拆分为可执行的子任务
4. **子 agent 执行** — 自动分配给专属 subagent
5. **代码 review** — 合并前的质量把关
6. **合并** — 确认后集成

每个环节都有专属 skill 负责，自动触发，自动接力。

## 核心优势

### 子 Agent 驱动

防止长任务里的上下文漂移。每个 subagent 有独立的 context window，完成后只把结果带回主会话。

### TDD 强制

要求**先写测试，再写实现**。这确保代码从设计之初就考虑可测试性，整体质量显著提升。

### Code Review 关卡

合并之前必须经过 review。Claude 不会跳过这个环节直接合并代码。

### Git Worktree 隔离

自动开 git worktree 做隔离开发，保证主分支不受实验性修改影响。

## 与 Everything Claude Code 对比

| 维度 | Superpowers | Everything Claude Code |
|------|-------------|----------------------|
| 定位 | 资深架构师/方法论 | 装备齐全的工具箱 |
| 核心价值 | 流程约束、方法论 | 场景覆盖、工程深度 |
| 管理方式 | 管开发流程 | 简工具 |
| 适用场景 | 中大型项目 | 全规模项目 |

**最佳搭配**：Superpowers 管流程（确保开发节奏是对的），[[everything-claude-code]] 管工具（确保每个环节都有最好的 agent）。

## 适用与不适用场景

### 适用

- 多文件、多步骤的大型开发任务
- 需要严格代码质量保障的项目
- 团队协作中的 AI 辅助开发

### 不适用

- 小任务（改个变量名、修个小 bug）— 原生 [[claude-code]] 处理更快
- 三五分钟能做完的事不需要走完整 Plan → Execute → Review 流程

## Relationships

- 是 [[skills]] 生态中最知名的工作流 skill
- 与 [[everything-claude-code]] 互补
- 基于 [[claude-code]] 的 subagent 机制
- 体现了 [[harness-engineering]] 的方法论

## See Also

- [[claude-code]] — 运行平台
- [[everything-claude-code]] — 互补的工具箱
- [[skills]] — Skills 生态系统
- [[harness-engineering]] — 方法论基础
