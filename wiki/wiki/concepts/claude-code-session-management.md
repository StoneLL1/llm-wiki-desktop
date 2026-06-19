---
title: Claude Code Session Management
created: 2026-05-22
updated: 2026-05-23
type: concept
tags: [methodology, tutorial, prompt-engineering]
sources:
  - raw/articles/2026-04-18-claude-code-session-management.md
  - raw/articles/2026-04-18-claude-code-creator-15-hidden-features.md
  - raw/articles/2026-04-18-claude-code-hidden-commands.md
  - raw/articles/2026-04-21-claude-code-1m-context-management-guide.md
  - raw/articles/2026-04-18-xhs-claude-no-compact-two-methods.md
---

# Claude Code Session Management

## Definition

Session Management 是 [[claude-code]] 使用中的高技能上限领域——在 rewind、主动 compact、子 Agent、新建 session 之间做选择需要策略性思考。核心思想是：**在信息量和注意力之间找平衡**。

这一方法论由 Claude Code 核心开发者 **Thariq Shihipar** 在博客「Using Claude Code: session management and 1M context」中系统阐述。

## 五条岔路框架

每当 Claude 完成一步操作后，你站在一个分叉路口，有五条路可以走：

### 1. 继续对话
在当前 session 里直接发下一条消息。
- **保留 context**: 全部
- **适用**: 同一任务，context 还健康

### 2. Rewind（双击 Esc）
跳回之前某条消息，从那个节点重新开始，后面的对话全部丢弃。
- **保留 context**: 前缀，砍掉尾巴
- **适用**: Claude 走错了路，保留有用的文件读取，丢掉失败尝试

### 3. /clear（手动简报）
自己写一段简报，然后开一个全新 session。
- **保留 context**: 只有你写的简报
- **适用**: 全新的任务

### 4. Compact（/compact）
让模型总结当前对话，压缩后继续在这个 session 里干活。
- **保留 context**: 有损摘要
- **适用**: 干到一半，context 被调试信息塞满

### 5. 子 Agent
把一块工作派给一个子 Agent，它有自己独立的 context，干完活只把结论带回来。
- **保留 context**: 完整指令和最终结果（中间噪音留在子 context）
- **适用**: 下一步会产生大量中间输出，但你只需要结论

## Context Rot（上下文腐烂）

**Context Rot** 是 [[context-engineering]] 中的核心问题——context 越长，模型注意力越分散，老的不相关内容干扰当前任务。详细分析参见 [[context-rot]]。

### 200K → 1M 的悖论

1M Context 上线后反而更容易触发 context rot，因为能塞进去的东西变多了，模型判断「什么重要什么不重要」的压力也跟着变大。

### 自动压缩的恶性循环

自动 compaction 发生在 context 快满的时候，而那个时刻恰恰是模型注意力最分散、最不聪明的时候。结果压缩质量差，重要信息被丢弃。

### 对策：主动出手

不要等自动 compaction。在 context 还很健康时（约 50%）手动 `/compact`，并告诉它你接下来打算做什么：
```
/compact 重点保留 auth 重构的部分，调试的那段可以丢掉
```

## Rewind 高级用法

大多数人只把 rewind 当「撤销」用，但它的价值远不止于此：

### 时光信模式

在 rewind 之前，先让 Claude **总结它学到了什么**，写一段「交接信息」。然后拿着这段信息，rewind 回去，贴给「新的」Claude：

> 就像是未来的 Claude 给过去的自己写了封信：「我试过这个路了，走不通，原因是……」

### 对比效果

- **纠正模式**：读文件 → 尝试 A 失败 → 「试 B」→ 尝试 B 失败 → 「试 C」→ 成功。Context 堆满失败
- **Rewind 模式**：读文件 → 尝试 A 失败 → Esc Esc → 直接说「用 C，别用 A/B」→ 一步到位。Context 干净

## Compact vs Clear

| 维度 | /compact | /clear |
|------|----------|--------|
| 执行者 | Claude 自己总结 | 你自己写简报 |
| 精确度 | 有损，Claude 可能遗漏 | 精确，每个 token 你选的 |
| 省力程度 | 省事 | 费力 |
| 适用场景 | 任务进行中，细节可以模糊 | 下一步至关重要，需精确控制 context |
| 比喻 | 让助理整理桌面 | 自己收拾桌子 |

## 会话管理五件套

来自社区的日常操作最佳实践：

