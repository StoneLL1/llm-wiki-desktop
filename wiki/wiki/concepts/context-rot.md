---
title: Context Rot（上下文腐烂）
created: 2026-05-22
updated: 2026-06-10
type: concept
tags:
  - methodology
  - engineering
  - inference
sources:
  - raw/articles/2026-04-21-claude-code-1m-context-management-guide.md
  - raw/articles/2026-04-20-10-claude-code-best-practices.md
  - raw/articles/2026-05-22-tencent-agent-memory-token-saving-mermaid.md
  - raw/articles/2026-06-09-agent-context-compression-strategies-comparison.md
---

# Context Rot（上下文腐烂）

## 定义

**Context Rot** 是指随着 LLM 上下文窗口变长，模型表现缓慢下滑的现象。注意力被更多 token 分散，早期已不相关的内容开始干扰当前任务的判断。

这个概念来自 Anthropic 官方博客「Using Claude Code: session management and 1M context」，是 [[context-engineering]] 中的核心问题之一。

## 200K → 1M 的悖论

在 200K context 时代，context rot 还不算特别明显——一个半天以内的会话大多能撑过去。

1M Context 上线后反而**更容易触发**，因为：
- 你能塞进去的东西变多了
- Claude 判断「什么重要什么不重要」的压力跟着变大
- 更长的上下文 = 更多的干扰信号

## Agent Dumb Zone

社区发现的经验阈值：上下文使用超过 **60%~70%** 时，模型表现会明显下降：
- 容易忽略指令
- 代码出低级错误
- 推理质量下降

腾讯 TencentDB Agent Memory 团队的实验引用了 Anthropic 的数据：当 context 长度超过 max window 的 **80%** 时会发生明显的上下文腐烂。详见 [[tencentdb-agent-memory]]。

最佳实践是在 **50%** 时就手动执行 `/compact`，别等系统自动 compact——那时候往往已经晚了。

## 自动压缩的恶性循环

自动 compaction 恰好是在模型注意力已经涣散的时候触发的。结果：

1. 模型注意力涣散 → 判断力下降
2. 此时触发自动压缩 → 模型用下降的判断力决定什么重要
3. 重要信息可能被错误丢弃 → 压缩质量差
4. 压缩后的 context 进一步恶化 → 下一轮表现更差

这是 Anthropic 官方明确指出的核心矛盾。

## 应对策略

### 主动 Compact

不等自动 compaction，在 context 还很健康时（~50%）手动 `/compact`，并给 hint 引导保留方向：
```
/compact 重点保留 auth 重构的部分，调试的那段可以丢掉
```

### Rewind 替代纠正

Claude 走偏了不要在原 context 里纠正（错误推理还在上下文里），而是 `Esc Esc` / `/rewind` 回滚到健康节点重新开始。

### 新任务开新会话

新任务用 `/clear` 开始干净会话，不携带旧任务的冗余 context。

### 使用子 Agent

大量中间输出但只需最终结论的任务，委托给 subagent —— 它有独立 context，噪音不污染主会话。

### 压缩的本质：保护注意力而非省 token

腾讯 MUR AI 团队提出一个关键洞察^[raw/articles/2026-06-09-agent-context-compression-strategies-comparison.md]：**Context 压缩的目标从来不是省 token，省钱只是顺带的。它要解决的问题是保护模型的注意力。** 200K 上下文窗口看似充裕，但研究表明上下文塞到 70% 以上时，模型中段失忆和指令漂移就会明显恶化——不是真的"忘了"，而是注意力被稀释、信号被噪声淹没。合格的压缩系统是一个"信号工程师"：把无关工具输出降为占位符、裁短老 assistant 文本、把历史合并成结构化摘要，让模型用事实思考而非用文本回忆。参见 [[context-compression-pipeline]] 的六家横向对比。

## 与相关概念的关系

- 是 [[context-engineering]] 的核心挑战
- 在 [[claude-code-session-management]] 中有系统性解决方案
- 影响 [[claude-code]] 的使用策略
- 与 [[claude-model-family]] 的 context window 大小直接相关

## See Also

- [[context-engineering]] — 更广义的上下文管理学科
- [[claude-code-session-management]] — Claude Code 中的具体实践
- [[claude-code]] — 受 context rot 影响的主要平台
- [[skills]] — 通过 skill 精简 context 的方法
- [[tencentdb-agent-memory]] — 腾讯的四层折叠记忆方案，应对 context rot
