---
title: Andrej Karpathy
created: 2026-04-23
updated: 2026-06-04
type: entity
tags: [person]
sources: [karpathy-knowledge-compilation, github-top10-weekly-stars, xhs-autowiki-paper-knowledge-base, 2026-06-03-karpathy-learning-methodology]
---

# Andrej Karpathy

## Overview

**Andrej Karpathy** is a prominent AI researcher and educator, formerly Director of AI at Tesla. He is widely known for his LLM coding insights, educational content, and his influential knowledge compilation vision that has shaped how developers think about building structured knowledge bases with AI.

His CLAUDE.md file (curated as [andrej-karpathy-skills](https://github.com/forrestchang/andrej-karpathy-skills) on GitHub) has gained 37K+ stars and is widely used by [[claude-code]] practitioners to improve coding agent performance.

## Knowledge Compilation Theory

Karpathy's most impactful recent contribution is his **"compile, don't search"** paradigm, articulated in his LLM Wiki gist. The core argument:

### The Problem with RAG

Most people use LLMs for document processing via RAG — upload files, retrieve relevant fragments at query time, assemble an answer. The fundamental flaw: every query starts from zero. Previous reasoning results vanish into chat history. Nothing accumulates.

### The Alternative: Compile at Write Time

Instead of retrieving at query time, **compile at write time**. When new material is added, the LLM reads it, digests it, integrates information into a continuously maintained wiki, updates related pages, flags contradictions with existing content, establishes cross-references, and adds synthesized conclusions.

Karpathy's analogy: **"Obsidian is the IDE, LLM is the programmer, wiki is the codebase."**

### Three-Layer Architecture

1. **Raw materials** — articles, papers, images, data files (read-only, the LLM cannot modify them)
2. **Wiki本体** — fully LLM-written and maintained (humans only read)
3. **Schema** — a configuration document (CLAUDE.md in Claude Code, AGENTS.md in Codex) defining wiki structure, conventions, and how to handle new materials

### Daily Operations

- **Ingest**: Add new materials, LLM processes and updates relevant pages
- **Query**: Ask the wiki, LLM synthesizes across pages with citations (good answers can be archived back)
- **Maintain**: Periodic health checks — find contradictions, orphan pages, data gaps

### Historical Context

Karpathy notes the idea is spiritually related to **Vannevar Bush's 1945 Memex** concept — a personal, curated knowledge storage system where document connections are as valuable as the documents themselves. Bush's unsolved problem: who does the maintenance? LLMs solve this.

### Applicable Scenarios

- Personal growth tracking
- Deep research topic dives
- Reading note accumulation
- Enterprise internal knowledge bases
- Research group literature management

## The LLM Wiki Pattern

Karpathy's wiki structure has become a de facto template for AI knowledge bases:

```
knowledge-base/
├── raw/          ← Raw material staging area
│   ├── articles/   Web clippings (.md + local images)
│   ├── papers/     arXiv PDFs
│   ├── repos/      GitHub repos
│   └── datasets/   Data sets
└── wiki/         ← LLM compilation output
    ├── index.md
    ├── concepts/*.md
    ├── outputs/
    └── ... cross-linked network
```

This pattern directly inspired [[autowiki]], which implements it as a SKILL.md for paper knowledge base construction.

## Learning Methodology

Karpathy's philosophy on learning, shared via X (5.6K likes, shared by Rohan Paul), centers on a core thesis that challenges the AI-assisted learning paradigm:

> **"You cannot replace deep engagement with a topic."**

Key tenets:

- **Learning requires friction.** Growth comes from effort — the best things come through struggle. It's paradoxical until you realize that not putting in the effort gets you nowhere with the same amount of effort spent feeling sorry for yourself.
- **AI can't substitute deep thinking.** You won't look to ChatGPT as a role model for "the life of the mind," nor thrill to Gemini's grand theories or idiosyncratic insights. Deep engagement with material — wrestling with ideas, building things that push your limits — cannot be replaced by prompting an LLM.
- **If you're not actively struggling to build something, you're just watching Netflix with a smarter aesthetic.**
- **Learning isn't fun at first** — like working out, it may feel like puking initially, but as days go by you get addicted to the growth.

This methodology stands in tension with AI-assisted knowledge work: Karpathy simultaneously advocates for using LLMs to compile knowledge ([[knowledge-compilation]]) while warning against using AI as a substitute for the deep thinking that produces genuine understanding. The LLM wiki is for *organizing* knowledge; the friction of learning is for *internalizing* it.

## Key Insight

Karpathy's essential contribution is framing knowledge management as a **paradigm problem, not a technical problem**: "Retrieval is inferior to compilation; chat history is inferior to persistent knowledge bases; starting from zero every time is inferior to continuous accumulation."

## Relationships

- His CLAUDE.md is widely used by [[claude-code]] practitioners
- His knowledge compilation theory is implemented by [[autowiki]]
- Related to the broader [[knowledge-compilation]] paradigm
- The [[claude-md]] convention is the schema layer in his architecture
- His approach complements [[claude-design]] thinking about AI-assisted creation

## See Also

- [[knowledge-compilation]] — the paradigm Karpathy articulated
- [[claude-md]] — project configuration (his "schema" layer)
- [[autowiki]] — tool implementing his vision for papers
- [[claude-code]] — Anthropic's CLI agent where his CLAUDE.md is used
