---
title: Claude Model Family
created: 2026-04-23
updated: 2026-05-30
type: entity
tags: [model, architecture]
sources:
  - claude-code-1m-context-management-guide
  - claude-design-impact-on-ai-design-vendors
  - claude-design-system-prompt-bilingual
  - claude-design-system-prompt-leak-analysis
  - claude-code-10-more-worthwhile-skills
  - raw/articles/2026-05-29-claude-opus-48-dynamic-workflow一次性并行上百个subagents.md
---

# Claude Model Family

## Overview

The Claude model family is [[anthropic]]'s series of large language models that power Claude Code, Claude Design, Claude Cowork, and the Claude conversational AI. The family is one of the most referenced AI models in this wiki's corpus, appearing in 30+ articles. Claude models are known for their large context windows, strong coding capabilities, and vision understanding.

## Model Variants

### Claude Opus 4.6
Anthropic's model with a 1M (one million) token context window. Key characteristics:

- **1M token context**: Can process approximately 750,000 words in a single session, enabling very long sustained tasks
- **14.5-hour task completion**: Demonstrated ability to maintain coherent work over extremely long task durations
- **Strong coding capabilities**: Powers Claude Code for complex software engineering tasks
- **Context management**: Requires careful [[context-engineering]] to avoid context rot at scale

Claude Opus 4.6 is referenced in claude-code-1m-context-management-guide and claude-design-system-prompt-leak-analysis.

### Claude Opus 4.7
The previous flagship model before Opus 4.8. Key advancements over Opus 4.6:

- **Strongest vision capabilities**: Best-in-class visual understanding for multimodal tasks
- **xhigh effort level**: New maximum effort setting for tasks requiring the highest quality output
- **Design generation**: Powers Claude Design's prototype and deck generation
- **Enhanced reasoning**: Improved performance on complex reasoning and planning tasks

### Claude Opus 4.8
Released 2026-05-29, the current flagship model. Key advancements over Opus 4.7:

- **Honesty-first training**: Code defect omission rate reduced 4× vs 4.7 — model proactively flags uncertainty instead of faking confidence
- **SWE-Bench Pro 69.2%** — leads all competitors in coding benchmarks (vs 4.7: 64.3%, GPT-5.5: 58.6%)
- **Dynamic Workflow native support**: Powers 100+ parallel subagent orchestration via [[claude-code-dynamic-workflow]]
- **244-page System Card**: Comprehensive safety evaluation
- **Mythos teaser**: Anthropic announced upcoming release of "strongest model on earth" Mythos

See [[claude-opus-48]] for detailed benchmark comparisons and honesty case studies.

### Claude Sonnet
The efficient variant in the Claude family, designed for routine tasks where maximum capability is not required. Sonnet offers:

- Faster response times
- Lower computational cost
- Strong performance on standard coding and writing tasks
- Suitable for high-volume, lower-complexity workloads

## Context Window Evolution

The Claude model family has progressively expanded its context window capabilities:

| Model | Context Window | Key Capability |
|-------|---------------|----------------|
| Earlier Claude | 200K tokens | Standard long-context tasks |
| Claude Opus 4.6 | 1M tokens | 14.5-hour sustained tasks |
| Claude Opus 4.7 | 1M+ tokens | Enhanced vision + xhigh effort |
| Claude Opus 4.8 | 1M+ tokens | Honesty-first + Dynamic Workflow |

The expansion to 1M context windows has fundamental implications for how developers interact with Claude models:

- **Longer tasks**: Single sessions can span entire development workflows
- **More context**: Entire codebases can be loaded into context
- **Context rot risk**: Longer contexts require more aggressive [[context-engineering]]
- **Cost management**: Token economics become more important at scale

## Products Powered by Claude Models

### Claude Code
[[claude-code]] is primarily powered by Claude Opus models. The 1M context window enables:
- Full codebase analysis
- Multi-file refactoring
- Extended research sessions
- Complex debugging workflows

### Claude Design
[[claude-design]] uses Claude Opus 4.7 for:
- Prototype generation from natural language
- Presentation deck creation
- Visual design with strong vision understanding
- Interactive adjustments via Tweaks Protocol

### Claude Cowork
Anthropic's desktop-level agent that operates on filesystems, powered by Claude models for file management and system interaction tasks.

## Effort Levels

Claude models support multiple effort levels that control the trade-off between output quality and computational cost:

- **Standard**: Default quality for routine tasks
- **High**: Enhanced reasoning and output quality
- **xhigh**: Maximum quality output (Opus 4.7 only); used for design generation and complex reasoning tasks where quality is paramount

## Comparison with Other Model Families

| Family | Creator | Context | Key Strength |
|--------|---------|---------|-------------|
| Claude (Opus 4.7) | Anthropic | 1M+ tokens | Vision, coding, long tasks |
| GPT-4/4o | OpenAI | 128K-1M tokens | General reasoning |
| Gemini 3.1 | Google | 1M+ tokens | Multimodal, Google ecosystem |
| Llama | Meta | 128K tokens | Open-source, local deployment |
| Gemma 3 | Google | Open-source | Open-source, efficient |

## Research Applications

Claude models are widely used in research and academic workflows:

- **Paper writing**: LaTeX paper generation with Overleaf
- **Deep research**: Comprehensive research methodology (claude-research-10x-better)
- **Knowledge compilation**: Structured knowledge base creation ([[knowledge-compilation]])
- **Scientific analysis**: Econometrics, data analysis, literature surveys

## Key Relationships

- Created by [[anthropic]]
- Powers [[claude-code]], [[claude-design]]
- Core to [[context-engineering]] practices
- Compared with GPT, Gemini, and Llama model families
- Used with [[mcp]] for tool integration

## Sources

- claude-code-1m-context-management-guide — Context window management with Opus 4.6
- claude-design-impact-on-ai-design-vendors — Opus 4.7 capabilities in design
- claude-design-system-prompt-bilingual — Model capabilities revealed through system prompt analysis
- claude-design-system-prompt-leak-analysis — Technical specifications from prompt leak
- claude-code-10-more-worthwhile-skills — Claude model capabilities in coding contexts

## 2026-05 模型性能更新

### 新增性能数据

| 模型 | 基准测试 | 得分 | 说明 |
|------|----------|------|------|
| **Opus 4.5** | SWE-bench | **80.9%** | 软件工程基准测试最高分，展示卓越的代码理解和修复能力 |
| **Opus 4.6** | BrowseComp | **84%** | 浏览器自动化任务基准测试，验证长程规划和工具使用能力 |
| **Sonnet 4.5** | BrowseComp | **43%** | 效率型模型在浏览任务上的表现，约为 Opus 4.6 的一半 |

### 性能对比分析

Opus 系列在复杂推理和长程任务上持续领先：
- **SWE-bench 80.9%**（Opus 4.5）意味着在真实世界软件工程问题中，Opus 已接近高级工程师的独立解决能力
- **BrowseComp 84%**（Opus 4.6）验证了 1M 上下文窗口在多步骤浏览任务中的实际价值
- Sonnet 系列作为效率选项，在常规任务中提供性价比更优的选择

这些数据进一步强化了 [[anthropic]] 在 Agent 驱动的软件开发领域的领先地位。
Google's [[gemma-4]] is a notable open-source multimodal competitor.
