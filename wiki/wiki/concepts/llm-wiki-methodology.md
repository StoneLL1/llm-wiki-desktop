---
title: LLM Wiki 方法论
created: 2026-05-19
updated: 2026-05-23
type: concept
tags: [knowledge-management, methodology]
sources:
  - raw/articles/2026-04-18-karpathy-knowledge-compilation.md
  - raw/articles/2026-04-18-ai-knowledge-base-tutorial.md
  - raw/articles/xhs-autowiki-paper-knowledge-base.md
---

# LLM Wiki 方法论

## Definition

LLM Wiki 方法论源于 [[andrej-karpathy]] 的「compile, don't search」理念，主张用 LLM 将原始信息一次性编译为结构化、可导航的知识图谱（wiki），而非每次查询时重新检索（[[rag]]）。

[[autowiki]] 是该方法论在学术论文管理场景的具体实现，通过单个 SKILL.md 文件定义质量标准和编译规则。

## Core Principles

1. **编译而非检索** — 信息处理一次，永久可用
2. **交叉引用** — 页面间通过双链（wikilinks）建立联系
3. **不可变原始层** — raw/ 目录存储原始素材，只读不改
4. **持续编译** — 新信息到来时增量更新知识图谱

## 目录结构

```
my-knowledge-base/
├── raw/              ← 原始素材收纳箱（你只管收集）
│   ├── articles/     ← 网页文章
│   ├── papers/       ← 论文 PDF
│   ├── repos/        ← GitHub 仓库
│   └── datasets/     ← 数据集
├── wiki/             ← LLM 编译产物（你几乎不需要手动编辑）
│   ├── index.md      ← 索引文件（自动维护）
│   ├── concepts/     ← 概念文章（互相 backlinked）
│   └── entities/     ← 实体文章
└── outputs/          ← 衍生输出（报告、答案、分析）
```

## 核心工作流

### Phase 1：数据摄入

- 什么都往 raw/ 扔，不整理不重命名
- 使用 [[agent-browser]] 自动化网页抓取（比 Playwright MCP 省 82% token）
- 也可使用 Obsidian Web Clipper 浏览器扩展

### Phase 2：LLM 编译

核心指令：读取 raw/ 中的所有内容，按照 CLAUDE.md 中的规则在 wiki/ 中编译。AI 自动：
- 创建 INDEX.md 索引
- 为每个主要主题创建 .md 文件
- 建立交叉链接（`topic-name` 格式）
- 总结每个源

### Phase 3：查询与增强

向知识库提问复杂问题：
- 「wiki 中我对某主题理解的最大空白是什么？」
- 「比较源 A 和源 B 对某概念的说法」
- 答案可保存回 wiki 或更新相关页面

### Phase 4：健康检查与维护

定期让 LLM 扫描 wiki：
- 标记文章间矛盾
- 找到提到但未解释的主题
- 列出没有 raw/ 来源支持的声明
- 建议填补空白的新文章

## Schema 配置

CLAUDE.md（或 AGENTS.md）是整个系统的配置文档，告诉 AI：
- 知识库的主题和范围
- 目录结构和文件命名约定
- 链接格式和索引维护规则
- 维基规则（每个主题一个 .md 文件，以摘要开头，使用双链）

## 关键洞察

- **错误会复利**：AI 写了稍微错误的内容被保存，后续答案跟着错。@HFloyd 的评论点出关键：「当输出被归档回去时，错误也会复利」
- **工具保持简单**：扁平文件加一个好的 schema，90% 场景比花哨的工具栈管用
- **人类只管读不管写**：wiki 内容完全由 LLM 维护，人的角色是阅读、提问、审查

## See Also

- [[knowledge-compilation]] — 知识编译的核心概念
- [[autowiki]] — 自动编译学术论文的实现
- [[karpathy-knowledge-compilation]] — Karpathy 知识编译的详细阐述
- [[agent-browser]] — 自动化网页抓取工具
- [[rag]] — 对比的检索增强生成技术
- [[claude-md]] — 项目级配置文件方案
