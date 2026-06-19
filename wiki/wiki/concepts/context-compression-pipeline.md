---
title: Context Compression Pipeline（上下文压缩管道）
created: 2026-05-23
updated: 2026-06-10
type: concept
tags: [engineering, agent, methodology]
sources:
  - raw/articles/2026-04-18-harness-engineering-source-code.md
  - raw/articles/2026-04-18-xhs-claude-no-compact-two-methods.md
  - raw/articles/2026-05-22-tencent-agent-memory-token-saving-mermaid.md
  - raw/articles/2026-06-09-agent-context-compression-strategies-comparison.md
---

# Context Compression Pipeline（上下文压缩管道）

## Overview

上下文压缩管道是 [[claude-code]] 源码中最优雅的设计之一，通过**四级渐进降级**解决 LLM 上下文窗口逐渐耗尽的问题。这是 [[harness-engineering]] 六大支柱中「上下文架构」的核心实现。

设计哲学：**能轻量解决的就不动用重量级方案**。从零 API 调用的裁剪到需要 LLM 摘要的全量压缩，每级都有明确的触发条件和回退路径。

## 四级管道

```
原始消息 → Snip Compact → Micro Compact → Context Collapse → Auto Compact → API
              (零API调用)    (缓存编辑)       (读时投影)         (LLM摘要)
              最轻量                                             最重，最后手段
```

### Level 1 — Snip Compact

基于标记的历史裁剪。在消息流中找到 snip 边界标记，移除标记之前的消息。**最轻量**，无需 API 调用。

### Level 2 — Micro Compact

缓存编辑压缩。利用 API 的 cache editing 能力，在不破坏整体缓存的情况下删除特定工具调用的结果。

### Level 3 — Context Collapse

上下文折叠。将多轮工具调用结果折叠为摘要，但保留结构。这是一个**读时投影**——折叠视图在每次发送前重新计算，原始消息仍然保存在 REPL 的完整历史中。

### Level 4 — Auto Compact

全量摘要压缩。当上下文接近窗口限制时，使用 LLM 生成对话摘要替换历史消息。这是**最重的操作**，也是最后的防线。

阈值计算：`effectiveWindowSize = contextWindow - min(maxOutput, 20000)`，其中 20000 基于 p99.99 数据（17,387 tokens）。

## 与外部方案的对比

- **LangChain Deep Agents**：工具结果超过 20,000 tokens 时卸载到文件系统
- **Claude Code**：通过 Context Collapse 在读时投影为摘要视图
- **TencentDB Agent Memory**：四级折叠架构（refs → JSONL → MMD → metadata），用 Mermaid Flowchart 保留任务拓扑，超长 Session 中节省 31-61% Token。详见 [[tencentdb-agent-memory]]
- 两者殊途同归：**永远不要让上下文窗口变成垃圾场**

## 在 query() 循环中的位置

[[claude-code]] 源码中 `query()` 的 16 步循环里，步骤 3-6 就是这四级压缩：

```
while(true):
    # 1-2: 前置预取
    # 3: Snip Compact
    # 4: Micro Compact
    # 5: Context Collapse
    # 6: Auto Compact
    # 7-16: 后续验证和执行...
```

## 设计洞察

1. **渐进降级优于一刀切**：四级管道让系统在上下文压力下优雅退化，而非突然崩溃
2. **读时投影是空间换时间**：Context Collapse 保留原始消息（空间），每次发送前重新计算折叠视图（时间）
3. **数据驱动阈值**：Auto Compact 的预留 token 数来自 p99.99 实测数据，而非拍脑袋

## Compact 的 Token 成本

社区实践发现（小红书用户 Erichain），无论是 Auto Compact 还是手动 `/compact`，**运行一次上下文压缩会消耗大量 Token**——当前 session 和 Weekly session 增长约 20-30%。这是因为 LLM 需要读取完整对话并生成摘要，本身就构成一次完整的推理调用。

这催生了 [[claude-code-session-management]] 中"不 Compact 的替代方法"：使用 Handoff 交接文档或 Plan Mode 续接来避免压缩的 Token 开销。

## 六家 Agent 横向对比

腾讯 MUR AI 团队的 mervynyang 对六大主流 Agent 的上下文压缩策略做了系统性横向拆解^[raw/articles/2026-06-09-agent-context-compression-strategies-comparison.md]，揭示了"六家产品六种哲学"——上下文压缩没有显而易见的最优解。

### 各家策略详解

**[[claude-code]]（Anthropic）：五段流水线 + 服务端卸载**

前四步纯本地操作零 API 调用，第五步才调 LLM：
1. **Budget Reduction** — 调整工具输出截断预算
2. **Snip** — 截短老的工具输出，留"做过什么"摘要行
3. **Microcompact** — 对工具输出做局部内联压缩
4. **Context Collapse** — 对更久远历史做粒度更细的折叠
5. **Auto-Compact** — 兜底，调 LLM 生成结构化摘要（固定九个章节：用户意图/主要请求/技术概念/文件与代码段/错误与修复/问题解决过程/用户消息/待办任务/下一步）

两条更激进的**服务端路径**（客户端字节不变，缓存不失效）：
- **cached_microcompact**：把"删掉旧 tool 结果"包装成 API 层的 cache_edits 指令，服务端在已缓存前缀上直接抠掉指定内容
- **apiMicrocompact**：直接调 Anthropic context_management API（beta context-management-2025-06-27），让服务端按 input_tokens 阈值自动裁剪

