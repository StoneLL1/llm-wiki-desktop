---
title: Context Engineering
created: 2026-04-23
updated: 2026-05-22
type: concept
tags: [prompt-engineering, optimization, tutorial]
sources:
  - raw/articles/claude-code-1m-context-management-guide.md
  - raw/articles/manus-agent-context-engineering.md
  - raw/articles/claude-research-10x-better.md
  - raw/articles/claude-code-session-management.md
  - raw/articles/xhs-claude-no-compact-two-methods.md
  - raw/articles/2026-04-18-build-ai-agent-framework.md
  - raw/articles/2026-04-18-claude-code-session-management.md
  - raw/articles/2026-04-18-claude-code-hidden-commands.md
---

# Context Engineering

## Definition

Context Engineering is the discipline of managing what information enters an LLM's context window to produce optimal output. Unlike prompt engineering (which focuses on crafting the right instructions), context engineering focuses on the broader ecosystem of information flow: what the model sees, in what order, with what priority, and when to remove or compress it.

As Anthropic's research and community practitioners have demonstrated, the quality of LLM output is often more dependent on *context management* than on prompt sophistication.

## Context Rot

**Context Rot** is the degradation of model performance as context grows longer. This occurs due to:

- **Attention dilution** — the model's attention mechanism spreads across more tokens, reducing focus on critical information
- **Recency bias** — later context receives disproportionate attention over earlier, potentially more important context
- **Information loss during compaction** — automatic compression discards details that may be crucial
- **Contradiction accumulation** — longer contexts accumulate conflicting instructions or information

Context rot is the primary reason why long Claude Code sessions eventually degrade in quality, even with 1M token context windows.

## Claude Code Context Management

With Claude Code's 1M context window (Claude Opus 4.6/4.7), Anthropic recommends five key usage patterns:

### 1. Manual Compaction Commands

- **`/compact`** — manually trigger context compression at a strategic point
- **`/rewind`** — roll back to a previous message, discarding later context entirely
- **`/clear`** — start a completely fresh session with a new prompt

### 2. Handoff Documents

Before a context switch or session end, create a **handoff document** that captures:
- Current state of work
- Decisions made and their rationale
- Pending tasks
- Key context that the next session needs

This is the most reliable method for preserving continuity between sessions. Claude Code can generate these documents, or humans can write them as structured Markdown files.

### 3. Plan Mode

Use **Plan Mode** to research and plan without writing code. This preserves the main context for implementation work. The pattern is:

1. Plan Mode → research architecture, explore codebase, design solution
2. Exit Plan Mode → implement with clean context focused on execution

### 4. Subagent Pattern

Delegate subtasks to separate agent instances to preserve the main session's context. The parent agent manages orchestration while child agents handle isolated tasks. This is fundamental to [[multi-agent-collaboration]] architectures.

### 5. The 50% Threshold Rule

Proactively compact or restructure context when it reaches ~50% of the window capacity. Waiting until the context is nearly full results in rushed, lossy automatic compaction. Manual intervention at the midpoint preserves critical information.

## Two Methods to Avoid Compaction Entirely

As shared by practitioner Erichain (Erichain), two approaches can eliminate the need for context compaction in Claude Code:

1. **Structured handoff documents** — write comprehensive session summaries before starting new sessions
2. **Modular task decomposition** — break work into small enough units that no single session accumulates excessive context

## Token Economics

Context engineering includes managing token usage and costs:

- 1M context windows are expensive per token — filling them carelessly wastes money
- Strategic information placement matters: instructions at the beginning and end of context receive more attention
- Compressed context is cheaper but lossy — know what you can afford to lose
- CLAUDE.md and Skills load into every session — keep them lean and essential

## Context Engineering in Research

The [[context-engineering]] methodology applies context engineering to academic research:

- Feed papers into context in structured order (abstracts first, then methods, then results)
- Maintain a running summary of key findings to prevent context rot during literature reviews
- Use subagents for parallel literature search while the main agent synthesizes

## Relationship to Other Concepts

