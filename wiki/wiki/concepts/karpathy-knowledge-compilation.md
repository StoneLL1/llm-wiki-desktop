---
title: Karpathy 知识编译
created: 2026-05-19
updated: 2026-05-23
type: concept
tags: [knowledge-management, methodology]
sources:
  - raw/articles/2026-04-18-karpathy-knowledge-compilation.md
  - raw/articles/2026-04-18-ai-knowledge-base-tutorial.md
  - raw/articles/xhs-autowiki-paper-knowledge-base.md
---

# Karpathy 知识编译

## Definition

Karpathy 知识编译是 [[andrej-karpathy]] 提出的信息管理范式：用 LLM 将原始文档一次性编译为结构化的 interlinked markdown 知识库，而非传统 [[rag]] 的每次检索。

核心洞察是 **「compile, don't search」** — 信息处理一次，永久可导航。这种方法产生的知识库具有天然的交叉引用和矛盾检测能力。

## RAG 的根本问题

Karpathy 指出当前大多数人用 LLM 处理文档的方式本质上都是 RAG：把文件上传，问问题时检索相关片段，拼出答案。NotebookLM、ChatGPT 文件上传、大多数知识库产品都遵循这个逻辑。

**根本缺陷**：每次都从零开始。你问一个需要综合五份文档的问题，LLM 就得重新找、重新拼、重新推导。上次的推导结果消失在聊天记录里。**结果就是什么都没有积累下来。**

## 三层结构

Karpathy 将编译过程分为三层：

1. **原始资料层**（raw/）— 文章、论文、图片、数据文件，只读不改，这是事实的源头
2. **Wiki 本体层**（wiki/）— 完全由 LLM 负责写作和维护，你只负责读
3. **Schema 层**（CLAUDE.md / AGENTS.md）— 告诉 LLM wiki 的结构、约定、遇到新资料该怎么处理

> Karpathy 原话：「Obsidian 是 IDE，LLM 是程序员，wiki 是代码库。」

## 日常操作三件事

1. **摄入** — 加新资料进来，LLM 处理并更新相关页面
2. **查询** — 向 wiki 提问，LLM 综合相关页面给出带引用的答案，好的答案归档回 wiki
3. **整理** — 定期让 LLM 做健康检查，找矛盾、找孤儿页面、找数据空白

## 作者的个人实践变体

文章作者在 Karpathy 的框架上做了四个调整：

1. **不用 Obsidian** — 认为 AI 时代应 Vibe Coding 打造专属笔记工具，用 [[cursor]] / [[claude-code]] 一上午搭一个
2. **只对话不收藏** — 看到有价值内容直接与 Claude 对话处理，有用结论落知识库，原始链接不留
3. **以项目为单位** — 每个项目独立 KB 知识库，而非一个大库（通用知识库维护成本随规模指数级上升）
4. **索引方式一致** — CLAUDE.md 告诉 AI 常用知识库路径，通过索引快速理解全貌

## 实践指南（Nick Spisak 保姆级教程）

Datawhale 整理的保姆级实现教程，不需要特殊软件和数据库，只要文件夹和文本文件：

### 四步工作流

1. **Phase 1：数据摄入** — 什么都往 raw/ 扔，不整理不重命名；使用 [[agent-browser]] 自动化网页抓取
2. **Phase 2：LLM 编译** — 一条指令让 AI 读取 raw/ 并编译成 wiki（创建 INDEX.md、主题页面、交叉链接）
3. **Phase 3：查询与增强** — 向知识库提问复杂问题，答案可归档回 wiki
4. **Phase 4：健康检查** — 定期审查矛盾、缺失、过时信息

### 关键提醒

- 错误会复利：AI 写了稍微错误的内容被保存，后续答案跟着错。定期运行健康检查
- 工具选择保持简单：Karpathy 原话「试图保持超级简单和扁平，只是一个嵌套的 .md 文件目录」
- 装了 47 个插件的 Obsidian 又是一个 Notion 陷阱

## 历史渊源

Karpathy 提到这个想法在精神上和 Vannevar Bush 1945 年提出的 Memex 相关——个人的、经过策划的知识存储系统，文档之间的连接和文档本身同样有价值。Bush 没有解决的是谁来做维护。LLM 解决了这个问题。

## See Also

- [[knowledge-compilation]] — 知识编译的详细阐述
- [[llm-wiki-methodology]] — 方法论的具体实践
- [[rag]] — 对比的检索增强生成技术
- [[agent-browser]] — 自动化网页抓取工具
- [[obsidian]] — 可选的知识库前端
- [[andrej-karpathy]] — 方法论提出者
