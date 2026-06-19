---
title: claude-plugins-official
created: 2026-05-25
updated: 2026-05-25
type: entity
tags: [open-source, tool, agent]
sources:
  - raw/articles/2026-05-25-guangguang-github-claude-plugins-official.md
  - raw/GitHub/anthropics-claude-plugins-official.md
---

# claude-plugins-official

## Overview

**claude-plugins-official** 是 [[anthropic]] 官方在 GitHub 上维护的 [[claude-code]] 插件目录，GitHub 27387 Stars / 2912 Forks。作为官方认证的插件市场，包含内部插件（Anthropic 开发维护）和外部插件（第三方合作伙伴和社区提交），涵盖 Code Review、功能开发、遗留代码迁移、Hook 管理、多语言 LSP 支持等场景。

仓库创建于 2025-11-20，主要语言为 Python，主题标签涵盖 claude-code、[[mcp]]、[[skills]]。

## 插件分类

- **内部插件**（`/plugins`）— 30+ 个，由 Anthropic 开发维护
- **外部插件**（`/external_plugins`）— 10+ 个，第三方社区提交

## 核心插件

### claude-code-setup
扫描代码库，推荐最适合项目的自动化配置（MCP Servers、Skills、Hooks、Subagents、Slash Commands）。只读分析，不修改文件。在 X 上被广泛推荐，被称为"让 Claude Code 满血复活"的插件。

```
/plugin install claude-code-setup@claude-plugins-official
```

### feature-dev
7 阶段结构化功能开发流程：发现需求 → 探索代码库 → 澄清问题 → 架构设计 → 编码实现 → 质量审查 → 总结。

- **第 4 阶段**：启动 2-3 个架构师 Agent，分别从最小改动、干净架构、务实平衡三个角度设计方案并对比推荐
- **第 6 阶段**：3 个独立审查 Agent 并行跑——代码质量、Bug 检测、项目规范检查

```
/plugin install feature-dev@claude-plugins-official
```

### hookify
用自然语言描述规则，自动生成 [[claude-code]] Hooks 配置。支持 warn（警告但允许）和 block（直接拦截）两种动作类型，立即生效无需重启。

典型场景：防止误删文件、阻止在 TypeScript 文件里写 console.log、要求提交前必须跑测试。

```
/plugin install hookify@claude-plugins-official
```

### code-modernization
遗留代码现代化插件，支持 COBOL、遗留 Java/C++、单体 Web 应用 → 现代技术栈迁移。7 步流程：

1. `/modernize-assess` — 评估
2. `/modernize-map` — 映射
3. `/modernize-extract-rules` — 提取规则
4. `/modernize-brief` — 方案
5. `/modernize-transform` — 转换
6. `/modernize-reimagine` — 重新构想
7. `/modernize-harden` — 加固

所有改动输出到 `modernized/` 目录，不直接修改源码。

## 安装方式

两种方式安装插件：

```bash
# 命令行安装
/plugin install {name}@claude-plugins-official

# 图形化浏览安装
/plugin > Discover
```

建议首次使用先装 claude-code-setup，让它分析项目后推荐最适合的插件组合。

## 插件标准结构

```
plugin-name/
├── .claude-plugin/
│   └── plugin.json      # 插件元数据（必需）
├── .mcp.json            # MCP 服务器配置（可选）
├── commands/            # 斜杠命令（可选）
├── agents/              # Agent 定义（可选）
├── skills/              # Skill 定义（可选）
└── README.md            # 文档
```

## 相关链接

- GitHub: https://github.com/anthropics/claude-plugins-official
- 官方文档: https://code.claude.com/docs/en/plugins
- 参考：[[everything-claude-code]]、[[superpowers]]、[[claude-code-hooks]]
