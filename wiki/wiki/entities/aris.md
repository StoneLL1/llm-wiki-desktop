---
title: ARIS (Auto-Research-In-Sleep)
created: 2026-05-23
updated: 2026-05-23
type: entity
tags: [tool, skill, agent, open-source]
sources:
  - raw/articles/2026-04-18-aris-auto-experiment-paper.md
---

# ARIS (Auto-Research-In-Sleep)

## 概述

**ARIS**（Auto-Research-In-Sleep）是一款专为机器学习科研定制的 [[claude-code]] Skills 集合，核心理念是"睡前定方向，醒来收初稿"——让 Claude Code 在用户睡觉时自动完成科研任务。

GitHub: https://github.com/wanshuiyin/Auto-claude-code-research-in-sleep

## 核心特性

- **跨模型协作**：Claude Code 负责执行（读文件、写代码、跑实验），外部 LLM（通过 Codex [[mcp]]）负责评审（打分、找弱点、建议修复），两个模型互不评阅自己的作业
- **17 个可组合 Skill**：自由混搭或串联为完整流水线
- **Human-in-the-loop**：通过 `AUTO_PROCEED` 参数切换全自动或逐步审批模式
- **灵活模型底座**：支持 GLM + GPT、GLM + MiniMax 等替代组合，无需 Claude API

## 三大核心工作流

### 工作流 1：Idea Discovery（文献调研与找 Idea）

```
/research-lit → /idea-creator → /novelty-check → 实现 → /run-experiment
```

- 调研全景（最新论文、开放问题）
- 头脑风暴 8-12 个具体 idea
- 初筛 + 深度验证 top idea
- 并行 pilot 实验
- 输出 `IDEA_REPORT.md`

一键调用：`/idea-discovery "你的研究方向"`

### 工作流 2：Auto Review Loop（自动科研循环）

```
外部 LLM 评审 → Claude Code 修复 → /run-experiment → 收结果 → 再评审 → 循环
```

- 4 轮自主审稿，一夜从 5/10 提升到 7.5/10
- 自动跑 20+ 组 GPU 实验
- 优先改叙事而非跑新实验

一键调用：`/auto-review-loop "你的论文主题"`

### 工作流 3：Paper Writing（论文写作流水线）

```
NARRATIVE_REPORT.md → /paper-plan → /paper-figure → /paper-write → /paper-compile → /auto-paper-improvement-loop
```

- Claims-Evidence 矩阵：每个声明映射到证据
- 自动图表生成（折线图、柱状图、对比表）
- Bib 自动清理（实测 948→215 行）
- De-AI 打磨（去除 AI 写作痕迹）
- GPT-5.4 xhigh 审稿 + 精确页数验证

一键调用：`/paper-writing "NARRATIVE_REPORT.md"`

### 全流程串联

```
/research-pipeline "your research direction"
```

## 安全机制

| 机制 | 说明 |
|------|------|
| MAX_ROUNDS = 4 | 防止无限循环 |
| > 4 GPU-hour 实验自动跳过 | 不会启动超大实验 |
| 优先改叙事 | 选择成本更低的路径 |
| 不隐藏弱点 | 明确规则防止骗高分 |
| 先修后审 | 必须实现修复后再 review |
| 上下文压缩恢复 | 状态持久化到 `REVIEW_STATE.json` |

## 实测成果

- 从 NARRATIVE_REPORT.md 生成 9 页 ICLR 2026 理论论文（7 节、29 条引用、4 张图、2 个对比表）
- 零编译错误、零 undefined reference
- 自动润色循环：3 轮共涨 4.5 分

## 未来规划

- 飞书集成：关键节点消息推送 + idea 审批
- wandb 集成：直接读取训练曲线与 loss 指标
- [[mcp]] 集成：Zotero + [[obsidian]] 深度读取个人文献库

## 相关链接

- [[ai-research-workflow]] — AI 研究工作流系统方法论
- [[claude-code]] — Skills 的运行平台
- [[ai-scientist-v2]] — 另一条全自动科研路线
- [[skill-engineering]] — Skill 工程化设计方法
- [[mcp]] — Codex MCP 用于跨模型协作
