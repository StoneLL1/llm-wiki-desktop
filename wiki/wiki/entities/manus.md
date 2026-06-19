---
title: Manus
created: 2026-05-21
updated: 2026-05-21
type: entity
tags: [tool, agent, company]
sources:
  - raw/articles/2026-04-18-build-ai-agent-framework.md
---

# Manus

## Overview

Manus 是 AI 初创公司 Monica 发布的 Agent C 端产品，2025 年初爆火出圈。它的出现让 Agent 产品进入大众视野，同时在 Agent 人机交互和工程实践方面产生了深远影响。

## 关键事件

### 产品爆火
Manus 模糊地勾勒出了 Agent 应用的人机交互雏形。如同键盘鼠标的出现和第一代 iPhone 的发布，Manus 代表了人机交互方式的变革性探索。

### "Actually, Manus doesn't use MCP"
Manus 首席科学家 Peak 在社交媒体直接表示 Manus 不使用 [[mcp]]，而是受 [[codeact]] 启发。这一表态引发了关于 Agent 工具调用方式的广泛讨论。

### 上下文工程博客（2025年7月）
Manus 工程博客发表《AI Agent 的上下文工程：构建 Manus 的经验教训》，分享关键决策：
- **放弃微调路线**，选择基于通用大模型深耕 [[context-engineering]]
- **使用文件系统作为上下文**——后来 Anthropic 的 Claude Skills 也采用了这一理念

## 对行业的影响

Manus 的实践确立了当前 Agent 工程的两大业内共识：

1. **使用文件系统作为上下文**（如 [[openclaw]] 的 SOUL.md/TOOLS.md/MEMORY.md 等）
2. **编程是解决通用问题的普适方法**（[[codeact]] 模式：问题→生成代码→执行代码→迭代）

3 个月后，Anthropic 在 2025 年 10 月推出 Claude Skills，"使用文件系统作为上下文"的理念开始深入人心。

## See Also

- [[codeact]] — Manus 采用的代码驱动执行模式
- [[context-engineering]] — Manus 的核心技术路线
- [[claude-code]] — 同样践行文件系统作为上下文的理念
- [[openclaw]] — 采用 SOUL.md 等文件系统方案的 Agent 平台
- [[multi-agent-collaboration]] — Agent 协作模式
