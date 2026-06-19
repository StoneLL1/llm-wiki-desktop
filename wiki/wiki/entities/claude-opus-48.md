---
title: Claude Opus 4.8
created: 2026-05-30
updated: 2026-05-30
type: entity
tags: [model, architecture, benchmark]
sources:
  - raw/articles/2026-05-29-claude-opus-48-dynamic-workflow一次性并行上百个subagents.md
---

# Claude Opus 4.8

## 概述

Claude Opus 4.8 是 [[anthropic]] 于 2026-05-29 发布的最新旗舰模型，与 Claude Code v2.1.154+ 同步推出。本次更新的最大亮点是**诚实度（honesty）** 的显著提升，以及对 [[claude-code-dynamic-workflow]] 动态工作流的原生支持。

## 核心改进

### 诚实度（Honesty）

Opus 4.8 专门针对「模型写错代码后仍自信宣称完成」的问题做了训练优化。官方数据显示，模型对自己代码缺陷「放过不提」的概率比 4.7 降低了约 4 倍——在没把握的地方会主动停下来告知「这块我没把握」，而非蒙混过关。

实测中，当 dynamic workflow 的核查阶段 75 个 subagents 批量崩溃时，Opus 4.8 并未简单将 17 条未完成核查的结论判定为「不可信」，而是主动分析后将结果分为三档：已核实、未核实但大概率为真、被明确反驳——体现了诚实的工程价值。

### 基准测试

Anthropic 使用同一套公开 harness 对比了 4.8、4.7、GPT-5.5、Gemini 3.1 Pro（DeepSeek 未参与）：

| 基准测试 | Opus 4.8 | Opus 4.7 | GPT-5.5 | Gemini 3.1 Pro |
|---------|----------|----------|---------|----------------|
| SWE-Bench Pro（编程）| **69.2%** | 64.3% | 58.6% | 54.2% |
| OSWorld-Verified（CUA）| **全面领先** | — | — | — |
| GDPval-AA（知识）| **全面领先** | — | — | — |
| Terminal-Bench 2.1（Agent 终端编码）| 74.6% | — | **78.2%** | — |

编程与多数指标仍是 Claude 主战场，但 Terminal-Bench 2.1 上 GPT-5.5 反超。

## 版本迭代节奏

Claude Code 从 4.5 → 4.6 → 4.7 → 4.8 基本保持 **1 个多月一次小版本迭代**。

## 配套发布

- **Claude Code Dynamic Workflow**（研究预览）：见 [[claude-code-dynamic-workflow]]
- **244 页 System Card**：安全评估文档
- **Mythos 预告**：Anthropic 计划未来几周发布此前仅少部分组织可用的「地表最强模型」Mythos

## 相关页面

- [[claude-model-family]] — Claude 模型家族总览
- [[claude-code-dynamic-workflow]] — 同步发布的动态工作流功能
- [[claude-code]] — Claude Code 编程 Agent
- [[anthropic]] — Anthropic 公司
