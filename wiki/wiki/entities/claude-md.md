---
title: CLAUDE.md
created: 2026-04-23
updated: 2026-05-23
type: entity
tags: [tool, code]
sources: [10-claude-code-best-practices, claude-code-creator-15-hidden-features, claude-code-hidden-commands, claude-code-1m-context-management-guide, stop-vibe-coding-shit-mountain, raw/articles/2026-04-18-github-hot-10-open-source-projects.md, raw/articles/2026-04-18-agent-skills-four-words.md, raw/articles/2026-04-18-claude-research-10x-better.md]
---

# CLAUDE.md

## Overview

**CLAUDE.md** is a project-level configuration file for [[claude-code]] that defines rules, conventions, build commands, and architecture decisions. It is the first file Claude Code reads when entering a project directory, serving as the "constitution" for how the AI agent should behave within that project.

In [[andrej-karpathy]]'s knowledge compilation framework, CLAUDE.md serves as the **schema layer** — the configuration document that tells the LLM what the wiki's structure is, what conventions to follow, and how to handle new materials.

## Best Practices

### Keep It Concise: 60 Lines Recommended

The most critical rule comes from **Boris Cherny** (Claude Code's creator) and the **HumanLayer** team:

- Frontier LLMs can reliably follow approximately **150–200 instructions**
- Claude Code's system prompt already consumes ~50 instructions
- **HumanLayer recommends keeping CLAUDE.md under 60 lines**; 300 lines is the hard limit
- Claude often selectively ignores rules, especially those toward the end of long files

**What to include**: Build commands, test commands, branch naming conventions, project-specific architecture decisions — things Claude cannot infer from reading code.

**What to exclude**: Information Claude can deduce from the codebase itself.

### Split Into .claude/rules/ Directory

For complex projects with many rules, use the `.claude/rules/` directory to split rules into multiple loadable files. This enables on-demand loading and prevents context bloat.

### Conditional Rules

Use the `<important if="...">` tag syntax to wrap critical rules that must not be overlooked, with conditions for when they apply.

### Explore → Plan → Implement → Commit

Anthropic's recommended four-step workflow for Claude Code:

1. **Explore** — Understand the codebase and requirements
2. **Plan** — Use Plan Mode (Shift+Tab×2) to research and plan without writing code
3. **Implement** — Execute the plan in normal mode
4. **Commit** — Verify changes and commit

For complex tasks, always use Plan Mode first. For simple tasks (variable renames, quick fixes), go directly to implementation.

## Three-Layer Rule Hierarchy

Following the "约束先行" (constraints first) principle:

1. **Global CLAUDE.md** (`~/.claude/CLAUDE.md`) — Loaded for every project. Defines identity, collaboration principles, and universal work habits. This is the highest-level "constitution."
2. **Project CLAUDE.md** (`project/CLAUDE.md`) — Loaded only within a specific project. Defines directory structure, naming conventions, what goes where.
3. **Project documentation** — Architecture docs, design specs, etc. within the project.
4. **Memory files** — Auto Memory, Claude's self-notes, conversation history.

Constraints cascade downward: global rules → project rules → operational procedures. Like corporate governance — you can't rely on the CEO micromanaging every employee; you rely on institutional rules cascading through the organization.

## 「约束先行」实践案例

数字生命卡兹克分享了完整的「约束先行」落地经验。^[raw/articles/2026-04-18-agent-skills-four-words.md] 核心洞见：**Agent 的短期记忆会丢失，对话框一关全忘了，下次打开它唯一能看到的就是你留下来的文档和记忆文件。你的文档里写了什么，是不是足够清晰，直接决定了 Agent 每一次醒来的时候，是清醒的还是懵的。**

他的全局 CLAUDE.md 包含以下板块：
- **关于我**：身份、工作哲学（重复 3 遍的事 AI 化或自动化）
- **第一性原理**：所有决策从问题本质出发，不因「惯例如此」照搬
- **约束先行**：无论开发还是知识管理，第一步永远是建规则；调整规范时先改文档、再改实践
- **交互设计原则**：为目标设计、不要让用户思考、系统承担复杂性、渐进式展示、反馈引导行动
- **工作方式**：默认中文、结论先行、不问确认
- **开发习惯**：改完主动跑验证、不注释掉报错
- **Git 与部署**：commit 英文、不自动 push

关键原则：「需要调整规范时先改文档、再改实践，不要反过来」——规则不是死的，但改规则也要走规则的路。

## Claude Code Creator's CLAUDE.md

The widely-circulated [andrej-karpathy-skills](https://github.com/forrestchang/andrej-karpathy-skills) repository (37K+ stars) packages Karpathy's LLM coding insights as a CLAUDE.md file, addressing common LLM coding pitfalls and improving code quality.

## CLAUDE.md 的内容建议（来自 .claude 文件架结构深入分析）

### 应该写的内容

- 构建、测试和 lint 命令（如 `npm run test`、`make build`）
- 关键架构决策（如「基于 Turborepo 的 monorepo」）
- 不明显的注意事项（如「TypeScript 严格模式，未使用变量会报错」）
- 导入规范、命名模式、错误处理风格
- 主要模块的文件和目录结构

### 不应该写的内容

- 应写在 linter 或 formatter 配置里的内容
- 可以通过链接获取的完整文档
- 大段理论性解释

**核心原则：控制在 200 行以内。** 文件太长会占用过多上下文，反而降低 Claude 对指令的遵循效果。

### CLAUDE.local.md

个人偏好文件（如不同的测试运行器偏好），自动 gitignore，与主 CLAUDE.md 一起读取。

## Key Commands Related to CLAUDE.md

- **Plan Mode** (Shift+Tab×2): Research and plan without writing code
- **/compact**: Manual context compression
- **/rewind**: Roll back to previous checkpoint
- **/clear**: Start fresh session with new prompt
- **/statusline**: Enable persistent status bar for context monitoring

## Relationship to SKILL.md

While CLAUDE.md defines project-level rules and conventions, [[skills|SKILL.md]] files define modular capabilities that can be loaded on demand. Together they form the configuration layer for Claude Code: CLAUDE.md = "how to behave in this project," SKILL.md = "how to perform this specific capability."

## Relationships

- Core configuration for [[claude-code]]
- Created/endorsed by [[anthropic]] and Boris Cherny
- The schema layer in [[andrej-karpathy]]'s knowledge compilation architecture
- Complements [[skills|SKILL.md]] files for modular capabilities
- Used alongside [[sooul-md|SOUL.md]] in multi-agent systems like [[hermes-agent]]
- Related to [[design-md|DESIGN.md]] for visual design specifications

## See Also

- [[claude-code]] — Anthropic's CLI coding agent
- [[anthropic]] — Claude Code's creator
- [[skills]] — modular capability units (SKILL.md files)
- [[andrej-karpathy]] — whose CLAUDE.md is widely used
- [[claude-md]] — project-level configuration best practices


## 自我改进循环（klöss 方法）

- 每次纠正后，以"编辑 CLAUDE.md 这样你不会再犯那个错误"结束
- 用 **lessons.md** 进一步积累：每次 PR/调试后，Claude 用导致问题的模式和防止规则更新
- 在 CLAUDE.md 中添加：`会话开始时审查 lessons.md 获取相关项目`
- 错误率可测量地下降，因为 AI 在编码它自己的纠正

### Sources
- raw/articles/2026-04-18-stop-vibe-coding-shit-mountain.md
