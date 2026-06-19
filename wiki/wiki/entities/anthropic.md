---
title: Anthropic
created: 2026-04-23
updated: 2026-06-08
type: entity
tags: [company, open-source]
sources:
  - claude-code-10-more-worthwhile-skills
  - claude-code-creator-15-hidden-features
  - claude-design-impact-on-ai-design-vendors
  - claude-design-system-prompt-bilingual
  - claude-design-system-prompt-leak-analysis
  - claude-code-1m-context-management-guide
  - 10-claude-code-best-practices
  - raw/articles/2026-05-14-anthropic-financial-skills.md
  - raw/articles/2026-04-18-claude-code-creator-15-hidden-features.md
  - raw/articles/2026-04-18-claude-code-session-management.md
  - raw/articles/2026-05-07-effective-harnesses-for-long-running-agents.md
  - raw/articles/2026-05-07-anthropic-harness-guide-dead-weight.md
  - raw/articles/2026-05-29-claude-opus-48-dynamic-workflow一次性并行上百个subagents.md
  - raw/articles/2026-06-05-anthropic-95-percent-data-analytics-claude.md
  - raw/articles/2026-06-07-anthropic-internal-skills-practices.md
---

# Anthropic

## Overview

Anthropic is an AI safety company and the creator of the Claude family of large language models, as well as the Claude Code CLI agent, Claude Design conversational design tool, and Claude Cowork desktop agent. Founded with a focus on AI safety research, Anthropic has become one of the leading AI labs alongside OpenAI and Google DeepMind. The company's products are referenced across 12+ articles in this wiki, making it one of the most frequently mentioned entities in the corpus.

## Key Products

### Claude Model Family
Anthropic's flagship LLM family powers all of its products. The family includes:

- **Claude Opus 4.6** — 1M token context window, capable of 14.5-hour sustained task completion
- **Claude Opus 4.7** — Latest model with the strongest vision capabilities, supports `xhigh` effort level
- **Claude Sonnet** — Efficient model for routine tasks

See [[claude-model-family]] for detailed model specifications.

### Claude Code
Anthropic's CLI-based coding agent, created by [[boris-cherny]]. Claude Code is the most frequently mentioned tool in the corpus (25+ articles). It supports Skills, MCP integration, Plan Mode, subagents, and the CLAUDE.md project configuration convention. Core developer **Thariq Shihipar** leads session management and context optimization research. See [[claude-code]] for comprehensive documentation.

### Claude Design
A conversational design tool that generates prototypes, PPTs, and presentation decks. Powered by Claude Opus 4.7, Claude Design features the Tweaks Protocol for interactive adjustments, 37 integrated tools, and 13 built-in skills. Its system prompt was leaked by security researcher Pliny the Liberator. See [[claude-design]] for details.

### Claude Cowork
Anthropic's desktop-level agent that operates directly on filesystems, extending Claude's capabilities beyond the CLI.

### Claude Code Channels
Integration layer connecting Claude Code to messaging platforms like Telegram and Discord for remote control and collaboration.

### Claude Memory
Anthropic's persistent memory feature that allows Claude to maintain context across sessions.

## AI Safety Focus

Anthropic positions itself as an AI safety-first company. Its research includes:

- Constitutional AI (CAI) methods for alignment
- Responsible scaling policies
- Interpretability research
- The Model Context Protocol ([[mcp]]) as a standardized way to connect AI models to external tools and data sources with security considerations

## Open Source Contributions

Anthropic has contributed several open-source tools and conventions to the AI developer ecosystem:

- **MCP (Model Context Protocol)** — Open protocol for connecting AI models to external tools and data sources
- **CLAUDE.md** convention — Project-level configuration file standard for AI coding agents
- **SKILL.md** convention — Modular skill definition file format
- **Anthropic Skill Best Practices** — Published guidelines for creating effective agent skills

## Impact on the Industry

Anthropic's products have had significant disruptive effects on multiple industries:

- **Design tools**: Claude Design threatens established players like [[figma]], Adobe, Canva, and Wix
- **Code editors**: Claude Code competes with [[cursor]] and VS Code Copilot
- **Agent platforms**: Claude Code and Claude Cowork challenge open-source alternatives like [[openclaw]] and [[hermes-agent]]
- **Research workflows**: Claude models power automated research, paper writing, and knowledge compilation workflows

## 内部数据分析自动化实践（2026-06）

Anthropic 数据科学团队在 2026-06 发布博客：内部 95% 的业务数据分析查询已由 Claude 自动完成，准确率约 95%^[raw/articles/2026-06-05-anthropic-95-percent-data-analytics-claude.md]。

### 三种 AI 分析模式

