---
title: "Open Code Review"
url: "https://github.com/alibaba/open-code-review"
source: "GitHub"
fetched: 2026-06-06
stars: 3144
forks: 142
language: "Go"
license: "Apache-2.0"
topics: [agent, code-review, code-review-assistant, harness, repository-level-context]
homepage: "https://alibaba.github.io/open-code-review/"
---

# Open Code Review (OCR)

> 阿里巴巴开源的 AI 代码审查 CLI 工具，混合架构：确定性工程管道 + LLM Agent，行级精确评论，内置 NPE/线程安全/XSS/SQL 注入规则集，兼容 OpenAI 和 Anthropic

**⭐ 3,144 stars | 🍴 142 forks | 📅 创建于 2026-05-18 | 🔗 Apache-2.0**

## 概述

Open Code Review 起源于阿里巴巴集团内部官方 AI 代码审查助手，经过两年大规模生产验证，服务数万名开发者，累计发现数百万代码缺陷。它读取 Git diff，将变更文件发送给可配置的 LLM Agent（具备工具调用能力），生成行级精确的结构化审查评论。Agent 能读取完整文件内容、搜索代码库、检查其他变更文件获取上下文，产生深度审查而非表层 diff 反馈。

## 核心设计：确定性工程 × Agent 混合

### 纯 Agent 的痛点

- **覆盖不完整** — 大变更集时 Agent "偷懒"，选择性审查部分文件
- **位置漂移** — 报告的问题行号/文件引用与实际代码位置不匹配
- **质量不稳定** — 自然语言驱动的 Skill 难以调试，审查质量随 prompt 微小变化大幅波动

根因：纯语言驱动架构缺乏审查过程的硬约束。

### 确定性工程（硬约束）

由工程逻辑（非语言模型）保证审查关键步骤的正确性：

- **精确文件选择** — 精确判断哪些文件需要审查、哪些应过滤，确保不遗漏重要变更
- **智能文件捆绑** — 将相关文件分组为单个审查单元（如 `message_en.properties` + `message_zh.properties`），每个捆绑作为独立子 agent 运行（分治策略，天然支持并发）
- **细粒度规则匹配** — 基于模板引擎的规则匹配，按文件特征匹配审查规则，比语言驱动更稳定可预测
- **外部定位与反思模块** — 独立的评论定位和评论反思模块，系统性提升 AI 反馈的位置准确度和内容准确度

### Agent（动态决策）

将 Agent 优势集中在最有价值的动态决策和上下文检索：

- **场景调优 Prompt** — 深度优化的代码审查 Prompt 模板，提升效果同时降低 token 消耗
- **场景调优工具集** — 从大规模生产数据的工具调用链分析中蒸馏（调用频率分布、工具重复率、新工具对整体调用链的影响），比通用 Agent 工具集更稳定可预测

## 安装与使用

```bash
# NPM 安装（推荐）
npm install -g @alibaba-group/open-code-review

# 配置 LLM
ocr config set llm.url https://api.anthropic.com/v1/messages
ocr config set llm.auth_token your-api-key
ocr config set llm.model claude-opus-4-6
ocr config set llm.use_anthropic true

# 测试连通性
ocr llm test

# 审查当前 workspace 所有变更
ocr review

# 审查分支差异
ocr review --from main --to feature-branch

# 审查单个 commit
ocr review --commit abc123
```

兼容 Claude Code 环境变量（`ANTHROPIC_BASE_URL`、`ANTHROPIC_AUTH_TOKEN`、`ANTHROPIC_MODEL`）。

## 与编码 Agent 集成

- **Skill 方式**：`npx skills add alibaba/open-code-review --skill open-code-review`
- **Claude Code 插件**：`/plugin marketplace add alibaba/open-code-review`
- **手动复制命令文件**：复制到 `.claude/commands/` 或 `~/.claude/commands/`

## CI/CD 集成

```bash
ocr review --from "origin/main" --to "origin/feature-branch" --format json
```

提供 GitHub Actions 和 GitLab CI 集成示例。

## 主要命令

| 命令 | 说明 |
|------|------|
| `ocr review` / `ocr r` | 启动代码审查 |
| `ocr rules check <file>` | 预览某文件的审查规则 |
| `ocr config set <key> <value>` | 设置配置 |
| `ocr llm test` | 测试 LLM 连通性 |
| `ocr viewer` / `ocr v` | 启动 WebUI 会话查看器 |
| `ocr version` | 版本信息 |

## 审查规则四层优先级

| 优先级 | 来源 | 路径 |
|--------|------|------|
| 1（最高） | `--rule` 标志 | CLI 显式覆盖 |
| 2 | 项目配置 | `<repoDir>/.opencodereview/rule.json` |
| 3 | 全局配置 | `~/.opencodereview/rule.json` |
| 4（最低） | 系统默认 | 内置 `system_rules.json` |

## 技术栈

- **Go 71.3%** + TypeScript 16.2% + CSS/JS/Shell/HTML
- 支持 OpenAI 和 Anthropic API
- OpenTelemetry 集成用于可观测性（默认关闭）
- 并发审查（默认 8 并发），可配置超时和最大工具调用轮次
