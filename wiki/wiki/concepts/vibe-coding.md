---
title: Vibe Coding
created: 2026-04-23
updated: 2026-05-23
type: concept
tags: [code, agent, tutorial]
sources:
  - raw/articles/2026-04-18-stop-vibe-coding-shit-mountain.md
  - raw/articles/2026-04-18-vibe-design-frontend-ui.md
  - raw/articles/2026-04-18-pi-agent-vibe-coding.md
  - raw/articles/2026-04-18-claude-code-1m-context-management-guide.md
  - raw/articles/2026-04-18-zero-human-coding-ai-native-dev-handbook.md
---

# Vibe Coding

## Definition

Vibe Coding is a programming paradigm where developers describe their intent in natural language and AI agents generate the corresponding code. Rather than writing syntax line-by-line, the human operates at the level of intent, architecture, and feedback — guiding AI to produce implementations through conversational iteration.

The term gained prominence through Andrej Karpathy's observation that with modern LLMs, coding often feels like "just vibing" — describing what you want and letting the AI figure out how to build it.

## The "Shit Mountain" Problem

Left unchecked, vibe coding produces what practitioners call a **"shit mountain"** — a growing pile of unmaintainable code generated without proper structure, documentation, or methodology. Key failure modes include:

- **No specifications**: jumping straight to code without defining what to build
- **No locked dependencies**: constantly shifting tech stack as the AI suggests new tools
- **No architectural consistency**: each generated component follows different patterns
- **No review discipline**: accepting AI output without critical evaluation
- **Context overflow**: losing track of project state as sessions grow

The solution is the [[document-first-system]] approach: writing comprehensive specification documents (PRD.md, APP_FLOW.md, TECH_STACK.md, etc.) *before* any code generation begins.

## The Four Words That Matter

As articulated by the agent-skills community, effective vibe coding distills to four principles:

1. **Context** — the AI needs rich, structured context about the project, not vague instructions. [[context-engineering]] is the discipline of managing this.
2. **Feedback** — rapid, specific feedback loops between human intent and AI output. The human must actively review and correct.
3. **Iteration** — successful vibe coding is iterative, not one-shot. Refine prompts, regenerate, adjust.
4. **Quality** — maintain quality standards through documentation, testing, and architectural guardrails.

## Tools for Vibe Coding

| Tool | Type | Key Feature |
|------|------|-------------|
| [[claude-code]] | CLI Agent | Subagents, Plan Mode, CLAUDE.md configuration, Skills |
| [[cursor]] | AI IDE | Inline code generation, multi-file editing |
| Pi Agent | Agent | Purpose-built for vibe coding workflows |
| GSD2 | Automation | Automated development pipeline |

## Relationship to Other Paradigms

Vibe Coding is the **coding** counterpart to [[vibe-design]] (natural-language-driven UI design). Together they form a complete AI-native development workflow covered in the [[ai-native-development]] paradigm.

The [[document-first-system]] methodology was specifically developed as an antidote to undisciplined vibe coding. When combined with [[context-engineering]] practices — particularly managing long sessions with Claude Code's 1M context window — vibe coding becomes a powerful, structured approach to software development rather than a chaotic one.

## Practical Workflow

1. Write specification documents (see [[document-first-system]])
2. Set up project configuration (CLAUDE.md, TECH_STACK.md)
3. Use [[claude-code]] Plan Mode for architecture decisions
4. Generate code in small, well-scoped increments
5. Review each output against specifications
6. Manage context proactively (handoff documents, /compact, subagents)
7. Lock dependencies — don't let the AI introduce new tech mid-project

## Open Questions

- At what project scale does vibe coding break down? Current evidence suggests it works well for greenfield projects but struggles with large legacy codebases.
- How do you vibe-code on a team? Multi-developer workflows with AI coding agents are still evolving (see [[multi-agent-collaboration]]).
- Will vibe coding make developers obsolete, or elevate them to architects? The [[ai-native-development]] handbook argues the latter.

## See Also

- [[document-first-system]] — the methodology that prevents shit mountain code
- [[ai-native-development]] — the broader paradigm of AI-first development
- [[context-engineering]] — managing what the AI sees in its context window
- [[vibe-design]] — the design counterpart to vibe coding
- [[claude-code]] — primary tool for vibe coding
- [[cursor]] — AI-powered IDE alternative
For the evolution from vibe coding to AI-native, see [[vibe-coding-vs-ai-native]].


## 为什么 Vibe Coding 失败（klöss）

> 氛围编程本身没问题，问题在你。

**失败模式**：
- 没有结构、没有清晰度、没有基础
- AI 是翻译器——意图是屎，代码也是屎
- 修复方法不是更好的提示词，而是**更好的理解**

**设计风格词汇表**：
- Glassmorphism（玻璃拟态）— 磨砂玻璃效果
- Neobrutalism（新粗野主义）— 厚边框、粗体原色
- Bento grid（便当网格）— 模块化卡片布局
- Dark mode（暗黑模式）— 从一开始规划双主题
- Micro-interactions（微交互）— 小动画区分专业与业余

**核心公式**：审问 → 文档（6份规范+2份会话）→ 代码。AI 做打字，你做思考。

### Sources
- raw/articles/2026-04-18-stop-vibe-coding-shit-mountain.md
