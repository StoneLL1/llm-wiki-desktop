---
title: Claude Code
created: 2026-04-23
updated: 2026-06-10
type: entity
tags: [tool, code, agent]
sources:
  - claude-code-10-more-worthwhile-skills
  - claude-code-creator-15-hidden-features
  - claude-code-hidden-commands
  - claude-code-session-management
  - claude-research-10x-better
  - 10-claude-code-best-practices
  - xhs-claude-code-terminal-setup
  - xhs-claude-no-compact-two-methods
  - claude-code-1m-context-management-guide
  - raw/articles/2026-05-13-skill-engineering-design.md
  - raw/articles/2026-05-14-anthropic-financial-skills.md
  - raw/articles/2026-05-12-claude-code-feishu-agent-workflows.md
  - raw/articles/2026-05-19-claude-code-slash-commands-guide.md
  - raw/articles/2026-04-18-claude-code-creator-15-hidden-features.md
  - raw/articles/2026-04-18-claude-code-hidden-commands.md
  - raw/articles/2026-04-18-claude-code-mobile-remote.md
  - raw/articles/2026-04-18-claude-code-session-management.md
  - raw/articles/2026-04-18-everything-claude-code-plugin-library.md
  - raw/articles/2026-04-20-10-claude-code-best-practices.md
  - raw/articles/2026-04-21-claude-code-1m-context-management-guide.md
  - raw/articles/2026-04-27-claude-code-agent-teams-best-practices.md
  - raw/articles/2026-05-25-guangguang-github-claude-plugins-official.md
  - raw/GitHub/anthropics-claude-plugins-official.md
  - raw/articles/2026-06-03-claude-code-ai-native-engineering-org.md
---

# Claude Code

## Overview

Claude Code is [[anthropic]]'s CLI-based coding agent and the most frequently mentioned tool in this wiki's corpus (25+ articles). Created by [[boris-cherny]], it functions as an AI-powered terminal assistant that can read, write, and execute code, manage projects, and integrate with external tools via MCP. Claude Code is powered by the [[claude-model-family]], particularly Claude Opus models with their 1M token context window.

## Core Features

### Skills System
Claude Code supports modular Skills defined as SKILL.md files that can be loaded on demand. This follows Anthropic's published best practices for skill design. Skills enable capabilities like:

- Video editing (video-use skill)
- LaTeX paper writing
- Stata econometrics analysis
- Architecture diagram generation
- Scientific research workflows

See [[skills]] for the broader skills ecosystem.

### MCP Integration
Claude Code uses the [[mcp]] (Model Context Protocol) to connect to external tools and data sources. This allows it to interact with browsers (Playwright MCP), design tools (Figma, Pencil), and other services. MCP servers provide tool integration while maintaining security boundaries.

### Plan Mode
A dedicated mode where Claude Code only researches and plans without writing code. This follows Anthropic's recommended Explore → Plan → Implement → Commit workflow. Plan Mode is particularly useful for complex tasks that benefit from thorough analysis before implementation.

### Subagent Pattern
Claude Code can delegate subtasks to separate agent instances to preserve the main conversation context. This is essential for managing [[context-engineering]] in long sessions. The evolution of this pattern now includes [[agent-teams]] (parallel multi-agent teams) and [[claude-code-dynamic-workflow]] (JS-scripted 100+ parallel subagents with cross-validation).

> **四种并行方案对比**：鲁工将 Claude Code 并行能力系统梳理为 Subagents、Agent View、Agent Teams、Dynamic Workflows 四种方案，核心区别在于"谁来拿主意"。详见 [[agent-teams]] 的「四种并行方案对比汇总」章节。

### Dynamic Workflow（2026-05 新增）
[[claude-code-dynamic-workflow]] 是 Claude Code v2.1.154+ 的研究预览功能：用户给需求，Claude 自己写 JavaScript 编排脚本，在后台并行跑几十到上百个 subagents，交叉验证后汇总结果。与传统 subagents 的核心区别在于：计划搬进代码（而非 Claude 逐轮决策），支持「多角度解题 + 专门挑刺 Agent」的质量套路。Bun 作者用此功能 11 天完成 75 万行 Zig→Rust 移植，测试 99.8% 通过。详见 [[claude-code-dynamic-workflow]]。

### CLAUDE.md Configuration
The project-level CLAUDE.md file defines rules, conventions, build commands, and architecture decisions. See [[claude-md]] for detailed conventions. Best practices include:

- Keep CLAUDE.md under 60 lines (recommended by HumanLayer)
- Split rules into `.claude/rules/` directory for larger projects
- Use `<important if="...">` tag syntax for conditional rules
- Include build commands, tech stack, and coding conventions

### Effort Levels
Claude Code supports multiple effort levels including the `xhigh` setting available with Claude Opus 4.7, which produces maximum quality output at higher computational cost.

## Context Management

Managing the context window is one of the most critical aspects of using Claude Code effectively. With Claude Opus 4.6's 1M token context window, Claude Code can handle very long tasks, but [[context-engineering]] remains essential.

### Key Commands（2026-05 更新）

