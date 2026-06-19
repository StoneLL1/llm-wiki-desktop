---
title: BrowserAct
created: 2026-06-04
updated: 2026-06-04
type: entity
sources:
  - raw/GitHub/browser-act-skills.md
  - raw/articles/2026-06-03-browseract-playwright-replacement.md
tags: [tool, agent, open-source, automation, security]
---

# BrowserAct

## Overview

BrowserAct is an open-source browser automation CLI built specifically for AI agents (1,573 ⭐, Python). It serves as a [[playwright]] alternative with a dedicated focus on anti-detection, captcha solving, session management, and human-AI collaboration. Unlike generic browser automation frameworks, BrowserAct is designed from the ground up for AI agent consumption — compact text output, indexed interactions (`click 3`), semantic memory, and concurrency safety.

**Repository**: [github.com/browser-act/skills](https://github.com/browser-act/skills) | **Created**: 2026-02-06

## Two Products

| Product | Purpose | Installation |
|---------|---------|--------------|
| `browser-act` | Real-time browser control CLI | `uv tool install browser-act-cli --python 3.12` |
| `browser-act-skill-forge` | Auto-discover website API endpoints and generate reusable [[skills]] | From repository |

## Core Capabilities

### Three Browser Modes

| Mode | Description | Use Case |
|------|-------------|----------|
| **Chrome** | Independent Chrome instance, reuses cookies/login state | Operate logged-in backends or social media |
| **Stealth** | Anti-detection browser, isolated fingerprint + proxy (requires API Key) | Bypass anti-bot protection, multi-account parallel scraping |
| **Chrome-Direct** | CDP direct connection to currently running Chrome | Fast debugging, human-AI collaboration |

### Three-Tier Anti-Detection

BrowserAct employs a layered defense against bot detection:

1. **Environment Layer** — Custom Chromium build with automation traces removed, independent browser fingerprint per session, dynamic proxy rotation, session isolation
2. **Execution Layer** — Built-in `solve-captcha` for Cloudflare/reCAPTCHA/Datadome; `stealth-extract` for JS-rendered content from protected pages
3. **Human Interaction Layer** — `remote-assist` generates a remote link so a human can handle phone verification / QR code login via mobile, after which the agent continues the original session

### Parallel Multi-Task Execution

- Cross-browser parallelism with independent cookies, fingerprints, and proxies
- Same-browser multi-session with shared login state but isolated execution
- Auto-strips ~90% invalid HTML (ads, tracking, framework noise), reducing token costs and improving LLM output quality

### Agent Compatibility

Works with: [[claude-code]], Cursor, VS Code, OpenCode, [[openclaw]], OpenAI Codex, Gemini CLI. Cross-platform: Windows / macOS / Linux.

## Skill Forge

`browser-act-skill-forge` can encapsulate any website's operational capabilities into a reusable [[skills|Skill]]:

- Automatically discovers API endpoints and request patterns behind websites
- Generates complete `SKILL.md` + Python script package after exploration
- Trial-and-error learnings persist — subsequent runs follow the optimal path
- Explore once, reuse at scale

## Pre-Built Skill Ecosystem (31 Skills)

| Category | Count | Examples |
|----------|-------|----------|
| E-commerce | 8 | Amazon ASIN query, hot products, Buy Box monitoring, competitor analysis, review scraping |
| Lead Generation | 7 | Business contacts, GitHub contributor finder, Google Maps search, industry influencer radar |
| Search & Research | 4 | Google image search, Google News, web research assistant, web search scraping |
| Social Monitoring | 3 | Reddit competitor analysis, WeChat Official Account search, Zhihu search |
| Video Platforms | 9 | YouTube search, channel analysis, comment extraction, transcript extraction, KOL discovery |

## Tech Stack

- Python 3.12+
- Playwright (browser engine)
- Camoufox (stealth anti-detection)
- Typer (CLI framework)

## See Also

- [[skills]] — Modular agent capability system that BrowserAct Skills integrate with
- [[mcp]] — Model Context Protocol for tool integration
- [[agent-browser]] — Vercel Labs' AI browser control tool (26K ⭐, 82% token savings vs Playwright MCP)
- [[browser-use]] — Open-source browser automation AI agent framework
- [[claude-code]] — Primary AI coding agent that BrowserAct is compatible with
