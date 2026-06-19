---
title: Spec-Driven Development (SDD)
created: 2026-05-21
updated: 2026-05-27
type: concept
tags: [methodology, engineering, multi-agent]
sources:
  - raw/articles/2026-04-18-multi-ai-sdd-coding-practice.md
---

# Spec-Driven Development (SDD)

## 定义

Spec-Driven Development（SDD，规范驱动开发）是由 GitHub 在 2025 年 9 月的 Blog 中正式提出的开发方法论。SDD 要求**规范先行，而非文档补写**——开发始于对"做什么"和"为什么"的清晰定义，技术细节暂不介入。

> Instead of coding first and writing docs later, in spec-driven development, you start with a spec. This is a contract for how your code should behave and becomes the source of truth your tools and AI agents use to generate, test, and validate code.

## 核心理念

### 1. 规范先行
开发始于业务规则、合规约束、成功标准的清晰定义。规范不是静态文档，而是随对话可变化的、可被 AI 理解并执行的 Contract。

### 2. 分阶段验证
SDD 将开发拆解为四个明确阶段：
1. **Specify** — 明确需求规范
2. **Plan** — 制定技术方案
3. **Tasks** — 拆分任务清单
4. **Implement** — 逐阶段实现

每个阶段产出物必须经人工确认后才进入下一阶段。

### 3. 规范即上下文
在 [[multi-agent-collaboration]] 场景中，统一的规范成为共享语境与约束边界。各模型基于同一份 Spec 工作，避免因理解偏差导致的返工。

## 主流 SDD 工具

| 工具 | 来源 | 特点 |
|------|------|------|
| Spec Kit | GitHub | 厂商级标准化尝试 |
| BMAD | 社区开源 | Behavior-Model-Architecture-Data 分层思想 |
| OpenSpec | Fission AI | 轻量 CLI，多 AI 友好，变更提案机制 |

## OpenSpec 工作流

OpenSpec 是当前最实用的 SDD 工具，工作流为：

1. **Draft Change Proposal** — 编写变更提案，与 AI 共享意图
2. **Review & Align** — 审查提案，编辑 specs/tasks，反馈循环
3. **Implement Tasks** — AI 基于批准的规范编写代码
4. **Archive & Update** — 归档变更，更新源规范

### 核心命令
- `/openspec:proposal` — 创建变更提案
- `/openspec:apply` — 执行变更实现
- `/openspec-archive` — 归档提案

## 与多 AI 协同

SDD 在多 AI 协同场景中尤为关键。典型模式：
- **Claude** 作为协调者，统筹全局
- **Codex** 作为高级工程师，负责代码实现
- **Gemini** 作为长文本分析师，负责大上下文分析
- 统一的 Spec 确保各模型在同一框架下工作

通过 [[mcp]] 协议将 Codex 和 Gemini 注入 [[claude-code]]，实现"同一份规范，多个模型协同"。

## 与其他方法论的关系

SDD 是从 [[vibe-coding]] 到规范开发的转变：
- Vibe Coding 快速但不可控
- SDD 在保持效率的同时增加可预测性
- 与 [[document-first-system]] 的理念一脉相承
- 与 [[harness-engineering]] 互补：SDD 管理需求，Harness 管理执行

## See Also

- [[vibe-coding]] — SDD 所修正的"自由编码"范式
- [[multi-agent-collaboration]] — SDD 在多 Agent 中的应用
- [[document-first-system]] — 先文档后代码的开发哲学
- [[harness-engineering]] — 约束 AI 执行的脚手架方法
- [[claude-code]] — SDD 实践的主要工具
- [[context-engineering]] — SDD 中的规范即上下文理念

## 企业实践案例

### binxiong 团队：跨境保险 SDD 全流程（2026-04）

使用 [[openspec]] + [[claude-code]] + Codex + Gemini，从零交付跨境保险产品：

**工作流**：
1. Spec-PRD → 2. 系统架构文档 → 3. 技术方案 → 4. OpenSpec 变更提案 → 5. 分阶段实现（每阶段人工 Review） → 6. 提案归档

**多 AI 协同**：
- Claude（协调者）+ Codex（代码专家）+ Gemini（分析专家）
- 通过 [[mcp]] 协议注入，在 [[claude-md]] 中定义强制性协作规则
- "工具调用是默认行为，不是可选项"

**关键教训**：
- 仅依赖 Spec 不足以确保高质量——还需在 CLAUDE.md 中加阶段性约束
- 分阶段交互开发比一口气生成全部代码更可控
- 工具韧性验证：Claude 不可用时可切换到其他 AI CLI