Context engineering is foundational to effective [[vibe-coding]], [[multi-agent-collaboration]], and [[claude-code]] usage. The [[document-first-system]] methodology can be seen as a form of context engineering at the project level — creating structured documents that serve as reusable context across sessions.

## Agent 框架中的上下文工程

从 Agent 框架的视角来看，上下文工程是 Agent 智能的核心所在。Agent 框架的三大要素中：

1. **LLM Call** — 基本无工程变量（LiteLLM 库已是佼佼者）
2. **Tools Call** — 工具列表范围有业内最佳实践，取决于业务场景
3. **Context Engineering** — **最大的工程变量**，决定 Agent 的智能水平

### Agent Loop 中的上下文管理

在 [[agent-loop]] 的每次迭代中，上下文按以下规则更新：
- System Prompt 初始化上下文
- User Message 追加到上下文
- Tool Results 追加到上下文
- 每次迭代后上下文持续增长

> Agent 框架设计的核心就是在 [[agent-loop]] 这个 While 循环中设计如何管理上下文。

### 文件系统作为上下文

[[manus]] 的实践经验确立了一个重要共识：使用文件系统作为上下文。
- [[openclaw]] 的 SOUL.md / TOOLS.md / MEMORY.md
- [[claude-code]] 的 CLAUDE.md / AGENTS.md / Skills
- 文件系统提供了持久化、可共享、可版本化的上下文载体

Shunyu Yao 团队在腾讯混元官网发文指出："模型想要迈向高价值应用，核心瓶颈就在于能否用好 Context。"——上下文工程仍是 Agent 领域中低垂的果实。

## Session Management 策略

[[claude-code-session-management]] 是 Context Engineering 在 [[claude-code]] 中的系统化实践。Claude Code 核心开发者 Thariq Shihipar 提出了「五条岔路」决策框架：每完成一步操作后，在继续对话、Rewind、/clear、Compact、子 Agent 之间做出选择。核心洞见：

- Context 越长注意力越分散（context rot），而非「能装更多东西」
- 自动 compaction 发生在模型最不聪明的时候——应主动在 context 50% 时手动 `/compact`
- Rewind 不只是「撤销」，更是「时光信」——先让 Claude 总结经验，再回退应用
- 子 Agent 是保护主 context 的关键：中间噪音留在子 context，只拿结论

社区最佳实践还包括：用 `/btw` 插话不污染主对话（复用 prompt cache，零额外 token），用 Hook 在工具调用 >8 次时自动触发 Skill 优化建议。

## Open Questions

- How much context is "enough" for different task types?
- Can we measure context rot quantitatively?
- What is the optimal context structure for complex multi-step tasks?
- How will context engineering evolve as context windows grow beyond 1M tokens?

## See Also

- [[claude-code]] — primary tool where context engineering is applied
- [[claude-model-family]] — models with 1M context windows
- [[claude-md]] — project-level configuration that loads into every session
- [[vibe-coding]] — paradigm that requires strong context engineering
- [[multi-agent-collaboration]] — uses subagents to manage context across agents
- [[ai-research-workflow]] — applies context engineering to academic research
- [[agent-loop]] — context engineering 的运行载体
- [[manus]] — "文件系统作为上下文"理念的先驱实践者


## 从 Agent 框架看上下文工程（yabohe / 腾讯技术工程）

Agent 框架三大部分：
1. **LLM Call**：API 管理范畴，兼容各大厂商 API。LiteLLM 库已是佼佼者
2. **Tools Call**：LLM 使用外部工具，从 Function Call → MCP → Skills
3. **Context Engineering**（最大变量）：
   - 狭义：Prompt 工程实现（Rules、CLAUDE.md、AGENTS.md）
   - 广义：也包含工具使用（Skills = 工具 + 提示词结合）

Shunyu Yao 团队观点：「模型迈向高价值应用，核心瓶颈在于能否用好 Context。不提供任何 Context 时，GPT-5.1 (High) 仅能解决不到 1% 任务。」

### Sources
- raw/articles/2026-04-18-build-ai-agent-framework.md
