---
title: Fiona Fung
created: 2026-06-04
updated: 2026-06-04
type: entity
sources:
  - raw/articles/2026-06-03-claude-code-ai-native-engineering-org.md
tags: [person, company]
---

# Fiona Fung

## Overview

Fiona Fung is the Director of Engineering for [[claude-code|Claude Code]] and Claude Cowork at [[anthropic]]. She spoke at Code w/ Claude SF 2026 about how agentic coding is reshaping engineering organizations — moving bottlenecks from writing code to verification, review, and security.

## Key Contributions

### AI-Native Engineering Organization Practices

Fung outlined four process transformations her team adopted after making agentic coding the default working mode:

#### 1. JIT (Just-in-Time) Roadmaps

Traditional six-month roadmaps became obsolete within three months because Claude Code accelerated change velocity. The team shifted to JIT planning: rapid prototypes → get internal users on them → act on feedback. Design discussions moved from separate docs into PRs and prototypes.

#### 2. Context Collection: Ask Claude First

Instead of finding the person who wrote the code to ask questions, engineers ask Claude directly — "Who caused this regression?" or "What's the reasoning behind this decision?" The team perpetually asks: "Is there a way to automate this?"

#### 3. Code Review: Trust But Verify

Claude handles all style, lint, PR feedback, bug catching, pre-commit fixes, and test additions. Humans focus exclusively on three areas requiring professional judgment: **legal review, trust-boundary/security code, and product taste**. Fung emphasizes that the trust-verify balance must be continuously re-evaluated as models improve.

#### 4. Team Composition: Role Blurring

Fixed roles dissolve — PMs do substantial coding, engineers take on content and design. Fung prioritizes two talent types:
- **Creative builders with product sense** — "dreamers, deeply curious, passionate about delivering products that solve problems"
- **Engineers with deep system expertise** — essential for building products like Claude Code on the Web

> "I'm less focused on raw throughput; models handle that. The more important question is where human expertise is still needed — that's where I focus."

### Dogfooding Principle

Every team member (including cross-functional partners) uses Claude Code and Claude Cowork. Managers remain hands-on ICs writing real code. The team keeps structure as flat as possible.

## Core Insight

The fundamental shift: **engineering bottlenecks moved from writing code to verification, review, and security.** The new paradigm's key question is identifying where human professional judgment is still required — not comparing coding speed.

Her practices connect directly to [[claude-code-self-check|Claude Code self-check feedback loops]] and the broader [[ai-native-development]] paradigm.

## See Also

- [[claude-code]] — The product she leads engineering for
- [[anthropic]] — Her employer and creator of Claude
- [[lance-martin]] — Anthropic engineer, Harness principles advocate
- [[boris-cherny]] — Creator of Claude Code
- [[ai-native-development]] — The development paradigm her practices exemplify
- [[claude-code-self-check]] — Self-check feedback loops complementing the "trust but verify" approach
