---
title: agentmemory
created: 2026-05-27
updated: 2026-05-27
type: entity
tags: [tool, open-source, agent, engineering]
sources:
  - raw/articles/2026-05-17-8-github-open-source-projects.md
---

# agentmemory

## 概述

**agentmemory** 是为 AI 编程助手构建的长期记忆服务器。它自动捕获每次工具调用、对话和代码修改，压缩成可搜索的结构化记忆，下次新会话开始时自动注入相关上下文。创建不到三个月，已获 9,000+ Star。

## 四层记忆架构

模仿人脑的工作方式，四层记忆各有分工：

| 记忆层 | 功能 | 类比 |
|--------|------|------|
| **工作记忆** | 存储原始观察 | 感官输入 |
| **情景记忆** | 存储会话摘要 | 个人经历 |
| **语义记忆** | 提取事实和模式 | 知识体系 |
| **程序记忆** | 记录工作流和决策习惯 | 技能和习惯 |

### 时间衰减机制
- 记忆随时间自然衰减
- 频繁访问的记忆会被强化
- 过时的记忆自动淘汰

## 检索系统

三流混合检索：
1. **BM25 关键词匹配** — 精确文本匹配
2. **向量语义搜索** — 语义相似度匹配
3. **知识图谱遍历** — 关系推理

在 LongMemEval 基准上 R@5 达到 95.2%。

## 部署特性

- 不需要外部数据库
- 默认绑定 127.0.0.1
- 全部自托管
- 一个服务器，所有 Agent 共享记忆

## 兼容性

支持 16+ 种编程助手：
- [[claude-code]]
- [[cursor]]
- Codex
- Windsurf
- 等等

## Relationships

- 解决 [[agent-memory-systems]] 的具体实现
- 与 [[claude-mem]] 互补（Claude Code 专用 vs 通用）
- 与 [[obsidian]] 方案互补（数据库 vs 文件系统）

## See Also

- [[agent-memory-systems]] — Agent 记忆系统概念
- [[claude-mem]] — Claude Code 跨会话记忆插件
- [[obsidian]] — 文件系统级 Agent 记忆方案
