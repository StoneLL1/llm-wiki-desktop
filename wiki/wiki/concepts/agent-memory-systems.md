---
title: Agent 记忆系统
created: 2026-05-17
updated: 2026-05-25
type: concept
tags:
  - agent
  - methodology
  - architecture
sources:
  - raw/articles/2026-05-08-obsidian-coding-agent-long-term-memory.md
  - raw/articles/2026-05-11-harness-engineering-knowledge.md
  - raw/articles/2026-05-17-8-github-open-source-projects.md
  - raw/articles/2026-05-22-tencent-agent-memory-token-saving-mermaid.md
  - raw/articles/2026-05-24-jason-liu-getting-most-out-of-codex.md
  - raw/articles/2026-05-24-openai-codex-tips-jason-liu.md
---

# Agent 记忆系统

## 定义

Agent 记忆系统是为 AI Agent 构建跨越会话的持久记忆能力的系统性方案。它解决的核心问题是：如何让 Agent 在不同会话之间保持对项目、用户偏好和历史决策的记忆。

## 核心问题

### 跨会话失忆

每次新会话开始时，Agent 丢失了之前所有的交互上下文，导致：
- 重复询问相同信息
- 无法延续之前的决策
- 项目理解从零开始

### 上下文压缩失真

在长会话中，为了适配上下文窗口限制，需要对历史信息进行压缩，导致：
- 关键细节丢失
- 语义偏移
- 决策依据不完整

## 认知科学模型

Agent 记忆系统的设计借鉴了认知科学中的记忆模型：

| 认知概念 | Agent 对应 |
|----------|-----------|
| 工作记忆 | 上下文窗口（Context Window） |
| 长期记忆 | 外部存储（文件、数据库、向量存储） |

理解这一映射关系，有助于设计更合理的记忆架构。

## Obsidian 方案

基于 Obsidian 的 Agent 记忆系统采用三层架构：

### 1. 入口层

Agent 启动时首先读取的入口文件，包含当前项目状态和关键信息索引。

### 2. 长期记忆层

结构化的 Markdown 文件存储，按主题和类型组织：
- 项目背景
- 技术决策
- 用户偏好
- 常见问题

### 3. 会话日志层

每次会话的结构化日志，记录交互过程和产出，供后续会话参考。

### Memory Protocol

定义了 Agent 如何读取、写入和更新记忆的规范协议，确保记忆系统的一致性。

## agentmemory 四层架构

基于认知科学的四层记忆模型：

1. **工作记忆**：当前上下文窗口中的信息
2. **情景记忆**：特定事件和交互的记录
3. **语义记忆**：抽象的知识和概念
4. **程序记忆**：操作步骤和技能

每一层有不同的存储策略、检索方式和生命周期。

## 腾讯方案

腾讯提出的五层知识存储架构：

### Layer 0 — P（个人偏好）

用户的个人编码风格、命名偏好等。

### Layer 1 — 项目约定

项目级别的编码规范、架构决策等。

### Layer 2 — 领域知识

与项目相关的业务领域知识。

### Layer 3 — 项目知识

具体的项目实现细节和历史记录。

## TencentDB Agent Memory 短期记忆压缩

腾讯 TencentDB Agent Memory 团队提出了**上下文卸载 + Mermaid 无限画布**的组合方案，在超长 Session 中节省 61% Token，任务通过率提升 52%。核心思想是「信息可以离开上下文窗口，但不能离开 Agent 的可达范围」。

### 四级折叠架构

1. **refs/\*.md** — 完整 tool result 原文
2. **JSONL 摘要** — 工具调用级摘要（offload-.jsonl）
3. **MMD 节点** — 任务步骤级摘要（mmds/\<task\>.mmd），用 Mermaid Flowchart 组织
4. **Metadata** — 任务级索引，只保留 taskGoal、status、mmdFilePath

### 层次化注意力

Agent 使用画布时分层查看：鸟瞰（任务概览）→ 聚焦（任务画布结构）→ 下钻（JSONL/refs 细节）。避免两种极端：全塞进上下文（Token 浪费 + 注意力稀释）vs 过度摘要（原文不可恢复）。

详见 [[tencentdb-agent-memory]]。

## Codex Shared Memory 方案

[[jason-liu]] 在 [[openai-codex]] 官方指南中提出了一种基于 [[obsidian]] 知识库的共享记忆方案：

### 核心设计

- **持久工作记忆**：用 Obsidian 知识库（~/vault）存储跨线程持久上下文
- **AGENTS.md 规则**：通过 AGENTS.md 定义 Agent 如何使用知识库——优先更新已有笔记、保留决策和阻塞项、无意义变化不产生干扰
- **分工明确**：代码归代码仓库，滚动的工作上下文归知识库
- **内置记忆**：Settings > Personalization > Memories 提供偏好级记忆层，补充显式知识库

### 与其他方案的比较

| 方案 | 存储位置 | 控制模式 | 适用场景 |
|------|---------|---------|---------|
| Codex Shared Memory | Obsidian 知识库 | 显式（AGENTS.md 规则） | 跨线程项目上下文 |
| Claude Code Memory | CLAUDE.md + 自动记忆 | 混合（手动 + 自动） | 跨会话编码偏好 |
| 腾讯四层折叠 | Mermaid 画布 + JSONL | 结构化（四级架构） | 超长 Session 压缩 |

Codex 的方案与 Claude Code 的记忆系统有异曲同工之处，但 Codex 选择了更「显式」的路：用户自己决定哪些上下文需要保留，而不是让模型自动记忆。

## 知识类型分类

将知识按类型进行分类管理：

- **model**：数据模型定义
- **decision**：技术决策及其理由
- **guideline**：编码规范和最佳实践
- **pitfall**：已知的坑和避免方法
- **process**：操作流程和步骤

## 三级成熟度 + 自动衰减

知识条目按照可靠程度分为三个等级：

- **draft**：新创建、未验证的知识
- **verified**：经过实践验证的知识
- **proven**：多次验证、高度可信的知识

### 自动衰减机制

知识条目随时间推移自动降低成熟度等级，促使团队定期重新验证，避免过时知识误导 Agent。

## 知识引用追踪闭环

建立知识的引用追踪机制：

1. Agent 使用某条知识时记录引用
2. 定期分析知识的使用频率和效果
3. 淘汰无效知识，强化高频有效知识
4. 形成知识质量的持续改进闭环

## 相关链接

- [[claude-code]] — 记忆系统在 Claude Code 中的实现
- [[claude-md]] — CLAUDE.md 作为记忆系统的入口层
- [[harness-engineering]] — 记忆系统是 Harness Engineering 的重要组成
- [[knowledge-compilation]] — 知识编译与记忆系统的关系
- [[tencentdb-agent-memory]] — 腾讯的四层折叠记忆系统
- [[claude-mem]] — Claude Code 轻量记忆插件
- [[context-rot]] — 记忆系统要解决的核心问题
- [[openai-codex]] — Codex 的 Shared Memory 方案
- [[jason-liu]] — Codex 官方指南作者，Shared Memory 方案提出者
