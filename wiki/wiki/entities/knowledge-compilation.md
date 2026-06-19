---
title: Knowledge Compilation
created: 2026-04-23
updated: 2026-05-23
type: entity
tags: [data, rag, knowledge-management, methodology]
sources:
  - raw/articles/2026-04-18-karpathy-knowledge-compilation.md
  - raw/articles/2026-04-18-ai-knowledge-base-tutorial.md
  - raw/articles/xhs-autowiki-paper-knowledge-base.md
---

# Knowledge Compilation

## Overview

Knowledge Compilation is [[andrej-karpathy]]'s methodology for using LLMs to transform raw information (papers, notes, articles) into structured, linked knowledge bases. Unlike [[rag]] (Retrieval-Augmented Generation), which rediscovers relevant information per query, Knowledge Compilation processes information once into a persistent, navigable format. The key insight is: **"compile, don't search."**

This concept appears in multiple articles in the corpus and is central to all of them, representing a paradigm shift in how researchers and learners manage information with AI assistance.

## Core Concept

### Compile vs. Search

The fundamental distinction between Knowledge Compilation and traditional approaches:

| Dimension | RAG (Search) | Knowledge Compilation (Compile) |
|-----------|-------------|-------------------------------|
| Processing | Per-query retrieval | One-time compilation |
| Freshness | Always queries latest sources | Compiled once, kept current |
| Structure | Flat retrieval results | Linked, navigable knowledge graph |
| Cost | Recurrent inference cost | Upfront compilation cost |
| Quality | Depends on retrieval accuracy | Curated and refined |
| Navigation | Linear search results | Wiki-style hypertext links |

Karpathy argues that for domains where information changes slowly (academic research, technical documentation), compilation is far more efficient than repeated retrieval.

### The Compilation Process

Knowledge Compilation follows a structured workflow:

1. **Ingest**: Feed raw materials (papers, articles, notes) into the system
2. **Extract**: LLM identifies key entities, concepts, relationships, and claims
3. **Structure**: Organize extracted information into a linked knowledge base
4. **Link**: Create wikilinks between related concepts and entities
5. **Curate**: Human review and refinement of compiled knowledge
6. **Maintain**: Incremental updates as new information becomes available

## Karpathy's Vision

### Key Insights

- **Wikis as compiled knowledge**: A well-structured wiki is a "compiled" version of raw information
- **LLMs as compilers**: LLMs can perform the compilation work that previously required human effort
- **Persistent value**: Compiled knowledge bases provide ongoing value unlike transient chat conversations
- **Linked structure**: Wikilinks create navigable knowledge graphs that enhance understanding
- **Incremental updates**: Knowledge bases can be maintained and updated incrementally
- **人类不做维护**: 维护知识库最烦的不是读资料和思考，是「记账」——更新交叉引用、修订旧观点、保持一致性。LLM 不会厌倦，一次处理十几个文件，维护成本接近零

### Connection to LLM Coding

Karpathy connects Knowledge Compilation to his broader insights about LLM-assisted coding:

- CLAUDE.md files as compiled project knowledge
- SKILL.md files as compiled methodology knowledge
- The "explore → plan → implement → commit" workflow as knowledge compilation in action

### 历史渊源

Karpathy 提到这个想法在精神上和 Vannevar Bush 1945 年的 Memex 相关——个人的、经过策划的知识存储系统，文档之间的连接和文档本身同样有价值。Bush 没有解决的是谁来做维护。LLM 解决了这个问题。

## AutoWiki Implementation

### Overview
AutoWiki is a concrete implementation of Knowledge Compilation for academic papers (xhs-autowiki-paper-knowledge-base). Created by 雾灯, AutoWiki uses SKILL.md-based compilation to transform research papers into structured wiki pages.

### AutoWiki Features
- **SKILL.md-based compilation**: Uses the skill file convention to define compilation rules
- **Paper-to-wiki pipeline**: Automated extraction of key information from academic papers
- **Milestone-centric organization**: Organizes papers by conceptual breakthroughs rather than folders
- **Temporal graph**: Auto-constructed timeline showing paper relationships (extends/complements/contrasts)
- **CRGP analysis**: Novelty extraction via prior/update comparison structure

## Relationship to RAG

Knowledge Compilation and [[rag]] serve different but complementary purposes:

- **RAG is for dynamic information**: When you need the latest data or frequently changing information
- **Knowledge Compilation is for stable knowledge**: When information changes slowly and benefits from curation
- **Hybrid approach**: A compiled knowledge base can serve as the primary source, with RAG filling gaps for recent information

## Practical Applications

- **个人研究者** — 长期跟踪某领域，资料越积越多，编译成持续生长的知识图谱
- **小团队** — 会议记录、客户通话、项目文档让 LLM 维护团队 wiki
- **Academic Research** — Compiling paper collections into navigable knowledge bases
- **Technical Documentation** — Converting raw documentation into structured wikis

## Best Practices

1. **Start with raw sources** — Papers, articles, notes — the more raw material, the better
2. **Extract, don't summarize** — Preserve details rather than reducing to high-level summaries
3. **Link everything** — Wikilinks are the key value proposition of compiled knowledge
4. **Use proper structure** — YAML frontmatter, consistent formatting, clear sections
5. **Maintain incrementally** — Update pages as new information becomes available
6. **Error-aware** — 错误会复利，定期健康检查

## Key Relationships

- Methodology by [[andrej-karpathy]]
- Implemented by [[autowiki]] for paper knowledge bases
- Related to [[rag]] — compilation vs. retrieval paradigm
- Connected to [[llm-wiki-methodology]] for wiki creation practices
- Uses SKILL.md convention from Anthropic's skill ecosystem
- Relevant to [[context-engineering]] for managing information flow

Tools like [[obsidian]] provide graph visualization for compiled wikis. [[agent-browser]] enables automated data ingestion.