[[claude-code-slash-commands|完整斜杠命令参考]] — Claude Code 内置 50+ 斜杠命令，覆盖会话管理（`/clear`、`/compact`、`/resume`）、信息诊断（`/usage`、`/context`、`/diff`）、模式控制（`/plan`、`/goal`、`/effort`）、代码审查（`/review`、`/simplify`）、子代理并行（`/agents`、`/tasks`、`/background`、`/loop`）等 6 大分类。可直接用中文描述需求，Claude 自动匹配对应命令。

### Context Management Strategies

Two primary approaches for managing long sessions:

1. **Auto Compact**: Automatic context compression when context fills up. Can be lossy — important details may be dropped.
2. **Handoff Documents**: Create summary documents before context switches to preserve state. This is the recommended approach for complex workflows.

### Environment Variables

- `CLAUDE_CODE_NO_FLICKER=1` — Enables mouse cursor control in Claude Code terminal

## Best Practices

Anthropic and the community have established several best practices for Claude Code usage:

1. **Explore → Plan → Implement → Commit workflow**: Research first, plan the approach, implement code, then commit
2. **60-line CLAUDE.md limit**: Keep project configuration concise and focused（300 行硬上限，前沿 LLM 可靠跟随约 150-200 条指令，系统提示已占 50 条）
3. **Use Plan Mode for complex tasks**: Don't rush into implementation（Shift+Tab 切两次进入 Plan Mode）
4. **Leverage subagents**: Delegate subtasks to preserve main context（prompt 里直接说 "use subagents"）
5. **Create handoff documents**: When switching contexts, summarize state for continuity
6. **Use `/compact` strategically**: Don't rely solely on auto-compact; manual compression gives more control
7. **Split rules into files**: Use `.claude/rules/` for project-specific rule sets
8. **让 Claude 先采访你再动手**：给简单需求描述，让它用 AskUserQuestion 问细节，采访完开新会话执行
9. **对平庸方案直接要求重写**："knowing everything you know now, scrap this and implement the elegant solution"
10. **贴 bug 说 fix，别微操**：把错误信息贴给 Claude，说 "fix"，不要指导怎么修
11. **上下文 50% 就手动 compact**：超过 60-70% 进入 "agent dumb zone"，表现明显下降
12. **走偏了 Esc Esc 回滚**：不要在同一上下文里纠正，回滚后带着新理解重新 prompt
13. **小任务别用复杂工作流**：原生 Claude Code 处理小任务比任何复杂工作流都快
14. **建功能特定的 subagent**：如 "前端组件 agent" 而非通用的 "QA agent"
15. **每个 Skill 建 Gotchas 部分**：记录失败模式，时间一长成为信噪比最高的内容

### 插件生态系统

Claude Code 的插件生态持续壮大：
- **[[superpowers]]**（131k Star）— 多 agent 开发方法论
- **[[everything-claude-code]]**（132k Star）— 36 agent + 150 skills 工具箱
- **claude-code-best-practice**（29.4k Star）— 84 条最佳实践集合
- **[[claude-plugins-official]]**（27.4k Star）— Anthropic 官方插件目录，30+ 内部插件，含 claude-code-setup、feature-dev、hookify、code-modernization 等

## Terminal Setup

Recommended terminal tools for Claude Code (from xhs-claude-code-terminal-setup, CrazyAllen, 542 赞 836 收藏):

- **[[ghostty]]** — Terminal emulator with flexible split-screen, tabs, copy-on-select, clipboard-trim-trailing-spaces, clipboard-paste-protection, and clickable URLs (⌘+click)
- **[[yazi]]** — Terminal file manager with in-window file/image preview and editing
- **`/statusline`** — Built-in Claude Code command that adds a persistent status bar showing current model, context window usage, token cost, and git branch
- **`CLAUDE_CODE_NO_FLICKER=1`** — Environment variable that enables mouse cursor control in Claude Code terminal

## Comparison with Alternatives

Claude Code is frequently compared with:

- **[[cursor]]** — AI-powered IDE; Claude Code is CLI-based while Cursor is GUI-based
- **OpenAI Codex** — OpenAI's CLI coding agent
- **[[openclaw]]** — Open-source multi-agent platform, alternative to Claude Code
- **[[hermes-agent]]** — NousResearch's open-source AI agent

## Remote Access & Mobile

Claude Code 支持多种远程访问方式：

- **官方移动端** — Claude iOS App 内置 Code 标签页，手机直接写代码
- **多端流转** — `--teleport` 和 `/remote-control` 在手机/网页/桌面/终端之间无缝切换会话
- **Session Spawning** — `claude remote-control` 从手机发起全新会话
- **第三方方案** — [[nexus4cc]] 开源项目通过 WebSocket + tmux 实现手机浏览器远程操控
- **Claude Code Channels** — 集成 Telegram 和 Discord 控制编码会话

## 隐藏功能（Boris Cherny 推荐）

[[boris-cherny]] 分享的关键使用技巧：

