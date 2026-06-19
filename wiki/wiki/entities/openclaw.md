---
title: OpenClaw
created: 2026-04-23
updated: 2026-05-30
type: entity
tags: [tool, agent, open-source]
sources:
  - openclaw-discord-ai-research-team
  - openclaw-xiaohongshu-sop
  - hermes-multi-agent-collaboration-guide
  - turix-cua-agent-skill
  - raw/articles/2026-04-18-openclaw-discord-ai-research-team.md
  - raw/articles/2026-04-18-openclaw-xiaohongshu-sop.md
  - raw/articles/2026-04-18-build-ai-agent-framework.md
  - raw/articles/2026-04-18-gsd2-auto-dev-tool.md
  - raw/articles/2026-05-29-openclawhermesai-agent.md
---

# OpenClaw

## Overview

OpenClaw (also known as Clawdbot) is an open-source multi-agent platform that serves as an alternative to [[claude-code]]. It supports Discord and WeChat integration, features a skill marketplace called CLAWHUB, and implements a sophisticated multi-agent architecture with five-layer memory and dual-track governance. OpenClaw appears in 8+ articles across the corpus, making it one of the most frequently mentioned open-source agent platforms.

## Core Architecture

### Multi-Agent System
OpenClaw enables multiple AI agents to work collaboratively with:

- **Role division**: Each agent has a defined role and specialization
- **Message routing**: Deterministic routing based on channel + account → agent mapping ([[openclaw]])
- **Shared memory**: Agents can access shared team memory for coordination
- **Session isolation**: Per-account-channel-peer isolation ([[openclaw]]) for multi-user privacy

### Five-Layer Memory Architecture
OpenClaw implements a sophisticated memory system with five layers:

1. **Daily logs** — Short-term conversation records from the current day
2. **Long-term memory** — Persistent individual agent memories (MEMORY.md)
3. **Group memory** — Shared team knowledge (GROUP_MEMORY.md)
4. **Cold archive** — Infrequently accessed historical data
5. **Semantic retrieval** — Vector-based search across all memory layers

This architecture enables agents to maintain context over extended periods while efficiently managing storage and retrieval.

### Dual-Track Governance
OpenClaw uses a two-track system for managing agent behavior:

- **Config track**: Hard constraints that cannot be overridden (security boundaries, access controls)
- **Rules track**: Soft guidance that agents follow but can adapt based on context (behavioral guidelines, preferred workflows)

This separation allows system administrators to enforce critical constraints while giving agents flexibility in how they accomplish tasks.

### Bindings Routing
Deterministic message routing that maps incoming messages to specific agents based on:

- Channel (which Discord channel or WeChat group)
- Account (which user account sent the message)
- Peer (conversation partner identity)

This ensures that the right agent handles each request without ambiguity.

## Key Features

### CLAWHUB Skill Marketplace
CLAWHUB is OpenClaw's skill marketplace and registry. It allows users to:

- Browse and install community-created skills
- Share custom skills with other users
- Manage skill versions and dependencies
- Discover skills for specific use cases

CLAWHUB follows the SKILL.md convention for skill definition, ensuring compatibility with the broader agent skills ecosystem.

### SOUL.md Personality Files
OpenClaw uses SOUL.md files to define agent personalities and identities. Each agent's SOUL.md specifies:

- Personality traits and communication style
- Role description and expertise areas
- Behavioral preferences and constraints
- Cultural context and language preferences

This allows customization of agent behavior without modifying core system code.

### Discord Integration
Discord is the primary platform for OpenClaw multi-agent collaboration:

- Multiple agents operate in different Discord channels
- Users interact with agents through direct messages or channel conversations
- Supports complex multi-agent research workflows
- Enables team collaboration with shared memory

### WeChat Integration
OpenClaw supports WeChat (微信) integration for the Chinese market:

- Operates within WeChat groups and direct messages
- Used for automated content production workflows (小红书 SOP)
- Supports Chinese-language interactions natively

## Use Cases

### AI Research Teams
OpenClaw excels at organizing multi-agent research teams (openclaw-discord-ai-research-team):

- **Pipeline-driven research**: Literature survey → Data preparation → Algorithm development → Paper writing
- **6-agent teams**: Example by @Saboo_Shubham_ who built a complete research team
- **Discord as workspace**: Each agent gets its own channel for focused work

### Xiaohongshu (小红书) Content Production
OpenClaw supports automated content production workflows for Xiaohongshu (Little Red Book):

- RPA-style automated data collection and analysis
- Multi-step content creation pipeline
- Integration with image generation models (Nano Banana)
- Cloud storage integration (Alibaba Cloud OSS)

### Computer Use Agent Integration
OpenClaw can integrate with Turix CUA ([[computer-use-agent]]) for desktop automation tasks.

## Comparison with Alternatives

| Feature | OpenClaw | Claude Code | Hermes Agent |
|---------|----------|-------------|-------------|
| Open source | Yes | No | Yes |
| Multi-agent | Yes (native) | Limited (subagents) | Yes (native) |
| Platform | Discord/WeChat | CLI | Discord |
| Skill system | CLAWHUB | SKILL.md | Skills |
| Memory | 5-layer architecture | Session-based | Profile-based |
| Governance | Dual-track | CLAUDE.md | Profile config |
| Personality | SOUL.md | N/A | SOUL.md |

