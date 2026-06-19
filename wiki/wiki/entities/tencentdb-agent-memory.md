---
title: TencentDB Agent Memory
created: 2026-05-23
updated: 2026-05-27
type: entity
tags: [agent, architecture, open-source, company]
sources:
  - raw/articles/2026-05-22-tencent-agent-memory-token-saving-mermaid.md
---

# TencentDB Agent Memory

## 概述

TencentDB Agent Memory 是腾讯推出的 Agent 记忆系统，分为**短期任务记忆**和**长期个性化记忆**两层。短期记忆通过「上下文卸载 + Mermaid 无限画布」的组合方案，在超长 Session 中最高节省 **61% Token**，任务通过率从 33% 提升到 50%（相对提升 52%）。已在 GitHub 开源：`Tencent/TencentDB-Agent-Memory`。

## 核心方案：短期记忆压缩 = 上下文卸载 + Mermaid 无限画布

### 压缩的艺术：从稀疏到稠密

对大模型而言，\"压缩\"的定义是用更少的 token 表达等量（或近似等量）的语义——把稀疏信息提炼成稠密信息。极致的压缩不只去掉冗余文本，而是**改变信息的表示形式本身**。

大模型的一个被严重低估的能力是它能高效理解\"符号\"——如 MBTI 四个字母能激活大量语义。但符号压缩的前提是模型在训练数据中大量接触过，且不同模型/上下文理解一致。

因此，TencentDB Agent Memory 选择 Mermaid 作为压缩载体，基于三条设计原则：

1. **通用知识** — 所有主流 LLM 在预训练阶段大量接触过的格式
2. **生成不过于复杂** — 拓扑结构决定语义，不依赖文本约定
3. **表达足够自由** — 模型可自主决定合并、拆分、标注节点

### 上下文卸载

将暂时不需要直接推理的内容搬到上下文窗口之外（文件系统），上下文里只保留摘要、路径或索引。需要细节时再通过索引找回原文。

Anthropic 实验显示：当 context 长度超过 max window 的 **80%** 时发生 [[context-rot]]，模型注意力涣散。

Claude 的 Skills、[[manus]] 的文件系统即上下文、Claude Code 的压缩实践，都体现了上下文卸载的思想。

### 为什么单独用上下文卸载或无限画布都不够？

上下文卸载解决了\"信息太长\"的问题，但不一定解决\"结构丢失\"的问题。线性排列的摘要和文件路径，Agent 很难判断任务走到哪一步、哪些信息属于同一个子任务。

反过来，Mermaid 画布很适合表达任务结构，但本质上是任务骨架，不适合承载所有原始细节。

**真正适合长任务的短期记忆压缩 = 细节可恢复 + 结构不丢失。**

### Mermaid 无限画布

用 Mermaid Flowchart（而非 StateDiagram）将任务执行过程转化为可导航的结构化记忆。

**选择 Mermaid 的三条设计原则：**
1. **通用知识** — 所有主流 LLM 在预训练阶段大量接触过的格式
2. **生成不过于复杂** — 拓扑结构决定语义，不依赖文本约定
3. **表达足够自由** — 模型可自主决定合并、拆分、标注节点

**Flowchart vs StateDiagram：** 实验表明 Flowchart 效果比 StateDiagram 好约 15%。原因是 Agent 的执行过程不是严格状态机，而是并行搜索、多源汇总、失败回退的开放探索过程。

**\"无限画布\"的核心含义：** 不是上下文窗口无限大，而是 Agent 的工作空间扩展为可持续创建、折叠、重新展开的外部认知空间。重点不是\"无限读取\"，而是\"无限可达\"——信息可以离开上下文窗口，但不能离开 Agent 的可达范围。

**层次化注意力：** Agent 使用画布时分三层——(1) 鸟瞰：任务级概览，判断方向；(2) 聚焦：打开具体任务画布，查看步骤结构和进度；(3) 下钻：追溯 JSONL 摘要或 refs 原文。

## 四级记忆折叠架构

```
Level 0: Raw 原文 → refs/*.md（完整 tool result）
Level 1: JSONL Summary → offload-<sessionId>.jsonl（工具调用级摘要）
Level 2: MMD Node → mmds/<task>.mmd（任务步骤级摘要）
Level 3: Metadata → 任务级索引（taskGoal、status、mmdFilePath）
```

