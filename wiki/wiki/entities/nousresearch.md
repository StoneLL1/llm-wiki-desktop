---
title: NousResearch
created: 2026-04-23
updated: 2026-04-23
type: entity
tags: [lab, open-source]
sources: [hermes-agent-chinese-community-feishu, hermes-agent-lobster-hermes, hermes-multi-agent-collaboration-guide, github-top10-weekly-stars]
---

# NousResearch

## Overview

**NousResearch** is an open-source AI research lab and the creator of **Hermes Agent**. The lab gained significant attention in April 2026 when the Hermes Agent repository surged to **51K+ GitHub stars** in a single week, making it the fastest-growing open-source AI agent project at the time.

GitHub: [NousResearch/hermes-agent](https://github.com/NousResearch/hermes-agent)

## Hermes Agent

NousResearch's flagship product is [[hermes-agent]], an open-source AI agent designed as a self-improving personal AI companion. Key differentiators from competitors like [[openclaw]]:

### Self-Evolving Skills
Hermes Agent treats skills as **procedural memory** — experience artifacts that evolve through use, not static plugin configurations. The agent actively:
- Scans available skills before each task
- Saves new skills after complex task completion
- Patches skills when they're found to be outdated
- Converts learned methods into persistent reusable capabilities

### Profile-Based Multi-Agent
Hermes implements **process-isolated agent profiles** for multi-agent collaboration:
- Each profile has independent config.yaml, .env, SOUL.md, memory, skills, and gateway process
- True process-level isolation — if one agent crashes, others continue unaffected
- Clone strategies: blank, `--clone` (shared config, clean state), `--clone-all` (full clone including memory)

### Memory Architecture
Three-layer integration:
- **memory**: Records stable facts and user preferences
- **session_search**: Recalls past session experiences
- **skills**: Solidifies reusable execution methodologies

Chain: remember facts → recall experiences →固化 processes.

### Gateway Integration
Supports multiple messaging platforms:
- Discord (primary, with full @-mention support for multi-agent)
- Feishu (飞书) CLI
- DingTalk (钉钉) CLI
- WeChat enterprise
- Terminal (local)

## Community Reception

The Chinese AI community has enthusiastically adopted Hermes Agent:

- A dedicated **Hermes Agent Chinese Community** Feishu group was established
- Users report significantly better stability than OpenClaw: "8 hours daily with OpenClaw, 4 hours fixing bugs; Hermes Agent is a comrade you can trust with your back"
- The epub2podcast skill conversion project succeeded on Hermes but completely failed on OpenClaw
- Multi-agent collaboration via Discord has been extensively documented by practitioners like 林月半子

## Hermes LLM Series

NousResearch is also known for the **Hermes LLM** series — open-source models notable for being "uncensored" (minimal alignment filtering). These models are popular in the open-source community for research and personal use.

## Installation

```bash
curl -fsSL https://raw.githubusercontent.com/NousResearch/hermes-agent/main/scripts/install.sh | bash
source ~/.bashrc    # reload shell
hermes              # start chatting!
```

Gateway setup for messaging platforms:
```bash
hermes gateway setup    # trigger channel addition
hermes gateway install  # register as system service
hermes gateway start    # start the messaging gateway
```

## Key Metrics

- **51K+ stars** in a single week (fastest-growing AI agent project)
- Compatible with Claude Code, Codex, and other agent frameworks
- Skills ecosystem with bundled 77+ skills
- Active Chinese community with Feishu group

## Relationships

- Creator of [[hermes-agent]]
- Competitor/alternative to [[openclaw]]
- Integrates with [[computer-use-agent|Turix CUA]] as a skill
- Part of the [[skills]] ecosystem
- Related to [[anthropic]]'s Claude Code skills paradigm

## See Also

- [[hermes-agent]] — NousResearch's flagship AI agent
- [[openclaw]] — competing multi-agent platform
- [[skills]] — the modular capability framework Hermes extends
- [[computer-use-agent]] — CUA paradigm integrable with Hermes
