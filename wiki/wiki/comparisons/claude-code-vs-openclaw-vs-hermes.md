---
title: "Claude Code vs OpenClaw vs Hermes Agent"
created: 2026-04-23
updated: 2026-04-23
type: comparison
tags: [tool, agent, comparison, open-source]
sources:
  - raw/articles/openclaw-discord-ai-research-team.md
  - raw/articles/hermes-multi-agent-collaboration-guide.md
  - raw/articles/claude-code-10-more-worthwhile-skills.md
  - raw/articles/claude-code-creator-15-hidden-features.md
  - raw/articles/hermes-agent-lobster-hermes.md
  - raw/articles/turix-cua-agent-skill.md
---

# Claude Code vs OpenClaw vs Hermes Agent

> 三大 AI Agent 平台的横向对比。三者定位不同但存在竞争关系。

## 对比维度

| 维度 | [[claude-code]] | [[openclaw]] | [[hermes-agent]] |
|------|----------------|-------------|-----------------|
| **开发者** | [[anthropic]] | 社区开源 | [[nousresearch]] |
| **开源** | ❌ 闭源（CLI 工具） | ✅ 开源 | ✅ 开源 |
| **核心模型** | Claude Opus/Sonnet | 多模型（可配置） | 多模型（可配置） |
| **平台** | 终端 CLI | Discord/微信/Telegram | Discord/飞书/Telegram |
| **Skill 系统** | SKILL.md + CLAUDE.md | CLAWHUB 技能市场 | Skills + Profiles |
| **多 Agent** | Subagent 模式 | 原生多 Agent | Agent Profile 隔离 |
| **记忆系统** | CLAUDE.md + Memory | 五层记忆架构 | Profile 独立记忆 |
| **MCP 支持** | ✅ 原生支持 | ✅ 支持 | ✅ 支持 |
| **上下文窗口** | 1M tokens | 取决于模型 | 取决于模型 |
| **适用场景** | 编程开发 | 自动化工作流 | 通用 Agent + 编程 |
| **中文生态** | ⚠️ 一般 | ✅ 微信/小红书 | ✅ 飞书社区 |

## 核心差异

### Claude Code：专业编程 Agent
- 最强编程能力，Anthropic 官方支持
- 1M 上下文，Plan Mode，subagent
- 闭源，仅终端使用
- 社区 Skills 生态（everything-claude-code 13.2w star）

### OpenClaw：自动化工作流平台
- 原生多 Agent 协作，支持 Discord/微信
- CLAWHUB 技能市场，社区驱动
- 五层记忆架构，双轨治理
- 适合小红书自动发布、科研团队等场景

### Hermes Agent：通用 Agent 框架
- Process-isolated Agent Profiles
- 多平台网关（Discord/飞书/Telegram）
- 记性好，Skills 可自我进化
- NousResearch 维护，开源社区活跃

## 选择建议

- **纯编程开发** → Claude Code
- **自动化工作流（微信/小红书）** → OpenClaw
- **通用 Agent + 中文社区** → Hermes Agent
- **多 Agent 科研团队** → OpenClaw 或 Hermes Agent

## 参见

- [[claude-code]] — Claude Code 详细页面
- [[openclaw]] — OpenClaw 详细页面
- [[hermes-agent]] — Hermes Agent 详细页面
- [[multi-agent-collaboration]] — 多 Agent 协作模式
- [[skills]] — Agent Skills 体系
