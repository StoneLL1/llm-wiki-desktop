---
title: Open Code Review
created: 2026-06-07
updated: 2026-06-07
type: entity
tags: [open-source, tool, code, engineering, skill]
sources:
  - raw/GitHub/alibaba-open-code-review.md
---

# Open Code Review

阿里巴巴开源的 AI 代码审查 CLI 工具（3,144 ⭐），采用「确定性工程管道 + LLM Agent」混合架构，解决纯 Agent 模式的覆盖不完整、位置漂移、质量不稳定三大痛点。

## 核心设计

### 混合架构

确定性工程层（硬约束）保证审查关键步骤：

- **精确文件选择** — 判断哪些文件需要审查、哪些应过滤
- **智能文件捆绑** — 将相关文件分组为审查单元，每个捆绑作为独立子 Agent 运行（天然并发）
- **细粒度规则匹配** — 模板引擎按文件特征匹配审查规则
- **外部定位与反思模块** — 独立模块系统性提升评论位置和内容准确度

Agent 层（动态决策）专注高价值环节：

- 场景调优 Prompt 模板
- 从大规模生产数据蒸馏的工具集（调用频率分布、工具重复率分析）

### 审查规则四层优先级

| 优先级 | 来源 | 路径 |
|--------|------|------|
| 1（最高） | CLI `--rule` 标志 | 显式覆盖 |
| 2 | 项目配置 | `<repo>/.opencodereview/rule.json` |
| 3 | 全局配置 | `~/.opencodereview/rule.json` |
| 4（最低） | 系统默认 | 内置 `system_rules.json` |

内置 NPE、线程安全、XSS、SQL 注入等规则集。

## 技术栈

- Go 71.3% + TypeScript 16.2%
- 支持 OpenAI 和 Anthropic API
- OpenTelemetry 可观测性集成
- 默认 8 并发，可配置超时和最大工具调用轮次

## 与 [[claude-code]] 集成

三种方式：

1. **Skill 方式**：`npx skills add alibaba/open-code-review --skill open-code-review`
2. **Claude Code 插件**：`/plugin marketplace add alibaba/open-code-review`
3. **手动复制**：命令文件复制到 `.claude/commands/`

兼容 Claude Code 环境变量（`ANTHROPIC_BASE_URL`、`ANTHROPIC_AUTH_TOKEN`）。

## 与同类工具对比

| 维度 | Open Code Review | [[aider]] | [[claude-code]]（内置审查） |
|------|-----------------|-----------|---------------------------|
| 定位 | 专用代码审查 CLI | 结对编程 CLI | 通用编程 Agent |
| 架构 | 确定性管道 + Agent | 纯 Agent | 纯 Agent |
| 审查粒度 | 行级精确评论 | 无审查功能 | diff 级别 |
| CI/CD | 原生支持 | 无 | 需手动配置 |
| 生产验证 | 阿里 2 年 / 数万开发者 | 社区验证 | Anthropic 内部 |