- **`/batch`** — 任务拆分并行到多个 worktree Agent，适合大型迁移和批量重构
- **`--bare`** — 非交互调用跳过配置加载，启动提速 10 倍
- **`--agent`** — 自定义 Agent（`.claude/agents/` 目录），如只读 Agent
- **`/voice`** — CLI 语音输入编程（暂不支持中文）
- **Chrome 插件 / 内置浏览器** — 给 Claude 验证前端渲染产出的能力
- **Hooks** — 在 Agent 生命周期插入自定义逻辑（SessionStart、PreToolUse）
- **Dispatch（Claude Cowork）** — 手机给电脑上的 Claude 派任务

## Session Management

参见 [[claude-code-session-management]] — Thariq Shihipar 提出的「五条岔路」框架：继续对话、Rewind、/clear、Compact、子 Agent。

## Research Applications

Beyond coding, Claude Code is used for:

- Academic paper writing (with Overleaf + LaTeX)
- Deep research workflows
- Econometrics analysis (Stata)
- Video editing pipelines
- Scientific literature surveys

## Sources

Key source articles covering Claude Code features, best practices, and workflows: claude-code-10-more-worthwhile-skills, claude-code-creator-15-hidden-features, claude-code-hidden-commands, claude-code-session-management, claude-research-10x-better, 10-claude-code-best-practices, xhs-claude-code-terminal-setup, xhs-claude-no-compact-two-methods, claude-code-1m-context-management-guide

## 2026-05 新进展

### Agent Teams
多 Agent 并行协作功能，允许 Claude Code 同时调度多个 Agent 实例处理独立子任务。这标志着从单 Agent 工作流向 [[multi-agent-collaboration]] 模式的演进，大幅提升了复杂项目的执行效率。

### Claude Managed Agents
[[anthropic]] 推出的托管 Agent 基础设施（Managed Agents API），支持无头（headless）部署模式。Agent 可以作为后端服务持续运行，无需人工交互，适用于定时任务、事件驱动和 API 调用场景。

### Routines
自主定时工作流功能，允许定义周期性执行的任务计划。Routines 可以自动触发 Agent 在指定时间执行特定 Skill，实现"设定即忘记"的自动化运维模式。

### Claude Agent SDK
通用 Agent 编排框架，提供标准化的 API 用于构建、组合和管理多 Agent 系统。SDK 支持 Skill 加载、[[mcp]] 工具集成、状态管理和错误处理，是 [[harness-engineering]] 方法论的基础设施层。

### 飞书 CLI 集成
Claude Code 新增与 [[feishu]] 的 CLI 集成，支持：
- 在飞书对话中直接调用 Claude Code
- 飞书文档与代码仓库的双向同步
- 飞书审批流与 Agent 工作流的对接

### 长程 Agent Harness 设计
引入 Initializer Agent + Coding Agent 的双 Agent 架构：
- **Initializer Agent**：负责项目初始化、环境配置和任务规划
- **Coding Agent**：专注代码实现，由 Initializer Agent 驱动

这种分工模式有效解决了长程任务中上下文膨胀和注意力衰减的问题，参见 [[skill-engineering]] 和 [[harness-engineering]]。
See also: [[agent-memory-systems]] for persistent memory across sessions.
The [[agent-teams]] feature enables multi-agent parallel execution.
For a detailed comparison, see [[claude-code-vs-openclaw-vs-hermes]].
Competitor: [[openai-codex]] — OpenAI's coding agent product.

## AI-Native Engineering Organization (Fiona Fung, 2026-06)

[[fiona-fung|Fiona Fung]] (Director of Engineering, Claude Code & Claude Cowork) shared how her team at [[anthropic]] transformed their engineering processes after adopting agentic coding as the default working mode. The core insight: **bottlenecks shifted from writing code to verification, review, and security.**

### Four Process Transformations

1. **JIT Roadmaps** — Abandoned six-month roadmaps (obsolete within three months due to Claude Code's acceleration). Shifted to: rapid prototype → get internal users on it → act on feedback. Design discussions moved from separate docs into PRs and prototypes.

2. **Context Collection: Ask Claude First** — Instead of finding the code author, engineers ask Claude directly ("Who caused this regression?" "What's the reasoning?"). Perpetual question: "Is there a way to automate this?"

3. **Code Review: Trust But Verify** — Claude auto-handles style, lint, PR feedback, bug catching, pre-commit fixes, and test additions. Humans focus exclusively on: **legal review, trust-boundary/security code, and product taste.** The trust-verify balance must be continuously re-evaluated as models improve.

4. **Team Composition: Role Blurring** — PMs do substantial coding; engineers take on content and design. Two talent types prioritized: creative builders with product sense, and engineers with deep system expertise.

### Dogfooding & Culture

Every team member (including cross-functional partners) uses Claude Code and Claude Cowork. Managers remain hands-on ICs writing real code. Flattest possible team structure.

This connects directly with [[claude-code-self-check|self-check feedback loops]] — humans move from checking code to checking rules. See also [[ai-native-development]].

## See Also

- [[claude-code-folder-structure|.claude 文件夹结构]]
- [[pinme|PinMe 一键部署]]
