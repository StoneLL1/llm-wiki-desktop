---
title: Multi-Agent Collaboration
created: 2026-04-23
updated: 2026-05-27
type: concept
tags: [agent, architecture, tutorial]
sources:
  - raw/articles/openclaw-discord-ai-research-team.md
  - raw/articles/hermes-multi-agent-collaboration-guide.md
  - raw/articles/multi-agent-collaboration-guide.md
  - raw/articles/build-ai-agent-framework.md
  - raw/articles/zero-human-coding-ai-native-dev-handbook.md
  - raw/articles/2026-04-18-multi-agent-collaboration-guide.md
  - raw/articles/2026-04-18-build-ai-agent-framework.md
  - raw/articles/2026-04-21-hermes-multi-agent-collaboration-guide.md
  - raw/articles/2026-05-11-x-how-to-build-first-ai-agent-10k-plus.md
  - raw/articles/2026-04-18-multi-ai-sdd-coding-practice.md
---

# Multi-Agent Collaboration

## Definition

Multi-Agent Collaboration is the paradigm of having multiple AI agents work together on complex tasks through role division, message routing, shared memory, and coordinated workflows. Rather than a single monolithic agent attempting everything, specialized agents handle distinct responsibilities while communicating through structured protocols.

As practitioner 林月半子 observes: **"这不是技术活，是管理活"** — "This isn't technical work, it's management work." Building effective multi-agent systems is fundamentally about organizational design, not just technical architecture.

## Five Architectures

### 1. Central Orchestrator

A single controller agent dispatches tasks to worker agents and aggregates results. Simple to implement but creates a bottleneck and single point of failure.

### 2. Peer-to-Peer

Agents communicate directly with each other without a central controller. More flexible but harder to coordinate and debug.

### 3. Hierarchical

Agents are organized in a tree structure with managers and workers. Natural for complex projects but requires careful role definition.

### 4. Blackboard

Agents share a common data space (the "blackboard") where they read and write information. Decoupled communication but requires conflict resolution.

### 5. Hybrid

Combines elements of the above architectures. Most real-world systems end up as hybrids — e.g., a central orchestrator for task allocation with peer-to-peer communication for collaboration between workers.

## OpenClaw Approach

[[openclaw]] implements a sophisticated multi-agent system with three key innovations:

### Bindings Routing

Deterministic message routing based on the mapping of **channel + account → agent**. This ensures that messages in a Discord channel are automatically directed to the appropriate specialized agent based on preconfigured bindings.

### Dual-track Governance

- **Config track**: Hard constraints that cannot be overridden (safety, access control, capability boundaries)
- **Rules track**: Soft guidance that agents can interpret and adapt (behavioral guidelines, task preferences)

This separation prevents agents from accidentally overriding critical safety constraints while maintaining flexibility for operational decisions.

### Five-layer Memory Architecture

1. **Daily logs** — ephemeral, session-scoped notes
2. **Long-term memory** — persistent individual agent knowledge (MEMORY.md)
3. **Group memory** — shared team knowledge (GROUP_MEMORY.md)
4. **Cold archive** — infrequently accessed historical data
5. **Semantic retrieval** — vector-based search across all memory layers

## Hermes Approach

[[hermes-agent]] by NousResearch implements multi-agent collaboration through:

### Agent Profiles

Each agent runs in a **process-isolated profile** with independent configuration, memory, skills, and gateway. This ensures complete isolation between agents — one agent's context or state cannot leak into another.

### Gateway Routing

A gateway layer handles message routing between agents, Discord channels, and external services. The gateway enforces routing rules and provides a single integration point.

### Discord Integration

Discord serves as the primary collaboration platform. Each agent has its own Discord presence, and channels are organized by project or function. Human team members interact with agents through the same Discord interface.

## Practical Considerations

### DM Scope (Session Isolation)

Per-account-channel-peer session isolation ensures multi-user privacy. Each user's conversation with an agent is completely isolated from other users' sessions.

### Skills Distribution

Different agents load different [[skills]] (SKILL.md files). A research agent loads literature search and analysis skills, while a coding agent loads implementation skills. This specialization prevents context bloat and improves output quality.

### Human-in-the-Loop

Most multi-agent systems require human oversight for:
- Task prioritization and approval
- Quality review of agent outputs
- Conflict resolution between agents
- Strategic direction setting

## Tools for Multi-Agent Systems

| Tool | Platform | Key Feature |
|------|----------|-------------|
| [[openclaw]] | Discord, WeChat | Bindings routing, five-layer memory |
| [[hermes-agent]] | Discord | Process-isolated profiles, gateway |
| [[claude-code]] | CLI | Subagent pattern, Plan Mode |
| Claude Cowork | Desktop | Filesystem-level multi-agent |
| Codex (OpenAI) | CLI | Multi-agent coding |

## Open Questions

- What is the optimal number of agents for different task types?
- How do you measure multi-agent system effectiveness?
- Can multi-agent systems self-organize without human management?
- How do memory systems scale across many agents?

## See Also

- [[openclaw]] — open-source multi-agent platform
- [[hermes-agent]] — NousResearch's process-isolated agent
- [[claude-code]] — supports subagent pattern for delegation
- [[skills]] — modular capability units loaded by agents
- [[context-engineering]] — managing context across agent boundaries
- [[vibe-coding]] — can be enhanced with multi-agent approaches
See [[agent-teams]] for Claude Code's built-in team orchestration.

## Anthropic 的 5 种多 Agent 模式（2026）

来自 1 万+ 赞推文总结的 Anthropic 官方推荐的 5 种多 Agent 架构模式：