## Key Relationships

- Alternative to [[claude-code]] as an AI coding/agent platform
- Related to [[hermes-agent]] — both support multi-agent collaboration
- Shares SOUL.md convention with Hermes
- Uses SKILL.md convention compatible with Anthropic's skill ecosystem
- Integrates with [[discord]] for multi-agent workspace
- Connects to [[turix-cua]] for desktop automation
- Part of the broader [[multi-agent-collaboration]] paradigm

## Sources

- openclaw-discord-ai-research-team — Multi-agent research team architecture
- openclaw-xiaohongshu-sop — Xiaohongshu content production workflow
- hermes-multi-agent-collaboration-guide — Multi-agent collaboration comparison
- turix-cua-agent-skill — Computer Use Agent integration
For a detailed comparison, see [[claude-code-vs-openclaw-vs-hermes]].

## 2026-05 源码架构深度剖析（腾讯技术工程）

基于腾讯技术工程对 OpenClaw 源码的深入分析（v2026.5.6），以下是 OpenClaw 架构的核心工程洞察：

### 设计哲学：Local-First + 万物皆插件

OpenClaw 不是云服务，是运行在用户设备上的 Gateway 进程。核心代码只负责编排（消息路由、会话管理、安全网关），所有具体能力以插件形式实现。五层架构：**触达层**（Channel Plugins）→ **编排层**（Gateway）→ **能力层**（Plugins/Skills）→ **记忆层**（向量引擎+Dreaming）→ **模型层**（9 种 LLM API 协议）。

### Gateway — 微内核中枢

Gateway 同时承担 5 大角色：
- **唯一长驻进程**：避免多进程下的 channel session 冲突
- **消息总线**：所有流量（聊天/控制/节点能力/心跳）统一走 Gateway `127.0.0.1:18789`
- **多 Agent 路由边界**：Multi-Agent Router 实现 Agent 间物理隔离（独立 workspace/SOUL/MEMORY/sessions）
- **认证+信任边界**：Challenge-Response + Ed25519 + Device Identity，写操作必须带 idempotency key
- **嵌入式 HTTP Host**：Agent 可主动构造 UI（canvas），不需单独起 web server

核心哲学：**边界 vs 实现** —— Gateway 只做「协议+路由+信任」，微内核保持几千行。

### Session Key — 消息路由核心

格式 `agent:{agentId}:{scope}`，将「谁在操作（Client）」和「哪条线路（Channel）」绑定。9 级路由优先级（peer > parent > wildcard > guild+roles > ... > default）。多 Agent 绑定让不同来源消息路由到不同 Agent，实现物理隔离。

### Channel Plugin — 25+ Adapter 完整契约

OpenClaw 的 Channel 不是简单消息适配器，而是完整的 IM 域协作单元。Channel 契约含 4 必选 + 30+ 可选适配器，覆盖 Setup、Auth/Security（7 项）、Messaging（7 项）、协作能力（7 项）、Gateway 绑定（6 项）、反向工具（agentTools）。

独门能力：
- **Per-channel Streaming Adapter**：LLM 流式输出的语义在每个 IM 协议中完全不同，Channel 封装这些差异
- **Channel Docking**：跨 Channel 会话迁移（「AI 会话的呼叫转移」）——验证 identityLinks 后保留 session 上下文不变，只换投递地址
- **精细化热重载**：只重启配置变更的 Channel，不重启整个 Gateway

### Auth Profile — 不止 API Key 数组

每个账号建模为带健康状态的对象（类型/状态/冷却原因）。失败时智能识别错误类型 → 标记冷却 → 毫秒级切换到备用 Profile → 自动探针恢复。对比 Hermes 的 Credential Pool：Hermes 只是 API Key 数组按顺序试，不区分错误类型，不知道上次哪个 key 挂了。

### Agent 执行引擎 — 三层架构

- **循环层**（pi-agent-core）：ReAct 循环、工具调用、流式输出——只负责循环本身
- **编排层**（pi-embedded-runner）：预算控制、Auth Profile failover、Compaction、Lane 分车道
- **拦截层**（Hooks）：beforeToolCall（安全审批）、afterToolCall（截断）、transformContext（压缩）

关键设计：AgentMessage ≠ LLM Message（内部用自定义消息类型，只在调 LLM 边界才转换）；StreamFn 可替换（可把 Claude Code stdio 当 LLM 响应）；双 Lane 排队（globalLane + sessionLane 防并发冲突）；七类分支顺序决定正确性（timeout compaction 必须在 overflow 之前）。

### 容错设计 — FailoverError 契约

`runEmbeddedPiAgent` 是「FailoverError 工厂」，`runWithModelFallback` 是「消费者」——两者只通过这一错误类型交流。Auth Controller 封装所有凭证复杂度，外层主循环只看到 4 个方法。可恢复错误的处理是静态可证明的，不靠 LLM 猜。

### 新来源

- `raw/articles/2026-05-29-openclawhermesai-agent.md` — 腾讯技术工程源码级架构剖析