Claude Code 在压缩时刻意保持消息序列前缀稳定，让 Prompt Cache 命中率不会因压缩而下降。

**[[openai-codex]]（OpenAI）：近期用户消息优先保护**

约 95% 容量时触发，生成 handoff 摘要替换旧历史。重建后上下文只剩三部分：摘要 + 近期 ~20k token 内用户消息原样保留 + 更早用户消息蒸馏进摘要。设计哲学："同事间的工作交接"——进展、约束、剩余任务，足够下一个模型接手。

**OpenCode：可逆隐藏 + 回放最后指令**

两步走：Prune（轻量无 LLM）每次成功响应后自动触发，用时间戳标记老的工具输出为"已压缩"占位符（数据仍在数据库里可恢复）；Summary（重量调 LLM）只在 token 超限时触发，生成五段式结构化摘要后自动回放用户最后一条消息——模型从最近指令而非摘要继续。

**Cline：自动 + 手动双模式**

- `/smol`（别名 `/compact`）：手动触发，生成摘要后在同一任务内接续
- Auto-Compact：接近上限时自动触发
- Focus Chain（v3.25 默认开启）让待办列表穿越压缩存活

**[[cursor]]：压缩 + 可回溯**

自动压缩旧消息 + 提示用户开新对话。2026 年新增 **Dynamic Context Discovery**（聊天历史变成可搜索文件，压缩后仍可检索原始细节），A/B 测试减少 46.9% 总 token 消耗。已知问题：压缩后模型有时"忘掉"刚才的编辑（高优 bug 修复中）。

**Amp（Sourcegraph）：不压缩，换线程**

立场鲜明：递归摘要导致性能逐步衰减（引用 OpenAI 内部研究），用 `/handoff` 打包要点进新线程。线程是一等公民（`@@` 引用其他线程、`threads: map` 可视化关系）。理念："一系列有焦点的短步骤，比一个逐渐退化的长对话好"。2026 年 Neo CLI 更新加了 90% 窗口时的自动上下文管理。

**[[letta]] / MemGPT：上下文当 RAM**

学术派代表，按操作系统内存层次建模：Core Memory（始终在上下文）↔ Archival Memory（外部向量存储）↔ Recall Memory（对话历史搜索）。关键区别：换入换出由 Agent 自己决定（通过函数调用），不是被动截断。Letta v0.16.7（2026-03），22.5k Star。代价是架构复杂度高、需要外部向量存储、有检索延迟，适合跨会话长期记忆但对单会话内压缩偏重。详见 [[letta]]。

### 实施陷阱：滑窗式 stub 替换 = 每步缓存失效

一个隐蔽但代价高昂的陷阱^[raw/articles/2026-06-09-agent-context-compression-strategies-comparison.md]：如果采用"保留最近 N 条 tool 结果，更老替换成 stub"的滑窗策略，且在每步都重算，则每完成一个 step 就有 1 条旧结果滑出窗口被替换，导致该位置往后整段 prompt 前缀对 Prompt Cache 失效。

**真实数据**：4 轮 177 step 59 分钟会话，总花费 $77.3，其中 **83%（$64.8）全是 cache_write**。从 step 7 起前缀缓存持续失效，cacheWrite 一路涨到约 12 万/step（cacheWrite 单价是 cacheRead 的 12.5 倍）。

**结论：stub 决策必须单调推进**——只能大跳不能滑窗。一个 part 一旦标成 stub，后续所有 turn/step 都保持不变。两种实现方式：按 part ID 持久化决策（Redis/内存映射）；或交给服务端 cache_edits / context_management API，客户端字节零变化。

### 共识原则

横向对比后提炼出接近共识的原则：

| 原则 | 说明 |
|------|------|
| **分层渐进** | 多个水位线，越接近上限越激进，避免悬崖式塌方 |
| **成本严格递增** | 便宜先做（截断/placeholder），贵的最后做（LLM 摘要） |
| **增量摘要** | 维护一份活摘要，每次只合并新增部分，避免"摘要的摘要"语义漂移 |
| **用真实 token** | `usage.totalTokens` 免费精确，`text.length / 3` 仅用于内部排序 |
| **用户消息特权** | 用户指令/代码是任务来源，至少保证不裁用户纯文本 |
| **保护近端** | 最近几轮不动（常见 8000 token 保护区），维持短期连贯性 |
| **单调边界** | stub 决策一旦做出必须固定，绝不因"又老了一步"反复触发 |

## Relationships

- [[claude-code]] 的核心基础设施
- [[harness-engineering]] 六大支柱中「上下文架构」的具体实现
- [[context-engineering]] 的工程化落地
- [[context-rot]] 的自动化应对方案

## See Also

- [[claude-code]] — 压缩管道运行的平台
- [[harness-engineering]] — 理论框架
- [[context-engineering]] — 上下文管理的方法论
- [[context-rot]] — 压缩管道要解决的问题
- [[tencentdb-agent-memory]] — 腾讯的四层折叠记忆方案
- [[openai-codex]] — handoff summary 压缩策略
- [[letta]] — 上下文当 RAM 的学术派方案
- [[agent-memory-systems]] — 跨会话记忆的系统性方案
