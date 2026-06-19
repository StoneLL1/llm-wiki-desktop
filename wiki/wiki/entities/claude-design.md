---
title: Claude Design
created: 2026-04-23
updated: 2026-05-23
type: entity
tags: [tool, multimodal, design]
sources:
  - raw/articles/2026-04-19-claude-design-impact-on-ai-design-vendors.md
  - raw/articles/2026-04-19-claude-design-system-prompt-bilingual.md
  - raw/articles/2026-04-19-claude-design-system-prompt-leak-analysis.md
---

# Claude Design

## Overview

Claude Design is [[anthropic]]'s conversational design tool that generates UI prototypes, presentation decks, and PPT files through natural language interaction. Powered by [[claude-model-family]] (specifically Claude Opus 4.7), it represents Anthropic's entry into the AI-assisted design space, directly competing with established design platforms like [[figma]], Adobe, Canva, and Wix.

## Key Features

### 37 Integrated Tools
Claude Design includes 37 tools for comprehensive design workflows, covering prototyping, layout, typography, color systems, and asset generation.

### 13 Built-in Skills
The tool ships with 13 pre-built skills that encode design best practices and workflows. These follow the SKILL.md convention used across Anthropic's product ecosystem.

### Tweaks Protocol
A PostMessage-based protocol that enables AI-generated prototypes to be interactively adjustable by users. The Tweaks Protocol allows real-time modification of design parameters without regenerating the entire prototype, creating a more iterative design experience.

### snip Tool
A delayed-execution context management tool designed for long conversations. The snip tool helps manage [[context-engineering]] in extended design sessions by strategically pruning and preserving relevant context.

### xhigh Effort Level
Claude Design leverages Claude Opus 4.7's `xhigh` effort level for maximum quality output in design generation tasks.

## System Prompt Leak

Claude Design's system prompt was leaked by security researcher **Pliny the Liberator** and published on the CL4R1T4S GitHub repository. The leak revealed significant details about Claude Design's architecture and capabilities:

### Bilingual System Prompt
The system prompt operates in both English and Chinese, reflecting Anthropic's international market strategy. Key architectural insights from the bilingual prompt analysis (claude-design-system-prompt-bilingual):

- **Negative prompting**: Extensive use of anti-patterns and forbidden styles
- **Strong constraint words**: Heavy use of MUST/NEVER/CRITICAL to enforce model compliance
- **Skill loading**: Minimal base prompt with on-demand skill-specific prompt injection (cold start + hot load pattern)
- **Context management**: Sophisticated multi-stage context handling for long design sessions

### Design Philosophy
The leaked prompt reveals Claude Design's approach:

1. Start with a minimal base prompt
2. Load design-specific skills on demand based on user intent
3. Apply strong constraints to maintain design quality
4. Use negative prompting to avoid common AI design anti-patterns
5. Manage context aggressively to maintain coherence in long sessions

## Industry Impact

Claude Design has significant disruptive potential for the design tool industry:

### Threatened Vendors
- **[[figma]]** — The leading design platform; has responded with Code to Canvas integration for Claude Code
- **Adobe** — Design tool giant whose Creative Cloud suite faces competition
- **Canva** — Integrated with Claude Design for one-click editing, suggesting a partnership/adaptation strategy
- **Wix** — Website builder impacted by AI-native design generation

### Market Positioning
Claude Design positions itself as a conversational alternative to traditional design tools, reducing the barrier to entry for design work. Rather than requiring design tool expertise, users describe their intent in natural language and Claude Design generates polished outputs.

## Handoff to Claude Code

Claude Design 与 [[claude-code]] 之间最关键的功能是 **Handoff to Claude Code**：

- 在 Claude Design 中敲定设计后，导出为 Handoff Bundle（包含布局语义、组件层级、设计意图）
- 将 Bundle 交给 Claude Code，一句话即可开始编码实现
- Brilliant 案例显示：其他工具需 20+ 次 prompt 的复杂页面，Claude Design 只需 2 次
- 这是原生态 [[vibe-design]] 的完整闭环：对话设计 → Handoff → 代码实现

### 与之前路线的区别

Claude Design 发布前，社区解决 Vibe Coding 前端 UI 问题的路线有三条：

1. **DESIGN.md 约束 AI** — 文本规范路线，AI 自由发挥空间仍大
2. **Figma MCP / Pencil** — 接入已有设计工具，适合有设计稿的团队
3. **Lovart** — 品牌设计等独立创作场景

这三条路线的共性：设计在专门工具中完成，再导给 Claude Code。

Claude Design **反过来了**：设计起点直接在对话窗口中，Opus 4.7 读代码库和设计文件后自动推出设计系统，最后一步才打包交给 Claude Code。一句话：把设计到开发的多工具串联压缩成单对话完成。

## Target Users

Claude Design 的目标用户**不是专业设计师**，而是：

- 没有设计背景的创始人
- 产品经理
- 营销团队成员
- 做 [[vibe-coding]] 的开发者（最需要 Handoff 功能）

专业设计师的精细控制、多人协作、设计规范管理需求，[[figma]] 短期内仍有护城河。

## Relationship to Claude Code

Claude Design shares the Anthropic product ecosystem with [[claude-code]]:

- Both use the SKILL.md convention for modular capabilities
- Both leverage MCP for external tool integration
- Both follow Anthropic's skill best practices
- Claude Code can connect to design tools (Figma, Pencil) via MCP

## Design Paradigm: Vibe Design

Claude Design exemplifies the [[vibe-design]] paradigm — using natural language to generate UI designs. This is the design-world complement to [[vibe-coding]] in software development:

- **Vibe Coding**: Describe software intent → AI generates code
- **Vibe Design**: Describe design intent → AI generates visual designs

## Related Products

- **Claude Code** — Anthropic's CLI coding agent
- **Claude Cowork** — Anthropic's desktop-level filesystem agent
- **Stitch (Google Labs)** — Google's competing AI-native UI design platform
- **Lovart** — AI brand design tool with recent major feature updates
- **v0 (Vercel)** — Vercel's AI UI generation tool
- **Lovable** — AI app builder tool

## Sources

- claude-design-impact-on-ai-design-vendors — Industry impact analysis
- claude-design-system-prompt-bilingual — Bilingual system prompt analysis with architectural insights
- claude-design-system-prompt-leak-analysis — System prompt leak details and security implications

## 2026-05 生态扩展

### Open-Design 开源平替（nexu-io）
nexu-io 推出的 Open-Design 项目提供了 Claude Design 的开源平替方案。Open-Design 复刻了 Claude Design 的核心交互模式——自然语言描述 → 设计生成 → 迭代调整，同时保持完全开源和可定制。这反映了 AI 设计工具生态中开源与商业产品并行的趋势。

### 与 hue、Kami 等设计 Skill 的生态关联
Claude Design 的能力正在通过 Skill 生态向外扩展：
- **hue**：色彩系统设计 Skill，提供品牌色板生成、色彩无障碍检查等功能
- **Kami**：模板化设计 Skill，聚焦于标准化文档和演示文稿的批量生成
- 这些设计 Skill 可以作为 [[claude-code]] 的插件加载，与 Claude Design 形成互补

设计 Skill 生态的繁荣表明，Claude Design 的架构（37 工具 + 13 内置 Skill）正在成为 AI 设计工具的事实标准。
For a comparison with Figma/Adobe, see [[claude-design-vs-traditional-tools]].
Related: [[hue]] — brand design skill that learns from URLs/screenshots.
Related: [[kami]] — AI document design system by tw93.

## See Also

- [[pliny-the-liberator|Pliny the Liberator]]
