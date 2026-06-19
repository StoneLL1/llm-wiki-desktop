---
title: Stitch
created: 2026-04-23
updated: 2026-05-22
type: entity
tags: [tool, multimodal]
sources:
  - raw/articles/2026-04-18-vibe-design-frontend-ui.md
---

# Stitch

## Overview

**Stitch** is an AI-native UI design platform created by **Google Labs**. It pioneered the **[[vibe-design]]** concept — using natural language to generate high-fidelity user interfaces, complementing the [[vibe-coding]] paradigm that focuses on generating functional code.

Stitch is powered by Google's **Gemini 3.1** series model and represents Google's entry into the AI-assisted design space, competing with tools like Anthropic's [[claude-design]], Vercel's v0, and Lovable.

## Core Concept: Vibe Design

Stitch introduced the term "Vibe Design" to describe its core philosophy: you don't need to manually draw wireframes, drag components, or tweak parameters. Instead, you describe the feeling and style of interface you want in natural language, and AI generates high-fidelity UI.

This directly addresses the aesthetic gap in [[vibe-coding]]: while AI can generate functional code, the resulting UI often looks like "2015 Bootstrap templates" with inconsistent spacing, mismatched colors, and an unmistakably AI-generated aesthetic.

## Key Feature: DESIGN.md Export

Stitch's most influential feature is its ability to **export a complete design system as a DESIGN.md file**. This means the design language refined in Stitch can be directly transferred to code generation tools like Claude Code, Cursor, or Gemini CLI.

This export capability is what made [[design-md|DESIGN.md]] a viral concept in the developer community — the [[design-md]] repository (37K+ stars) collected 58 website design systems extracted as DESIGN.md files, covering brands like Apple, Linear, Figma, Notion, BMW, and Tesla.

## Pricing and Availability

Stitch is currently free during the Google Labs phase:
- **350 Flash generations** per month
- **50 Pro generations** per month
- Export to Figma or direct frontend code output

## DESIGN.md Standard

Each DESIGN.md file contains 9 standard modules:

1. **Visual theme and atmosphere** — overall mood and style direction
2. **Color system and role definitions** — primary, secondary, accent, semantic colors
3. **Typography rules** — font families, sizes, weights, line heights
4. **Component styles** — buttons, cards, inputs, navigation specifications
5. **Layout principles** — spacing, grid, whitespace systems
6. **Hierarchy and shadows** — elevation system, depth cues
7. **Design "dos and don'ts"** — explicit guidelines for AI generation
8. **Responsive behavior** — breakpoint rules and adaptive patterns
9. **Agent prompt guide** — natural language instructions for AI design decisions

The Agent Prompt Guide is the critical component — it tells AI in natural language what to prioritize, what to avoid, and how to make design decisions across different scenarios.

## Competition

| Tool | Company | Approach |
|------|---------|----------|
| **Stitch** | Google Labs | AI-native design platform + DESIGN.md export |
| **Claude Design** | Anthropic | Conversational design tool with system prompt |
| **v0** | Vercel | AI UI generation from prompts |
| **Lovable** | Lovable | AI app builder |
| **Framer** | Framer | AI website builder |
| **Lovart** | Lovart | AI brand design |

## Impact

Stitch's DESIGN.md export feature has arguably had more impact than the platform itself. By creating a standardized, LLM-friendly format for expressing design intent, it solved a fundamental problem: how to communicate visual design requirements to AI coding tools in a way they can precisely understand and execute.

Markdown's advantage here is dual: it can carry both precise parameters (color values, spacing, font sizes) and fuzzy guidance (atmosphere, style, principles), and AI can process both types effectively.

## Relationships

- Created by **Google Labs**
- Uses **Gemini 3.1** model
- Pioneered the [[vibe-design]] concept
- Complements [[vibe-coding]] for functional code generation
- Its DESIGN.md export inspired [[design-md]] (37K+ stars)
- Related to [[claude-design]] as a competing approach
- Part of the [[design-md]] ecosystem

## See Also

- [[vibe-design]] — the design paradigm Stitch pioneered
- [[design-md]] — the markdown design system format
- [[vibe-coding]] — the complementary code generation paradigm
- [[claude-design]] — Anthropic's competing design tool
- [[design-md]] — community DESIGN.md collection
- [[lovart|Lovart 品牌设计]]


## Vibe Design 概念推广

Google 推 Stitch 时用了 Vibe Design 概念：
- 过去一年 Vibe Coding 解决了「不会写代码也能做产品」
- Vibe Design 补的是「做出来的产品好不好看」
- 不需要系统学设计、不需要会用 Figma
- 只需要有审美直觉，用 DESIGN.md 锚定

Stitch 定价（Google Labs 阶段）：免费，350 次 Flash + 50 次 Pro/月

### Sources
- raw/articles/2026-04-18-vibe-design-frontend-ui.md（鲁工）
