---
title: Vibe Design
created: 2026-04-23
updated: 2026-05-22
type: concept
tags: [tool, multimodal, tutorial]
sources:
  - raw/articles/2026-04-18-vibe-design-frontend-ui.md
  - raw/articles/claude-design-impact-on-ai-design-vendors.md
  - raw/articles/figma-vs-pencil-claude-code.md
  - raw/articles/2026-05-19-beautiful-practical-frontend-guide.md
---

# Vibe Design

## Definition

Vibe Design is a design paradigm where users describe visual and UI intent in natural language, and AI generates complete design systems, layouts, and interactive prototypes. It is the design counterpart to [[vibe-coding]], completing the AI-native creation pipeline from concept to implementation.

## The DESIGN.md Convention

At the core of Vibe Design is the **DESIGN.md** file — a structured Markdown document that encodes a complete visual design system. A well-written DESIGN.md includes:

- **Color palette** with semantic naming (primary, secondary, surface, etc.)
- **Typography scale** (font families, sizes, weights, line heights)
- **Spacing system** (consistent padding/margin tokens)
- **Component library** definitions (buttons, cards, forms, navigation)
- **Layout grid** specifications (columns, breakpoints, containers)
- **Design tokens** in structured JSON format for direct consumption by code
- **Reference sites** with links to real-world examples of desired aesthetics

The **awesome-design-md** repository (37K+ stars) collects 58 website DESIGN.md files, providing a rich library of design system starting points curated by the community.

## Key Implementations

### Claude Design (Anthropic)

Anthropic's conversational design tool built on [[claude-model-family]] models (notably Claude Opus 4.7 with `xhigh` effort level). Claude Design generates prototypes, presentations, and design systems from natural language descriptions. Its launch significantly impacted existing design tool vendors including [[figma]], Adobe, Canva, Wix, and Vercel.

Key capability: the Tweaks Protocol — a PostMessage-based system allowing AI-generated prototypes to be interactively adjusted by users without regeneration.

### Stitch (Google Labs)

Google Labs' AI-native UI design platform, powered by Gemini 3.1. Stitch embodies the Vibe Design concept by allowing natural-language-driven creation of complete, production-ready interfaces.

### Other Tools

| Tool | Creator | Focus |
|------|---------|-------|
| v0 | Vercel | Component and page generation from prompts |
| Lovable | Lovable | Full app builder from descriptions |
| Framer | Framer | AI-powered website creation |
| Pencil | — | Connects to Claude Code via MCP for design-to-code pipeline |
| Webflow | Webflow | No-code website builder with AI features |

## Figma vs. AI-Native Design

The tension between traditional design tools and AI-native approaches is a central debate. [[figma]] responded by releasing **Code to Canvas**, integrating with Claude Code to bridge design and implementation. However, the fundamental question remains: will AI-native design tools replace traditional design software, or will they coexist?

Arguments for replacement: faster iteration, no design skill required, direct-to-code pipeline.

Arguments for coexistence: complex design systems, brand consistency, collaborative workflows, pixel-level control.

## Design Tokens

Design tokens are structured values (colors, spacing, typography) expressed in JSON format. They serve as the bridge between DESIGN.md specifications and code implementation:

```json
{
  "colors": {
    "primary": "#2563EB",
    "secondary": "#7C3AED",
    "surface": "#FFFFFF",
    "background": "#F8FAFC"
  },
  "typography": {
    "heading": { "font": "Inter", "size": "2rem", "weight": 700 },
    "body": { "font": "Inter", "size": "1rem", "weight": 400 }
  }
}
```

## Practical Workflow

### 通用流程

1. Study reference sites and collect inspiration
2. Write a comprehensive DESIGN.md with all design tokens
3. Choose a Vibe Design tool (Claude Design, Stitch, v0)
4. Iterate on generated output with natural language refinements
5. Export design tokens to code (connect to [[vibe-coding]] pipeline)
6. Use Tweaks Protocol for interactive fine-tuning

### 前端设计速成工作流（Mav高未央，2026-05）

一套从 PRD 到美观实用前端的四步工作流，核心理念是「风格服务于目的、降低自由度让 AI 做美学决策、可复现才实用」：

**Step 1: 信息架构（IA Document）**
用 ia-planner（自定义 Agent workflow）阅读 PRD → 理解受众和功能 → 设计页面规划和信息流 → 交付 IA 文档（概述、sitemap、用户路径、核心模块清单）。注意：线框图在确认后删除，避免限制后续设计 Agent 发挥。

**Step 2: 视觉方案（Visual Schema）**
用 visual-explorer（自定义 Agent workflow）引导从 PRD + IA 向视觉设计方案过渡 → 确定色彩、动态和整体调性 → 交付物类似 [[design-md|DESIGN.md]]。

**Step 3: [[stitch]] 完成设计**
将 IA document 和 Visual Schema 分别依次交给 Stitch → 分别按信息架构和视觉方案完成设计 → Visual Schema 尝试多个方案 → 选中的设计 export → code to clipboard → 交给 [[claude-code]] / [[openai-codex]] 生成代码。

**Step 4: 动态部分设计**
用 ux-designer（自定义 Agent workflow）完成创意概念动态表达、音效、动画、生图提示词 + html-in-canvas 改造 → 交付动态交互方案和生图提示词文档。

> 工具链：Antigravity（或任意支持 Agent/Skill 的 coding agent）+ [[stitch]] + 即梦（生图模型）。每个 workflow 的子功能（线框图、色彩情绪版等）可单独做成 skill，交给 AI 自己写。^[raw/articles/2026-05-19-beautiful-practical-frontend-guide.md]

## Open Questions

- How do you maintain design consistency across AI-generated components?
- Can Vibe Design handle complex, enterprise-grade design systems?
- What is the role of human designers in an AI-native design workflow?
- How do design tokens and DESIGN.md evolve as projects grow?

## See Also

- [[vibe-coding]] — the coding counterpart to Vibe Design
- [[claude-design]] — Anthropic's conversational design tool
- [[figma]] — traditional design platform adapting to AI
- [[document-first-system]] — methodology that includes DESIGN.md in specifications
- [[claude-model-family]] — models powering Claude Design


## DESIGN.md 实践（鲁工）

DESIGN.md 是 Markdown 格式的设计系统规范文件，9 个标准模块：
1. 视觉主题与氛围
2. 色彩体系与角色定义
3. 排版规则
4. 组件样式（按钮、卡片、输入框、导航）
5. 布局原则（间距、网格、留白）
6. 层次与阴影
7. 设计的「该做」和「不该做」
8. 响应式行为
9. **Agent 提示指南**（关键：用自然语言告诉 AI 生成 UI 时应关注什么）

awesome-design-md（VoltAgent）提取 58 个知名网站设计系统为 DESIGN.md，4 万+ Star。

### Sources
- raw/articles/2026-04-18-vibe-design-frontend-ui.md（鲁工）
