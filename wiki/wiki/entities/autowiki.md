---
title: AutoWiki
created: 2026-04-23
updated: 2026-05-24
type: entity
tags: [tool, data, open-source]
sources: [raw/articles/2026-04-23-xhs-autowiki-paper-knowledge-base.md, raw/articles/2026-04-21-github-top10-weekly-stars.md]
---

# AutoWiki

## Overview

AutoWiki is an LLM-driven paper knowledge base compilation tool developed by **AlphaLab-USTC** (雾灯). It implements [[andrej-karpathy]]'s "compile, don't search" knowledge compilation paradigm specifically for academic paper management. The entire system is encoded as a single 390-line SKILL.md file that defines quality standards, anti-patterns, and workflow rules for transforming raw PDFs into a structured, interconnected wiki.

GitHub: [AlphaLab-USTC/AutoWiki-skill](https://github.com/AlphaLab-USTC/AutoWiki-skill)

## Core Concepts

### Milestone-Centric Organization

Rather than organizing papers by folder or topic category, AutoWiki clusters papers by **conceptual breakthroughs** — the way a good survey traces how ideas evolved rather than listing references. This produces knowledge structures that mirror actual scientific progress.

### Temporal Graph

AutoWiki automatically constructs a timeline for each paper in a domain, annotating relationships as:

- **extends** — builds directly on prior work
- **complements** — adds orthogonal capability or perspective
- **contrasts_with** — challenges or contradicts prior findings

This creates an evolution chain that makes a field's development trajectory immediately visible.

### Deep Analysis with CRGP

AutoWiki does not produce simple summaries. It uses a **CRGP (prior/update comparison)** structure to extract true novelty deltas: "What did the field already have → What is genuinely new in this paper." Each paper automatically generates:

- Essence distillation
- CRGP factor analysis
- Temporal relationship graph
- Critical analysis (prior → update comparison)

### Three-Layer Classification

Topic pages in the generated wiki read like mini-surveys: milestone definitions, evolution trajectories, open problems, and cross-domain connections — all auto-generated.

## Performance

- **80 papers** (Agent Self-Evolution domain) ingested in **2 hours**
- Produced **13 milestone nodes** + **3-layer classification system**
- Each paper receives full analysis including temporal positioning

## Technical Architecture

The AutoWiki system operates as a pipeline:

1. **Ingestion**: Raw papers (PDFs, arXiv links) are deposited into the workspace
2. **Extraction**: LLM processes each paper, extracting key contributions, methods, and findings
3. **Milestone Mapping**: Papers are clustered into milestone nodes based on conceptual contribution
4. **Graph Construction**: Temporal relationships (extends/complements/contrasts_with) are auto-generated
5. **Deep Analysis**: CRGP comparison extracts novelty deltas against prior work
6. **Wiki Assembly**: Final structured wiki with cross-linked pages is compiled

The entire pipeline is governed by the SKILL.md file, which acts as both quality standard and execution blueprint. No external orchestration framework is needed — the LLM follows the skill's rules directly.

## Design Philosophy

The system embodies the principle that **Skill = Architecture**: the entire compilation pipeline is governed by a single SKILL.md file. This follows the [[skills]] paradigm where modular capability units encode complete workflows.

AutoWiki directly implements [[karpathy-knowledge-compilation]]'s vision: users deposit papers, LLMs compile the wiki, and knowledge compounds automatically.

## Relationships

- Created by **雾灯** (Wu Deng) at AlphaLab-USTC
- Implements the [[knowledge-compilation]] paradigm from [[andrej-karpathy]]
- Built as a [[skills|SKILL.md]]-based system compatible with [[claude-code]]
- Complementary to [[mineru]] for PDF-to-markdown preprocessing
- Part of the broader [[llm-wiki-methodology]] ecosystem

## See Also

- [[knowledge-compilation]] — the paradigm AutoWiki implements
- [[skills]] — the modular capability framework
- [[claude-md]] — project configuration for Claude Code
- [[mineru]] — PDF conversion tool often used as preprocessing step
Works with [[obsidian]] for local knowledge graph visualization.
