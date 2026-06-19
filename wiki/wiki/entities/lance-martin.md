---
title: Lance Martin
created: 2026-05-23
updated: 2026-05-23
type: entity
tags: [person, company]
sources:
  - raw/articles/2026-05-07-anthropic-harness-guide-dead-weight.md
---

# Lance Martin

## Overview

**Lance Martin**（@RLanceMartin）是 [[anthropic]] 的工程师/研究者，专注于 AI Agent 的 Harness 设计和最佳实践。他于 2026 年 4 月发表了博客「Harnessing Claude's Intelligence」，系统阐述了 [[harness-engineering]] 的三大原则，是 Anthropic 官方 Harness 方法论的重要推广者。

## 核心贡献

### Harness 三原则

Lance Martin 提出的 Harness 设计三原则：

1. **用 Claude 已经会的** — 底层工具越通用，Claude 发挥空间越大。bash + 文本编辑器就能衍生出 Skills、程序化工具调用、Memory 等复杂工作模式
2. **问自己还能停掉什么** — 在编排、上下文、记忆三个维度把控制权还给 Claude
3. **该设的边界还是要设** — 安全边界、用户界面、可观测性三种情况应提升为专用工具

### Dead Weight 概念推广

Lance 明确提出了 **Dead Weight** 的概念：

> Harness 编码的是「Claude 自己做不到什么」的假设，但这些假设会随着 Claude 变强而过时。

他用宝可梦实验生动说明：Sonnet 3.5 把 memory 当记录仪用，14000 步攒 31 个文件还在第二个城镇晃悠；Opus 4.6 同样步数只有 10 个分类文件，已拿到 3 个道馆徽章，还写了"从失败中提炼教训"文件。

### Build to Delete

> Build to delete. 造了就要敢拆。

Lance 将这个原则推到更底层：不只是编排架构要拆，预加载的指令、设计的工具、搭建的记忆系统，每一层都应定期审视。

## 性能数据

Lance 在博客中引用的关键数据：

| 场景 | 改动 | 效果 |
|------|------|------|
| BrowseComp | 让 Opus 4.6 自己过滤工具输出 | 45.3% → 61.6%（+16pp） |
| BrowseComp + subagent | Opus 4.6 分叉子 Agent | 再 +2.8% |
| BrowseComp + compaction | 同样压缩设置，模型升级 | Sonnet 4.5: 43% → Opus 4.5: 68% → Opus 4.6: 84% |
| BrowseComp-Plus + memory folder | Sonnet 4.5 + memory folder | 60.4% → 67.2% |

## Relationships

- [[anthropic]] 工程师，Harness 方法论推广者
- 其工作与 [[harness-engineering]] 页面覆盖同一方法论体系
- 与 Anthropic 工程博客的多 Agent 编排文章形成上下文
- 其"Build to Delete"理念适用于 [[long-running-agent]] 场景

## See Also

- [[harness-engineering]] — 他推广的核心方法论
- [[long-running-agent]] — Harness 在长程 Agent 中的应用
- [[anthropic]] — 他所在的公司
- [[claude-model-family]] — 他研究的模型系列
