---
title: Document-First System
created: 2026-04-23
updated: 2026-05-23
type: concept
tags: [code, tutorial]
sources:
  - raw/articles/2026-04-18-stop-vibe-coding-shit-mountain.md
  - raw/articles/2026-04-18-zero-human-coding-ai-native-dev-handbook.md
  - raw/articles/2026-04-18-multi-ai-sdd-coding-practice.md
---

# Document-First System

## Definition

The Document-First System is a development methodology that requires writing comprehensive specification documents *before* any code is generated. It was developed as a direct antidote to undisciplined [[vibe-coding]] — the "shit mountain" problem where AI-generated code piles up without structure, consistency, or clear purpose.

The core principle: **documents are the source of truth, not the code.** AI agents read these documents as context and generate implementations that conform to the specifications.

## The Six Core Documents

### 1. PRD.md (Product Requirements Document)

Defines **what** you are building:
- Problem statement and target users
- Feature list with priority levels
- User stories and acceptance criteria
- Non-functional requirements (performance, security, accessibility)
- Out of scope — explicitly stating what NOT to build

### 2. APP_FLOW.md (Application Flow)

Defines **how** users navigate the application:
- Screen-by-screen navigation flow
- User journey maps
- State transitions
- Edge cases and error states
- Often includes wireframe descriptions or links to design mockups

### 3. TECH_STACK.md (Technology Stack)

Defines **which** technologies to use, with **locked versions**:
- Frontend framework and version (e.g., React 18, Next.js 14)
- Styling solution (e.g., Tailwind CSS)
- Backend framework and version
- Database and ORM
- API patterns (REST, GraphQL)
- Deployment target

**Critical rule**: once TECH_STACK.md is written, the AI must not suggest alternative technologies. Locking the stack prevents the AI from introducing dependency chaos.

### 4. FRONTEND_GUIDELINES.md (Frontend Design System)

Defines the complete visual and interaction design system:
- Color palette and design tokens
- Typography scale
- Component library specifications
- Responsive design breakpoints
- Animation and interaction patterns
- This document often overlaps with or references the [[vibe-design]] DESIGN.md

### 5. BACKEND_STRUCTURE.md (Backend Architecture)

Defines the server-side architecture:
- Database schema with entity relationships
- API endpoint contracts (request/response formats)
- Authentication and authorization logic
- Data validation rules
- Error handling patterns
- Caching strategy

### 6. IMPLEMENTATION_PLAN.md (Build Sequence)

Defines **in what order** to build:
- Phased build plan with dependencies
- Task breakdown for each phase
- Estimated complexity per task
- Testing requirements per phase
- Milestones and checkpoints

## Why It Works with AI

The document-first approach is particularly effective with AI coding agents because:

1. **Context richness**: Documents provide structured, comprehensive context that natural language prompts alone cannot match
2. **Consistency**: Multiple AI sessions (or multiple AI tools) can read the same documents and produce consistent output
3. **Reduced ambiguity**: Explicit specifications leave less room for the AI to "guess" what you want
4. **Reviewable**: Humans can review and approve documents before any code is generated
5. **Reusable**: Documents persist across sessions, projects, and even teams

## OpenSpec Framework

[[openclaw]] and related tools use **OpenSpec** — a specification framework that connects design tools to coding tools through standardized document formats. OpenSpec ensures that specifications written in design tools are automatically available to coding agents.

## Multi-AI SDD (Spec-Driven Development)

The Multi-AI SDD approach uses **multiple AI models** in a spec-driven development workflow:

1. One model generates/refines specifications
2. A different model implements code against those specifications
3. A third model reviews code against specifications

This separation of concerns prevents a single model from creating self-referential, unreviewed output.

## Practical Workflow

1. Write PRD.md — define the product
2. Write APP_FLOW.md — map the user journey
3. Write TECH_STACK.md — lock your dependencies
4. Write FRONTEND_GUIDELINES.md — define the design system
5. Write BACKEND_STRUCTURE.md — define the architecture
6. Write IMPLEMENTATION_PLAN.md — sequence the build
7. Configure CLAUDE.md to reference all documents
8. Begin [[vibe-coding]] with specifications as context
9. Review each generated component against specifications
10. Update documents when requirements change

## Open Questions

- How much documentation is "enough" before starting to code?
- Do documents need to be maintained as the codebase evolves?
- Can AI itself generate good specification documents?
- How does document-first work for exploratory/prototype projects?

## See Also

- [[vibe-coding]] — the paradigm that document-first disciplines
- [[ai-native-development]] — broader AI-first development paradigm
- [[claude-code]] — primary tool that reads these documents as context
- [[vibe-design]] — DESIGN.md as part of the document-first system
- [[claude-md]] — project configuration that can reference specification documents


## Vibe Coding 文档栈（klöss）

### 六份规范文档
1. **PRD.md** — 产品需求文档：功能范围、用户故事、成功标准
2. **APP_FLOW.md** — 页面导航路径：逐步序列、决策点、错误处理
3. **TECH_STACK.md** — 依赖锁定到确切版本
4. **FRONTEND_GUIDELINES.md** — 完整设计系统：色值、间距、组件样式
5. **BACKEND_STRUCTURE.md** — 数据库模式、API 端点、存储规则
6. **IMPLEMENTATION_PLAN.md** — 逐步构建序列

### 两份会话文件
- **CLAUDE.md** — AI 每次会话自动读取的规则文件（活的文档，持续自我改进）
- **progress.txt** — 跟踪已完成/进行中/待办，AI 跨会话记忆的桥梁

### Interrogation 系统
在写文档之前，让 AI 无尽审问你的想法：
> "在写任何代码之前，在 Planning 模式下无尽地审问我的想法。不要假设任何问题。问问题直到没有假设剩下。"

顺序：**审问 → 文档 → 代码**。永远不要跳过。

### 多工具工作流
- **Claude** → 思考（审问、文档、架构）
- **Cursor Agent** → 构建（一般实施）
- **Kimi K2.5** → 视觉重前端（截图→像素级代码）
- **Codex** → 调试和完成（跨文件 bug 追踪、测试）
- **Git worktrees** → 并行会话（3-5 个 worktree 同时开发）

### Sources
- raw/articles/2026-04-18-stop-vibe-coding-shit-mountain.md（klöss @kloss_xyz）