### 模式 1：Prompt Chaining（串行管道）
将复杂任务拆分为串行的子任务管道，每一步的输出是下一步的输入。
- 优势：每步专注单一任务，降低单 Agent 的认知负担
- 风险：管道中任一步骤失败会导致后续全部失败
- 适用：线性工作流，如"调研→大纲→起草→审阅→定稿"

### 模式 2：Routing（路由分发）
一个路由 Agent 分析输入，将其分发给最合适的专家 Agent 处理。
- 优势：每个专家 Agent 可以针对特定领域优化
- 风险：路由决策的准确性是关键瓶颈
- 适用：客服系统、多领域问答

### 模式 3：Parallelization（并行执行）
多个 Agent 同时独立处理同一任务的不同方面，结果汇总。
- 优势：显著提升处理速度
- 风险：结果汇总和冲突解决可能复杂
- 适用：代码审查（多人同时审查不同方面）、多角度分析

### 模式 4：Orchestrator-Workers（编排者-工人）
一个编排者 Agent 动态分解任务并分配给工人 Agent，根据结果调整计划。
- 优势：灵活应对复杂和不确定的任务
- 风险：编排者的规划质量决定整体效果
- 适用：复杂编码项目、研究任务

### 模式 5：Agentic Loop（自主循环）
Agent 在 [[agent-loop]] 中自主运行，持续推理-行动-观察直到任务完成。
- 优势：最大化 Agent 的自主性和适应性
- 风险：可能失控或陷入无效循环
- 适用：探索性任务、开放性研究

## Anthropic 多 Agent 协作架构（2026-04）

Anthropic 在官方博客总结了 5 种多 Agent **协作架构模式**（与行为模式正交）。这些模式解决的是 Agent 之间如何组织协作：

### 模式 A：生成器与验证器
最简单的多 Agent 模式，也是目前落地应用最多的架构。生成器产出初稿，验证器按标准检查，循环直到通过。
- 适用：对输出质量要求极高且评估标准可量化的场景（代码测试、事实核查、合规审查）
- 风险：验证器能力取决于标准定义，循环可能卡死需设最大迭代次数

### 模式 B：编排器与子智能体
层级制架构——一个 Agent 当组长，其他 Agent 各自领走特定任务。[[claude-code]] 用的就是这套架构：主 Agent 写代码，需要时在后台唤起子 Agent 进行代码库搜索和调查。
- 适用：任务拆解清晰且子任务之间几乎没有互相依赖
- 风险：组长容易变成信息瓶颈，排队干活拖慢速度

### 模式 C：智能体团队
多个 Agent 作为独立进程运行，从共享任务池接单。与编排器模式的区别在于工人的**持久性**——队友会一直在线，积累上下文和领域专长。
- 适用：完全独立且需要长期多步连贯操作的子任务
- 风险：绝对的独立性是死穴，资源冲突和完成判断困难

### 模式 D：消息总线
通过共享通信层发布/订阅事件。新 Agent 加入无需改底层代码。
- 适用：事件驱动流水线，Agent 生态系统不断膨胀
- 风险：调试困难，路由派发准确率是命门

### 模式 E：共享状态
所有 Agent 在持久化存储空间自由读写交流，无中央控制器。
- 适用：高度协作且需实时互通重大发现的研究型任务
- 风险：容易重复劳动或南辕北辙，无限接话死循环需强硬终止条件

### 选型建议

Anthropic **强烈建议从编排器与子智能体模式起步**，观察瓶颈后再向其他模式进化。工业级系统往往是混合流派——主干用编排器，局部切成共享状态；或消息总线做总分发，下游由团队模式攻坚。

> 核心原则：以上下文为中心的任务拆解——不是按 Agent 能干什么来分工，而是按它们需要什么上下文来分工。

## 相关设计模式

这些架构模式与 Agent 的行为模式是正交的——每种架构都可以搭配不同的行为模式：

- [[react-pattern]] — 边推理边行动的 Agent 行为模式
- [[plan-and-execute-pattern]] — 先规划后执行的 Agent 行为模式
- [[reflection-pattern]] — 自我反思改进的 Agent 行为模式
- [[spec-driven-development]] — 用规格文档协调多 Agent 的开发方法

## SDD 多 AI 协同实践（2026-04）

来自 binxiong 团队的跨境保险产品全流程交付实录，展示了 [[spec-driven-development]] 在多 AI 协同中的实战应用：

### 模型角色分工
- **Claude**（协调者）— 统筹全局，决定何时调用其他模型
- **Codex**（高级工程师）— 代码实现、调试、重构
- **Gemini**（长文本分析师）— 大上下文分析、日志分析、模式发现

### 协同机制
通过 [[mcp]] 协议将 Codex 和 Gemini 注入 [[claude-code]]，在 [[claude-md]] 中定义强制性协作规则：
- 工具调用是默认行为，不是可选项
- Claude 在每个关键节点自问："Codex/Gemini 能帮忙吗？"
- Gemini 默认为 read-only 分析师，所有实现仍由 Claude 完成

### 标准工作流（四步闭环）
1. **理解与规划** — Claude 澄清目标 → 调用 Codex 细化方案 → 需要时调用 Gemini 获取全局视图
2. **实现与运行** — 向 Codex 请求 unified diff 原型 → Claude 审查改进后应用
3. **审查与分析** — Codex 代码审查 + Gemini 日志分析 → 结论冲突时要求二者互相回应，Claude 仲裁
4. **撰写** — Gemini 总结法规要点 + Codex 校验代码与文档一致性

### 企业实践
使用 [[openspec]] 管理"提案→审查→实现→归档"全流程，配合 SubAgent 架构（系统架构专家 + 技术方案专家），实现从需求到交付的 AI 全流程自动化。

> 本质不是限制 AI，而是将人类置于关键决策点，让 AI 成为高效执行者，而非盲目代码生成器。