1. **`/rename`** — 给会话起有意义的名字，从「匿名聊天记录」变成「项目日志」
2. **`/branch`** — 像开 git 分支一样开对话分支，探索不同方案
3. **`claude --resume`** — 恢复历史会话，配合 `/rename` 快速找到目标
4. **`Ctrl+R`** — 搜索历史会话，类似 shell 反向搜索
5. **`Ctrl+G`** — 打开 vi 编辑器写 prompt，适合长指令和语音输入改错

## 决策速查表

| 场景 | 操作 | 理由 |
|------|------|------|
| 同一任务，context 还健康 | 继续对话 | 内容都还有用 |
| Claude 走错了路 | Rewind (Esc Esc) | 保留有用的，丢掉失败尝试 |
| context 被调试信息塞满 | `/compact` + 提示词 | Claude 筛选，你引导方向 |
| 全新的任务 | `/clear` | 零腐烂，你来决定带什么走 |
| 下一步会产生大量中间输出 | 子 Agent | 噪音留在子 context，只拿结论 |

## 1M Context 下的新建议

Anthropic 官方博客专门讨论了 1M context window 下的会话管理（2026-04），核心要点：

- **新斜杠命令 `/usage`**：查看 Claude Code 使用情况，辅助会话管理决策
- **上下文组成**：系统提示符 + CLAUDE.md + 当前对话 + 工具调用及其输出 + 已读取文件
- **Subagent 使用心法**：只需要结论就开 subagent，还要用中间输出就别开
- **Opus 4.7 变化**：默认对 subagent 的开启比 4.6 更克制，需要在 prompt 里明确写出

### Subagent 的判断标准

> 你还会再用到这个工具的中间输出吗，还是只需要最终结论？

- **只需要结论** → subagent（用自己的 context 跑完，只带回结论）
- **还要用中间输出** → 留在主会话

### 可直接抄作业的 Prompt

- "Spin up a subagent to verify the result of this work based on the following spec file"
- "Spin off a subagent to read through this other codebase and summarize how it implemented the auth flow, then implement it yourself in the same way"
- "Spin off a subagent to write the docs on this feature based on my git changes"

## 不需要 Compact 的两种替代方法

来自小红书用户 Erichain 的实践（52 赞、85 收藏），核心观点：**无论是 Auto Compact 还是手动 Compact，都会消耗大量 Token**（运行一次上下文压缩，当前 session 和 Weekly session 增长 20-30%）。

### 方法一：Handoff 交接文档

手动让 [[claude-code]] 总结当前对话，生成一份 **handoff 文档**（交接文档），然后新开一个对话窗口，读取交接文档来继续任务。

- **优势**：零压缩 Token 开销，交接信息精确可控
- **适用**：任务中途需要换会话但保持连续性

### 方法二：Plan Mode 续接

在上下文用到约 75% 时，让 Claude 进入 **Plan Mode**（规划模式），制定新计划来完成剩余工作。

Claude 生成新计划后，会提示是否清除当前上下文。选择清除并继续，Claude 会自动将计划内容粘贴进新会话。

- **优势**：利用 [[claude-code]] 内置的 Plan Mode 机制，自动化程度更高
- **适用**：剩余工作量明确，可以一次性规划

### 与五条岔路框架的关系

这两种方法本质上是"五条岔路"中 **3. /clear（手动简报）** 的变体：
- Handoff 文档 = 自己写简报的升级版（让 Claude 帮你写）
- Plan Mode 续接 = /clear + 自动注入新计划

共同目标：**避免 Compact 的 Token 开销，同时保持任务连续性**。

## 关键洞察

> context window 从 200K 扩到 1M，真正的意义并非「装更多东西进去」，它给你的其实是「做精细管理的空间」。就像搬进了一个更大的房子，目的并非堆更多杂物，每个房间各司其职才是关键。

## Relationships

- 是 [[context-engineering]] 在 [[claude-code]] 中的具体实践
- 基于 [[claude-model-family]] 的 1M token context window
- 与 [[multi-agent-collaboration]] 中的子 Agent 模式紧密关联
- 是 [[harness-engineering]] 方法论在会话层面的体现

## See Also

- [[context-engineering]] — 更广义的上下文管理学科
- [[claude-code]] — 这些方法的应用平台
- [[claude-code-slash-commands]] — 完整命令参考
- [[multi-agent-collaboration]] — 子 Agent 和多 Agent 协作
