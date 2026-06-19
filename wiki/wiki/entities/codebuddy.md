---
title: CodeBuddy
created: 2026-05-23
updated: 2026-05-27
type: entity
tags: [tool, engineering, enterprise, skill]
sources:
  - raw/articles/2026-04-18-zero-human-coding-ai-native-dev-handbook.md
---

# CodeBuddy

## 概述

CodeBuddy 是腾讯内部使用的 AI 编码助手，支持 CLI 版和 IDE 插件版（推荐 CLI）。在「0 人工 Coding」AI Native 研发实战中，CodeBuddy 作为核心编码执行工具，配合 [[openspec]] 实现从规范到代码的全链路自动化。

作者 binxiong 团队在 2025 年开始大规模使用 AI 辅助编码，2026 年初正式全面转向 AI Native 研发模式。

## 核心能力

### 三大机制

1. **知识库** — 让 AI「知道」项目上下文
   - OpenSpec specs/ 作为核心记忆（活文档）
   - MCP 外部知识作为辅助记忆（业务术语、架构决策、踩坑记录）
2. **MCP** — 让 AI「连接」团队工具
   - TCS Component MCP：前端组件库查询
   - TAPD MCP：需求平台直连
   - iWiki MCP：文档平台检索
   - 极光流水线 MCP：部署平台触发
3. **Skills** — 让 AI「掌握」团队方法
   - SKILL.md 定义的标准化操作流程
   - 统一管理在内部 Git 仓库（ted.aurora/tcsc-skills），按业务域分类
   - 通过 SkillHub 市场预览和安装

### 工作流集成

配合 [[openspec]] 的 `/opsx:propose` → `/opsx:apply` → `/opsx:archive` 三步工作流，CodeBuddy 实现了：
- 按图施工：读取 proposal.md + design.md + tasks.md 上下文
- 自动标记完成：tasks.md 中的清单逐步打钩
- 持久文档同步：archive 后 specs/ 主目录自动更新

### CLI 与 IDE 搭配

- **CLI**：执行 `/opsx:propose`、`/opsx:apply` 等核心指令，跨项目协同
- **IDE**：审查修改 proposal.md / design.md，解决 Git 冲突，单行逻辑优化

### 多项目联动

在父目录下启动 CLI，AI 拥有全局视角，一条指令同时搞定前后端（如 web-frontend/ + api-backend/）。

## 关键设计决策

### Bridge Rule — 解决「链路断裂」

AI 不知道要读 `config.yaml` 中的 rules。解法是在安装时自动生成一个"指路牌"文件（`.codebuddy/rules/openspec-config-awareness.md`），使用 `alwaysApply: true` 每次会话自动加载。

设计思想：只做"指路"而非复制规则内容，保持 config.yaml 作为 single source of truth。

版本迭代：v0.4.2 使用 `alwaysApply: false` 偶尔漏判；v0.4.3 改为 `alwaysApply: true`，问题彻底解决。

### Token 交互 — 在 SKILL.md 指令层完成

MCP 服务需要 PAT Token 认证，但由于 CodeBuddy 的 bash 执行环境是非 TTY 的，Token 交互通过 SKILL.md 的 SOP 指令层完成（AI 在对话中引导用户），而非 bash 脚本的 `read -p`。

同时自动将 `.mcp.json` 加入 `.gitignore`，防止含有 Token 的文件被意外提交。

### 三级降级策略

版本检查采用 MCP 查询 → 本地文件读取 → 静默跳过的三级降级，确保辅助功能失败不阻塞核心安装。脚本通过环境变量 `MCP_REMOTE_VERSION` 接收 MCP 的查询结果。

### 幂等性设计

每个安装步骤都设计了幂等性——重复执行不会出问题。安装中断了可以随时重来。

## openspec-installer Skill 深度剖析

openspec-installer 是一个生产级 Skill 的完整范例：

### 文件结构

```
openspec-installer/
├── SKILL.md                    # 600+ 行的完整安装 SOP
├── version.json                # 版本控制元数据
├── scripts/
│   ├── INSTALL_MAC_LINUX.sh    # macOS/Linux 环境自动安装
│   ├── INSTALL_WINDOWS.ps1     # Windows 环境自动安装
│   ├── install_skills.sh       # 项目 Skills 批量安装
│   ├── check-skill-updates.sh  # 所有 Skills 版本检测
│   └── self_update.sh          # Skill 自身热更新
└── templates/
    ├── mcp-servers.json        # MCP 配置模板（含认证信息）
    ├── skill-bundle.json       # 项目默认 Skill 套装清单
    └── openspec-config-awareness.md  # Bridge Rule 模板
```

### version.json

不只是版本号——包含 changelog、git_repo、skill_path 等元数据。`self_update.sh` 会解析 JSON 比较版本号，展示精确到每个版本的变更内容。

### skill-bundle.json

定义每个项目需要安装的 Skills 清单，`install_skills.sh` 批量安装。新人入职无需问"要装什么 Skill"。

### SKILL.md 设计原则

- **description 字段双重职责**：让人理解功能 + 让 AI 知道何时触发（关键词匹配）
- **分步 SOP**：每个步骤都是可执行的 bash 代码块
- **前后端中间件**：7 个主要步骤，覆盖从环境检测到用户指引

### 迭代管理

openspec-installer 自身也用 OpenSpec 的 propose → apply → archive 流程管理，每个变更都有完整的 proposal + design + spec + tasks。

## 团队协同规则

### 原子化变更
- 一次变更解决一个需求
- tasks.md 控制在 15 项以内（避免 AI 因上下文过长产生幻觉）
- 小步快跑确保人工 Code Review 在可控范围内

### 双重视角审查
- 代码逻辑 vs design.md
- 架构合规 vs proposal.md
- 需求覆盖 vs specs/

### 规范即文档
- `openspec/specs/` 目录就是永远与代码保持一致的"活体文档"
- 禁止绕过 OpenSpec 直接手写大量核心业务逻辑

## See Also

- [[openspec]] — 规范驱动开发框架
- [[ai-native-development]] — AI 原生开发范式
- [[spec-driven-development]] — 规范先行的方法论
- [[skills]] — AI Agent 的模块化能力单元
- [[claude-code]] — 类似定位的开源编码 Agent
- [[mcp]] — Model Context Protocol
- [[skill-engineering]] — Skill 工程化设计方法论
