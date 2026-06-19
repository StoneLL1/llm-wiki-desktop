---
title: RAG (Retrieval-Augmented Generation)
created: 2026-05-19
updated: 2026-05-19
type: concept
tags: [rag, architecture]
sources: [karpathy-knowledge-compilation, xhs-autowiki-paper-knowledge-base]
---

# RAG (Retrieval-Augmented Generation)

## Definition

RAG（检索增强生成）是一种将外部知识检索与 LLM 生成结合的技术架构。每次查询时动态检索相关文档片段注入上下文，让模型基于最新、最相关的信息生成回答。

## RAG vs Knowledge Compilation

| 维度 | RAG（检索） | Knowledge Compilation（编译） |
|------|------------|--------------------------|
| 处理方式 | 每次查询动态检索 | 一次性编译 |
| 信息新鲜度 | 始终查询最新源 | 编译一次，持续更新 |
| 结构 | 扁平检索结果 | 互联的知识图谱 |
| 矛盾检测 | 无 | 编译时自动发现 |

## See Also

- [[knowledge-compilation]] — 编译范式的对比
- [[context-engineering]] — 上下文管理的技术
