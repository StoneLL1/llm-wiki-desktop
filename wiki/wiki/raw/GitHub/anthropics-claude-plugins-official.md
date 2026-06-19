---
title: "claude-plugins-official"
url: "https://github.com/anthropics/claude-plugins-official"
source: "GitHub"
fetched: 2026-05-25
stars: 27387
forks: 2912
language: "Python"
topics: [claude-code, mcp, skills]
created: 2025-11-20
updated: 2026-05-25
---

# claude-plugins-official

> Official, Anthropic-managed directory of high quality Claude Code Plugins.

**⭐ 27387 stars | 🍴 2912 forks | 📅 创建于 2025-11-20**

## 概述

Anthropic 官方在 GitHub 上维护的 Claude Code 插件目录/市场。这是一个官方认证的插件目录，包含内部插件（Anthropic 开发维护）和外部插件（第三方合作伙伴和社区提交）。

目前仓库里有 **30+ 个内部插件** 和 **10+ 个外部插件**，涵盖 Code Review、功能开发、遗留代码迁移、Hook 管理、多语言 LSP 支持等场景。

## 关键特性

- **一键安装**: `/plugin install {plugin-name}@claude-plugins-official`
- **图形化浏览**: Claude Code 内 `/plugin > Discover` 界面
- **插件结构标准化**: 每个插件包含 plugin.json 元数据、MCP 配置、命令、Agent、Skills
- **分类**: 内部插件（`/plugins`）+ 外部插件（`/external_plugins`）

## 核心插件

### claude-code-setup
扫描代码库，推荐最适合项目的自动化配置（MCP Servers、Skills、Hooks、Subagents、Slash Commands）。只读分析，不修改文件。

### feature-dev
7 阶段结构化功能开发流程：发现需求 → 探索代码库 → 澄清问题 → 架构设计 → 编码实现 → 质量审查 → 总结。第 4 阶段启动 2-3 个架构师 Agent 对比方案，第 6 阶段 3 个审查 Agent 并行跑。

### hookify
用自然语言描述规则，自动生成 Claude Code Hooks 配置。支持 warn/block 动作，立即生效无需重启。

### code-modernization
遗留代码现代化（COBOL、Java/C++、单体 Web → 现代技术栈）。7 步流程：评估 → 映射 → 提取规则 → 方案 → 转换 → 重新构想 → 加固。改动输出到 modernized/ 目录，不直接修改源码。

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

## 技术栈

- 主要语言: Python
- 主题标签: claude-code, mcp, skills
- 官方文档: https://code.claude.com/docs/en/plugins
