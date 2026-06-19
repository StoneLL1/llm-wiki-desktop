---
title: Claude Code 上下文续接方法
created: 2026-05-27
updated: 2026-05-27
type: concept
tags: [engineering, agent, methodology, skill]
sources:
  - raw/articles/2026-04-18-xhs-claude-no-compact-two-methods.md
---

# Claude Code 上下文续接方法

## Definition

在 [[claude-code]] 长对话场景中，避免使用 Compact（上下文压缩）的两种替代方法。来源于小红书用户 Erichain 的实践分享（52 赞、85 收藏）。

## 为什么避免 Compact

无论是 Auto Compact 还是手动 `/compact`，运行一次上下文压缩会消耗大量 Token——当前 session 和 Weekly session 增长约 **20-30%**。因为 LLM 需要读取完整对话并生成摘要，本身就构成一次完整的推理调用。

详见 [[context-compression-pipeline]]。

## 两种替代方法

### 方法一：Handoff 交接文档

1. 手动告诉 Claude Code：**总结当前对话，生成一份 handoff 交接文档**
2. 新开一个对话窗口
3. 让新对话**读取交接文档**来继续任务

优点：
- 原始上下文完整保留在交接文档中
- 新对话有干净的上下文窗口
- 交接文档可以作为项目知识沉淀

### 方法二：Plan Mode 续接

1. 在上下文用到 **75% 左右**时，让 Claude 进入 Plan Mode（规划模式）
2. Claude 生成新计划来完成剩余工作
3. Claude 会提示选择是否清除当前上下文
4. 选择"是"后，Claude 自动将计划内容粘贴进新对话

优点：
- 无需手动写交接文档
- Plan Mode 生成的计划天然结构化
- 与 [[claude-code]] 原生功能无缝衔接

## 与相关概念的关系

| 方法 | 适用场景 | Token 开销 | 上下文保留度 |
|------|---------|-----------|------------|
| Auto Compact | 自动触发，无需干预 | 高（20-30%） | 摘要级 |
| Handoff 文档 | 需要完整保留关键信息 | 低（仅文档写入） | 手动选择 |
| Plan Mode 续接 | 任务有明确剩余计划 | 低（仅计划生成） | 计划级 |

## Relationships

- 是 [[claude-code-session-management]] 中"Compact 替代方案"的具体实践
- 解决 [[context-rot]] 问题的一种人工干预手段
- 与 [[context-compression-pipeline]] 的四级压缩形成互补
- 体现 [[context-engineering]] 中"主动管理上下文"的理念

## See Also

- [[claude-code-session-management]] — 会话管理完整方法论
- [[context-compression-pipeline]] — Claude Code 四级压缩管道
- [[context-rot]] — 上下文腐烂问题
- [[context-engineering]] — 上下文工程
