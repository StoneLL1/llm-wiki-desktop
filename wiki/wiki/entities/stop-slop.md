---
title: stop-slop
created: 2026-04-23
updated: 2026-05-23
type: entity
tags: [tool, nlp, open-source, skill]
sources:
  - raw/articles/stop-slop-remove-ai-flavor-skill.md
  - raw/articles/2026-04-18-stop-slop-remove-ai-flavor-skill.md
  - raw/articles/aigc-rewriter-open-source-model.md
---

# stop-slop

## Overview

**stop-slop** is an open-source SKILL.md rule set created by **Hardik Pandya** for removing recognizable AI writing patterns ("slop") from generated text. Rather than being a traditional AI detector or subscription-based humanization tool, it is a pure text rules file that establishes strict "writing commandments" to eliminate formulaic AI-style expression.

The project addresses a growing content ecosystem problem: as generative AI proliferates, highly recognizable and mass-replicated writing patterns are creating content pollution with extremely low information density.

## What It Targets

Stop-slop is not aimed at AI writing itself, but at the **derivative, highly recognizable writing patterns** it produces. The Harvard Business Review defines this phenomenon as **workslop**: "low-effort, AI-generated work that looks plausibly polished, but ends up wasting time and effort as it offloads cognitive work onto the recipient."

### Typical AI Writing Patterns

- **Verbose hedging openers**: "This is important / You might not have realized / Let's dive deeper"
- **Shortcut structures**: Overuse of rhetorical questions, manufactured binary oppositions, formulaic three-part lists, forced "golden quote" summaries
- **Abstract vocabulary stacking**: "Profoundly impactful," "Far-reaching significance," "Worth noting" — lacking concrete factual support
- **Filler phrases**: "Here's the thing," "Let that sink in," "The uncomfortable truth is"

## How It Works

The SKILL.md file establishes four categories of rules:

1. **Banned clichés**: Common placeholder phrases added to a blacklist
2. **Rejected structural tropes**: Binary opposition writing, dramatic line breaks, pretentious narrative pacing
3. **Forced specificity**: Arguments lacking concrete factual support are rejected
4. **Clear agency**: Sentences must have explicit actors — no "the data tells us" anthropomorphization to mask logical gaps

As Hardik Pandya stated: *"AI writing has tells. Once you notice them, you see them everywhere."*

## Limitations

Stop-slop can immediately improve surface-level text style — articles become shorter, more direct, with less "self-explanation" and preachiness. However, it has a fundamental limitation:

> **"You can sanitize the prose. You can't sanitize the absence of a perspective that cost someone something."**

It fixes sentence structure but cannot fix the absence of genuine perspective, experience earned through trial and error, or authentic conviction.

## The Meta-Problem

Creator **Romy** articulated the core insight: *"Outsourcing writing = outsourcing thinking."*

**James Campbell** identified a further risk: if everyone adopts the same de-sloping rules, a new template emerges — uniform brevity, uniform short sentences, uniform "de-golden-quoted" style. The old template is stripped away, replaced by a new polished homogeneity that discerning readers can still detect.

## Impact and Adoption

Stop-slop has gained significant traction as an early example of the [[skills]] paradigm applied to content quality rather than technical capability. It demonstrates that SKILL.md files can encode subjective quality standards alongside technical instructions.

The project has also sparked broader discussion about the "AI content homogeneity" problem — not just surface-level writing patterns, but the deeper issue of perspective-less, experience-free content that looks polished but carries no genuine insight.

## Related Tools

- **AIGC Rewriter (格式工坊)**: An open-source model for Chinese AIGC text rewriting, using Qwen3-merged to reduce AI detection rates. Complementary to stop-slop for Chinese-language content.

## Key Quotes

- Guri Singh called stop-slop an **"open-source cheat code"**
- Romy: **"The essence of writing is thinking; outsourcing writing equals outsourcing thinking"**

## Relationships

- Created by Hardik Pandya
- Related to [[anti-slop-writing]] concept
- Companion tool: [[aigc-rewriter]] for Chinese text
- Built as a [[skills|SKILL.md]] file following Anthropic's skill paradigm
- Addresses the content quality crisis in the AI-generated content ecosystem

## See Also

- [[aigc-rewriter]] — Chinese-language AIGC rewriting tool
- [[skills]] — the SKILL.md modular capability framework
- [[anti-slop-writing]] — broader concept of anti-slop techniques
- [[anthropic]] — creator of the skills paradigm