信息逐级变轻，但通过 `result_ref` → `node_id` → `mmdFilePath` 链路保持可恢复性。

**MMD 节点不是空图骨架：** 每个节点更像一张\"任务卡片\"，包含节点名称、状态、摘要和时间戳。一条 JSONL 记录包含 timestamp、node_id、tool_call、summary、result_ref 等字段，一个 MMD 节点可对应多条 JSONL 记录。

**找回过程分级：** metadata → 打开 mmds/ 文件 → 查看 MMD 节点 → 通过 node_id 在 JSONL 中查找 → 通过 result_ref 读取 refs 原文。Agent 很多时候只看 MMD 就够，只有当节点 summary 不够支持下一步判断时才继续下钻。

## 实验结果

### 总体结果

| 评测集 | Token 节省 | 成功率变化 |
|--------|-----------|-----------|
| SWEbench 500题 | 31-33% | 58.4% → 64.2%（+9.93%） |
| Toolathlon 20题 | 最高 26% | 20% → 35%（+75%） |
| WideSearch 200题 | 61.38% | 33% → 50%（+51.52%） |
| AA-LCR 800题 | 31% | 44.0% → 47.5%（+3.5pp） |

**注意：** 实验采用超长 Session 设计（多任务串行执行），比单题独立上下文评测更接近真实生产压力。

### 详细实验数据

**SWEbench 500题（主模型 4.5-haiku）：** 无插件完成率 58.4%，加入插件后提升到 61.8%–64.2%。Token 节省约 31%–33%，完成率相对提升 5.82%–9.93%。不同 Offload 模型（4.5-haiku / Opus / GLM 5.1）效果略有差异。

**Toolathlon（主模型 Opus 4.6）：** 20 个复杂长任务，通过数从 4 个提升到 6–7 个，通过率从 20% 到 30%–35%。MiniMax 2.7 作为 Offload 模型时 Token 节省最高 26.18%。

**WideSearch 200题（主模型 4.5-haiku）：** 网页搜索任务，Token 节省最显著。Opus Offload 方案 Token 节省 63.59%，通过率从 8.5% 提升到 12%（+41.18%）。

**AA-LCR 800题（主模型 4.5-haiku）：** 长文总结分析任务，准确率 44.0% → 47.5%（+14 题），主模型 Token 节省 36.67%，含 Offload 总 Token 节省 31%。

**消融实验：** 仅上下文卸载（无 MMD）时 Token 节省约 15%，成绩提升约 5%；完整方案 Token 节省 31-33%，成绩提升 5.82-9.93%。证明 Mermaid 画布额外贡献约一半收益。

## 长期个性化记忆

在 PersonaMem 评测集（6000+ 消息、589 题）上，回答准确率相比原生"龙虾"记忆**相对提升 59%**（48% → 76%）。部分场景 Token 节省接近 90%。

已上线 Qclaw、Lighthouse、ClawPro 等产品，并适配 [[openclaw]]、[[hermes-agent]] 等 Agent 框架。

## 在记忆生态中的定位

TencentDB Agent Memory 是 [[agent-memory-systems]] 在腾讯生态中的系统性实现。与 [[claude-mem]]（Claude Code 轻量插件）和 [[letta]]（MemGPT 虚拟内存）不同，它采用文件系统原生的四级折叠架构，特别强调用结构化图（Mermaid）保留任务拓扑。

核心洞见：**压缩不是让 Agent 少知道，而是让 Agent 少背负；信息可以离开上下文窗口，但不能离开 Agent 的可达范围。**

## 相关链接

- [[agent-memory-systems]] — Agent 记忆系统的总体框架
- [[context-rot]] — 记忆压缩要解决的核心问题
- [[context-compression-pipeline]] — Claude Code 的四级压缩管道
- [[claude-mem]] — Claude Code 生态的轻量记忆插件
- [[context-engineering]] — 上下文管理的方法论
- [[openclaw]] — TencentDB Agent Memory 支持的 Agent 框架
- [[hermes-agent]] — 同样支持的 Agent 框架
- [[manus]] — 文件系统即上下文的先驱实践
