---
title: Anti-Slop Writing
created: 2026-04-23
updated: 2026-05-23
type: concept
tags: [nlp, prompt-engineering, skill, tutorial]
sources:
  - raw/articles/stop-slop-remove-ai-flavor-skill.md
  - raw/articles/2026-04-18-stop-slop-remove-ai-flavor-skill.md
  - raw/articles/aigc-rewriter-open-source-model.md
  - raw/articles/claude-design-system-prompt-bilingual.md
---

# Anti-Slop Writing

## Definition

Anti-Slop Writing encompasses techniques, tools, and practices for removing AI-style writing patterns from generated text. "AI slop" (also called "workslop") refers to low-effort AI-generated content characterized by recognizable patterns, verbose hedging, and low information density. As AI-generated content becomes ubiquitous, the ability to produce writing that reads as authentically human has become a critical skill.

## Characteristics of AI Slop

AI-generated writing tends to exhibit these identifiable patterns:

### Verbose Hedging
Instead of stating conclusions directly, AI text pads statements with qualifications:
- "It's important to note that..."
- "It's worth mentioning that..."
- "In today's rapidly evolving landscape..."
- "While there are many factors to consider..."

### Unnecessary Summaries
AI frequently opens and closes with redundant summaries:
- "In conclusion, [restating everything already said]"
- "To summarize the key points..."
- "In this article, we explored..."

### False Balance
AI tends to present all perspectives as equally valid, even when evidence strongly favors one position. This creates a bland, non-committal tone.

### Enthusiasm Inflation
- "Incredibly powerful"
- "Revolutionary approach"
- "Game-changing technology"
- "Groundbreaking research"

### List-Heavy Structure
AI defaults to lists, bullet points, and numbered items even when prose would be more appropriate. Articles become sequences of "Here are X ways to Y" without narrative flow.

### Qualifying Language
- " arguably"
- "perhaps"
- "it seems"
- "one might say"
- "in some sense"

## Countermeasures

### stop-slop (SKILL.md)

The **stop-slop** SKILL.md by Hardik Pandya is an open-source rule set that can be loaded into AI coding agents (particularly [[claude-code]]) to enforce anti-slop writing patterns. Guri Singh described it as an "open-source cheat code" for better AI writing.

Key rules include:
- Ban specific hedging phrases and filler words
- Require direct statement of conclusions
- Enforce information density over word count
- Prohibit unnecessary summaries and recapitulations
- Mandate specific, concrete language over vague abstractions

### AIGC Rewriter (格式工坊)

An open-source model built on Qwen3-merged (from [[claude-model-family|Qwen3]] by Alibaba) that rewrites AI-generated Chinese text to reduce its "AI rate" (the detectability of AI authorship). This tool is specifically designed for the Chinese content ecosystem where AI detection is increasingly common.

### Negative Prompting

Explicitly instructing AI models about what *not* to do:
- "Do not use hedging language"
- "Do not summarize at the end"
- "Do not use lists when prose is more appropriate"
- "Do not use enthusiasm inflation words"

### Strong Constraint Words

Using high-priority constraint words to enforce style compliance:
- **MUST** — non-negotiable requirements
- **NEVER** — absolute prohibitions
- **CRITICAL** — essential rules
- **ALWAYS** — mandatory behaviors

These words carry more weight in model attention than softer instructions like "please" or "try to."

## The Meta-Problem

James Campbell identified a critical concern: **anti-slop patterns can themselves become new templates**. If everyone uses the same anti-slop rules, the resulting writing develops its own recognizable uniformity — just a different flavor of sameness.

This creates a paradox: the more we standardize "good" writing rules, the more we risk replacing AI slop with a new, equally identifiable "anti-slop style."

## Philosophical Considerations

### "Outsourcing Writing = Outsourcing Thinking"

Commentator Romy raised a deeper concern: when we use AI to write for us, we are not just outsourcing the mechanical act of writing — we are outsourcing the cognitive process of thinking through ideas in written form. Writing is not merely a communication channel; it is a thinking tool.

### Authenticity vs. Efficiency

Anti-slop writing exists in tension between two goals:
- **Authenticity**: writing that sounds genuinely human and reflects individual voice
- **Efficiency**: using AI to produce content faster than manual writing

Perfect anti-slop may be indistinguishable from human writing, but it still lacks the genuine perspective and lived experience that gives human writing its depth.

## Practical Application

### For AI Agent Configuration

When configuring [[claude-code]] or similar tools for writing tasks:
1. Load stop-slop SKILL.md rules
2. Add project-specific style guidelines
3. Use strong constraint words in prompts
4. Include negative examples of AI slop in context
5. Review output specifically for slop patterns before accepting

### For Content Creators

1. Use AIGC Rewriter as a post-processing step for Chinese content
2. Train yourself to recognize slop patterns in your own AI-generated drafts
3. Rewrite AI output in your own voice rather than accepting it verbatim
4. Use AI for research and structure, but write conclusions and insights yourself

## Open Questions

- Can we define objective measures of "slop" vs. "quality" writing?
- Will anti-slop tools create an arms race with AI detection systems?
- How do anti-slop practices differ across languages and cultures?
- Is "human-sounding" writing always better than clearly AI-generated writing?
- As AI models improve, will slop patterns naturally diminish?

## See Also

- [[stop-slop]] — the open-source SKILL.md rule set
- [[aigc-rewriter]] — open-source model for reducing AI detection rate
- [[garden-skills]] — Skills 合集，web-design-engineer 专门解决 AI 生成网页的「视觉 AI 味」问题
- [[claude-model-family]] — models that can produce both slop and quality writing depending on configuration
- [[claude-md]] — project configuration that can include anti-slop rules
- [[prompt-engineering]] — broader discipline that includes anti-slop techniques
