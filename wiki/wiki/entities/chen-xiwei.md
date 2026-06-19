---
title: 陈希伟
created: 2026-06-02
updated: 2026-06-02
type: entity
tags:
  - person
  - research
sources:
  - raw/articles/2026-05-30-harness-research-reflection.md
---

# 陈希伟

## 概述

陈希伟是 Datawhale 社区的独立研究者，专注于长程 LLM Agent 的可靠性问题。在 CMU/Yale 发布 Harness Engineering 综述论文后，他提出了 [[state-aware-runtime|State-Aware Runtime]]（状态感知运行时）的概念框架，将其定位为 Harness Engineering 的下一步演进方向。

## 研究定位

> 关注长程 LLM Agent 中的状态保持、程序遵循、过程审计、门控与回滚机制，并将其理解为 State-Aware Runtime 问题，而不是单纯的 Prompt Engineering 或 Memory Augmentation 问题。

作为资源有限的独立研究者，他拒绝硬拼模型训练或冲榜 Benchmark，而是深耕系统失败分析——拼的不是千卡 GPU 阵列，而是对系统失败极度的敏感与耐心。

## 核心贡献

### State-Aware Runtime 框架

提出了超越 [[harness-engineering|Harness Engineering]] 的下一个 Agent 工程范式，四大核心原则：

1. **严格区分候选输出与已提交状态** — 错误可以发生在候选层，但不能穿透到已提交层
2. **长上下文 ≠ 长期状态管理** — 谁有权修改状态？污染的状态如何隔离恢复？
3. **Trace-Native Evaluation** — 真正的失败轨迹比完美的 Demo 有价值得多
4. **独立研究者的壁垒方向** — 在"算力即正义"的时代，找到适合小团队的差异化切入点

### 五大研究方向

通过不同领域的独立研究汇聚到 State-Aware Runtime：

- **规范推理**：答案正确和过程忠实之间的断裂（Procedural Fidelity）
- **长篇叙事 Agent**：角色知识的时序管理（Epistemic Memory）
- **多 Agent 社会交互**：运行环境对 Agent 行为的塑形作用
- **结构化生成**：语言流畅和结构忠实之间的断裂
- **游戏 Agent Runtime**：自由对话和世界状态提交之间的边界

## 发表平台

文章发表在 Datawhale 微信公众号（2026-05-30），Datawhale 是知名的 AI 学习社区，此前出品过 [[vibe-coding-course]]（1 万+ Star）等高质量教程。

## 相关链接

- [[state-aware-runtime]] — 核心概念
- [[harness-engineering]] — 概念的前序阶段
- [[long-running-agent]] — 长程 Agent 是 State-Aware Runtime 的重点场景
- [[vibe-coding-course]] — 同属 Datawhale 社区出品