| 模式 | 复杂度 | 适用场景 | 覆盖率 |
|------|--------|----------|--------|
| 自动模式 | 低 | 简单直接的数据查询，用户自然语言描述，系统自动选择方法 | ~60% |
| 指导模式 | 中 | AI 先展示分析计划，用户确认后执行，兼顾自动化和可控性 | ~35% |
| 人工模式 | 高 | 复杂、敏感或需深度领域知识的分析，AI 提供辅助人工主导 | ~5% |

### 技术架构三件套

1. **Text-to-SQL**：用户自然语言→SQL 查询，关键技术为 Schema 描述、示例查询、查询验证（Claude 先检查 SQL 正确性）
2. **RAG（检索增强生成）**：知识库存储历史分析报告、业务术语表、数据字典，语义检索提供背景信息
3. **自定义 Tool Use**：封装复用分析逻辑（漏斗分析、留存分析等），标准化输出，权限控制

### Skill 文件：核心精髓

Anthropic 强调 Skill 文件是数据分析自动化的关键——将资深分析师的领域知识沉淀为结构化文本：

| Skill 文件内容 | Anthropic 做法 | [[skills]] 对应 |
|----------------|---------------|-----------------|
| 上下文信息 | 数据库概况、表用途、业务指标定义 | skill 中的 context |
| 编码规范 | SQL 编写风格、命名规范、性能优化 | skill 中的 conventions |
| 质量标准 | 查询验证方法、错误类型检查、格式要求 | skill 中的 pitfalls |
| 工具封装 | 常用分析流程封装为可复用工具 | skill + tool 调用 |

关键经验：**领域知识比技术能力更重要**。渐进实施路径：简单查询→收集错误案例→补充 Skill 文件→扩展到复杂查询。安全措施包括数据脱敏、权限管理、审计日志。

数据团队因此能专注做因果建模、预测和机器学习等更高价值的工作。

## Key Relationships

- Creator of [[claude-code]], [[claude-design]], [[claude-model-family]]
- Creator of [[mcp]] (Model Context Protocol)
- [[boris-cherny]] is the creator of Claude Code
- [[lance-martin|Lance Martin]]
- **Thariq Shihipar** is a core developer of Claude Code (session management, context optimization)
- Competes with OpenAI (GPT/Codex), Google (Gemini/Stitch), and open-source alternatives
- Referenced by Chinese tech bloggers including 鲁工 (AI编程实验室)

## Sources

- Anthropic appears in 12+ articles across the corpus
- Key source articles: claude-code-10-more-worthwhile-skills, claude-code-creator-15-hidden-features, claude-design-impact-on-ai-design-vendors, claude-design-system-prompt-bilingual, claude-design-system-prompt-leak-analysis, claude-code-1m-context-management-guide, 10-claude-code-best-practices

## 2026-05 新产品与发布

### Claude Managed Agents
Anthropic 推出 Managed Agents 托管基础设施，将 Agent 从开发工具扩展为生产级服务。支持无头部署、定时触发和多 Agent 编排，标志着 Anthropic 从模型提供商向 Agent 平台的转型。

### claude-for-financial-services 金融 Skills 开源
Anthropic 开源了面向金融行业的 10 个预构建 Agent（Pitch Agent, Model Builder, KYC Screener 等），展示了垂直行业 Agent 的标准化方案。详见 [[vertical-industry-agents]]。

### Code with Claude 活动（2026-05-06）
Anthropic 举办的大型开发者活动，聚焦两大主题：
- **多 Agent 编排**：Agent Teams、Claude Agent SDK 的实战演示
- **Dreaming**：AI Agent 的自主探索和创意生成能力

### Harness Engineering 方法论推广
Anthropic 正式推广 [[harness-engineering]] 作为 Agent 开发的核心方法论，强调通过结构化 Harness 设计实现可控的自主 Agent 工作流。Lance Martin 发表了 Harness 博客，系统阐述了这一方法论的理论基础和实践指南。

### Claude Opus 4.8（2026-05-29）
2026-05-29 发布的最新旗舰模型。诚实度（honesty）提升为核心亮点——代码缺陷「放过不提」概率比 4.7 降低 4×。同步推出 [[claude-code-dynamic-workflow]] 动态工作流功能（研究预览）。SWE-Bench Pro 69.2% 领先所有竞品。配套 244 页 System Card。预告即将发布 Mythos 模型。

详见 [[claude-opus-48]] 和 [[claude-code-dynamic-workflow]]。

### Mythos 预告
Anthropic 在 Opus 4.8 发布结尾预告，未来几周将发布此前仅少部分组织可用的「地表最强模型」Mythos。
