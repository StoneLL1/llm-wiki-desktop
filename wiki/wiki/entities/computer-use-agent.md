---
title: Computer Use Agent (CUA)
created: 2026-04-23
updated: 2026-05-22
type: entity
tags:
  - agent
  - tool
  - open-source
sources:
  - raw/articles/2026-04-18-github-open-source-control-computer-skill.md
  - raw/articles/2026-04-21-turix-cua-agent-skill.md
---

# Computer Use Agent (CUA)

## Overview

A **Computer Use Agent (CUA)** is an AI agent paradigm where AI operates desktop applications via screen recognition and mouse/keyboard simulation. If the LLM is the AI's brain, CUA is its hands and eyes — it visually recognizes screen content, then simulates human mouse clicks and keyboard input to control computers.

CUA represents the most universal approach to app automation because it works with **any application** regardless of whether it provides APIs or CLI access — critical in ecosystems (especially China's) where most apps are closed platforms.

## Key Implementations

### Turix CUA

**Turix CUA** is the leading open-source CUA implementation, with 2.3K+ GitHub stars. It offers both a desktop application and a modular skill that can be integrated into any agent framework.

GitHub: [TurixAI/TuriX-CUA](https://github.com/TurixAI/TuriX-CUA)

#### Architecture: turix-brain + turix-actor

Turix has developed custom models specifically fine-tuned/trained for GUI operations:

- **turix-brain**: Visual understanding model — recognizes screen elements, understands UI context, identifies clickable regions
- **turix-actor**: GUI operations model — plans and executes mouse movements, clicks, keyboard input sequences

The official combination of turix-brain + turix-actor provides optimal performance. Users can also configure custom vision model APIs (providing API key, model name, and base URL).

### Mano-P

**Mano-P** 是明略科技（Mininglamp AI）开源的纯视觉驱动 CUA，特点是**本地隐私优先**和 **Think → Act → Verify 闭环推理**。

GitHub: [Mininglamp-AI/Mano-P](https://github.com/Mininglamp-AI/Mano-P)

- OSWorld 专项模型榜单排名第一（72B 模型成功率 58.2%）
- 数据完全不出设备，断网也能跑
- 4B 量化模型仅需 4.3GB 内存，可在普通 M4 MacBook 上运行
- 可通过 mano-skill 集成到 [[claude-code]]、[[openclaw]] 等 Agent 中

详见：[[mano-p]]

## Desktop App vs. Skill Integration

**Desktop version** (turix.ai):
- Install-and-use, optimized GUI
- Work mode + Chat mode
- Better safety: confirmation dialogs for file deletion, email sending, etc.
- Faster execution speed

**Skill integration**: Can be mounted as a sub-agent skill in [[hermes-agent]], [[openclaw]], [[claude-code]], or [[openai-codex|Codex]]. When integrated, it acts as a dedicated "hands-on assistant" running under the main agent.

## Capabilities

Turix CUA has been demonstrated controlling:
- **WeChat** (微信): Auto-approving friend requests, querying WeChat Index, automated conversations
- **QQ Music**: Navigating charts, playing specific songs
- **Web browsers**: GitHub issue filing, form filling
- **DingTalk** (钉钉): Enterprise collaboration tasks
- Any desktop application with a GUI

### Operational Characteristics

- Speed: approximately 50% slower than an experienced human operator — considered very fast for CUA
- No account ban risk: operates by simulating normal mouse clicks and swipes, not protocol hacking
- Experience accumulation: Once a task succeeds once, the workflow can be saved as a reusable skill

## Comparison with Codex CUA

OpenAI's Codex desktop also added CUA capabilities:

| Dimension | Turix CUA | Codex CUA |
|-----------|-----------|-----------|
| Open source | Fully open | Closed source |
| Modularity | Can be embedded in any agent | Standalone only |
| Model flexibility | Customizable vision model backend | Fixed |
| Execution speed | Faster (desktop) | Slightly slower |
| Mouse behavior | Takes over system cursor | Generates virtual cursor |
| Chinese app support | Good | Struggles with Chinese input |

## Industry Significance

CUA is positioned to disrupt the traditional RPA (Robotic Process Automation) industry. With RPA, developers write complex scraping scripts that break when web pages change. With CUA, users give a natural language instruction once; if the agent succeeds, the workflow is saved as a persistent skill for future faster, more reliable execution.

As CLI adoption by app developers remains slow (only Feishu, DingTalk, etc. provide CLI), CUA is the most universally applicable app automation solution for the foreseeable future.

## Known Limitations

- Number sensitivity: Turix may not strictly follow numeric constraints in instructions (e.g., "chat for 5 rounds then stop")
- Mouse takeover: Desktop version grabs the system mouse, preventing concurrent manual use

## Relationships

- Key implementation: [[turix-cua]] by TurixAI
- Integrates with [[hermes-agent]], [[openclaw]], [[claude-code]]
- Related to [[mcp]] as an alternative connectivity paradigm (API-first vs. vision-first)
- Part of the [[hermes-agent]] tool ecosystem

## See Also

- [[turix-cua]] — the leading CUA implementation
- [[mano-p]] — 纯视觉驱动、本地隐私优先的 CUA 实现
- [[mcp]] — Model Context Protocol (API-based alternative)
- [[hermes-agent]] — agent framework supporting CUA integration
- [[agent-building-tutorial]] — Agent 构建实战方法论
Related: [[openai-codex]] also supports sandbox execution.
