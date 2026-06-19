---
title: Everything Claude Code
created: 2026-05-22
updated: 2026-05-22
type: entity
tags:
  - tool
  - agent
  - open-source
sources:
  - raw/articles/2026-04-18-everything-claude-code-plugin-library.md
---

# Everything Claude Code

## 概述

Everything Claude Code 是 Claude Code 的开源插件集合（agent harness 优化系统），由 Anthropic 黑客松获奖者 **Affaan Mushtaq** 开发，经过 10 个多月实战打磨。GitHub 13.2 万 Star，18.9k Fork。

官方主页：https://ecc.tools/

仓库地址：https://github.com/affaan-m/everything-claude-code

## 规模

- **36 个专用 subagent** — 每个针对具体编程场景打磨
- **150+ skills** — 覆盖代码审查、测试驱动开发、数据库迁移、E2E 测试等
- **68 个命令** — 按场景调用
- **10 种编程语言 rules** — TypeScript、Python、Go 等主流语言，按需安装

## 核心特性

### pass@k / pass^k 验证指标

独特的代码可靠性量化评估方式（其他 Claude Code 配置库中罕见）：

- **pass@k**：跑 k 次至少有一次通过的概率。k=3 时达 91%
- **pass^k**：k 次全部通过的概率。k=3 时仅 34%

这个差距说明：agent 输出能否稳定可靠，不是看「碰巧能跑通」，而是看「每次都能跑通」。对关键代码的自动化生成有重要参考价值。

### AgentShield 安全集成

v1.6.0 引入，内置 **102 条安全规则**和 **1282 个安全测试**。通过 `/security-scan` 命令在提交前做安全审查，覆盖 OWASP 常见漏洞类型。

### 沙盒化 Subagent

每个 subagent 有独立的工具权限限制。例如代码审查 agent 只能读文件和搜索，不能执行写操作。在多 agent 并行工作场景下有效防止误操作。

## 跨平台支持

不只支持 [[claude-code]]，还兼容：
- [[openai-codex|OpenAI Codex]]
- [[cursor]]
- OpenCode
- Antigravity
- Gemini

一套配置通吃所有主流 AI 编程工具平台。

## 安装方式

### Plugin 命令安装（不含 rules）

```
/plugin marketplace add affaan-m/everything-claude-code
/plugin install everything-claude-code@everything-claude-code
```

### Clone 手动安装（含 rules）

```
git clone https://github.com/affaan-m/everything-claude-code.git
cd everything-claude-code
npm install
./install.sh --profile full
```

v1.9.0+ 支持按需安装，如只装 TypeScript 和 Python 的规则和 agent。

## 与 [[superpowers]] 对比

| 维度 | Everything Claude Code | Superpowers |
|------|----------------------|-------------|
| 定位 | 装备齐全的工具箱 | 资深架构师/方法论 |
| 核心 | 大量 agent + skills 覆盖场景 | 完整开发方法论（brainstorm→plan→TDD→review） |
| 流程约束 | 不约束开发流程 | 强制 TDD、code review、git worktree 隔离 |
| 优势 | 深度工程化、pass@k 验证、跨平台 | 方法论成熟、流程质量保障 |
| 偏差 | Go 语言偏重，学习成本高 | 适用场景偏重中大型项目 |

**建议**：两者可同时安装——[[superpowers]] 管流程，Everything Claude Code 管工具。

## 评分

9/10。加分：规模大、覆盖广、工程化程度高、pass@k 验证创新、跨平台到位。扣分：学习成本高（36 agent / 150 skills / 68 命令），配置偏 Go 语言。

## Relationships

- 基于 [[claude-code]] 的插件系统
- 与 [[superpowers]] 互补，定位不冲突
- 使用 [[skills]] 框架加载能力模块
- 跨平台支持 [[cursor]]、[[openai-codex]] 等工具

## See Also

- [[claude-code]] — 主要平台
- [[superpowers]] — 互补的开发方法论
- [[skills]] — Skills 生态系统
- [[claude-md]] — 项目配置文件
