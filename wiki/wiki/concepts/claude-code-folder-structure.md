---
title: Claude Code .claude 文件夹结构
created: 2026-05-23
updated: 2026-05-27
type: concept
tags: [tool, engineering, agent]
sources:
  - raw/articles/2026-04-18-claude-research-10x-better.md
---

# Claude Code .claude 文件夹结构

## Overview

`.claude` 文件夹是控制 [[claude-code]] 在项目中如何行为的核心中枢，包含指令、自定义命令、权限规则和跨会话记忆。理解其结构是高效使用 Claude Code 的关键——将它配置成完全符合团队需求的样子，你花在纠正 Claude 上的时间就越少。

实际上存在两个 `.claude` 目录：**项目级**（团队配置，提交到 Git）和**全局级** `~/.claude/`（个人偏好和本机状态）。

## 项目级结构

```
your-project/
├── CLAUDE.md                # 团队指令（提交到仓库）
├── CLAUDE.local.md          # 个人覆盖（git 忽略）
│
└── .claude/
    ├── settings.json        # 权限、hooks、配置（提交）
    ├── settings.local.json  # 个人权限覆盖（git 忽略）
    │
    ├── hooks/               # hook 脚本
    │   ├── bash-firewall.sh
    │   ├── auto-format.sh
    │   └── enforce-tests.sh
    │
    ├── rules/               # 模块化规则（可按路径生效）
    │   ├── code-style.md
    │   ├── testing.md
    │   └── api-conventions.md
    │
    ├── skills/              # 自动触发的工作流
    │   ├── security-review/
    │   │   └── SKILL.md
    │   └── deploy/
    │       └── SKILL.md
    │
    └── agents/              # 专用子代理（子人格）
        ├── code-reviewer.md
        └── security-auditor.md
```

## 全局 ~/.claude/ 结构

```
~/.claude/
├── CLAUDE.md        # 全局指令（所有项目生效）
├── settings.json    # 全局设置 + hooks
├── skills/          # 全局技能
├── agents/          # 全局代理
└── projects/        # 会话历史 + 自动记忆
```

## 各组件详解

### CLAUDE.md — 核心指令（最重要的文件）

Claude Code 启动时首先读取 [[claude-md|CLAUDE.md]]，直接加载进 system prompt，整个对话过程中持续参考。**写在 CLAUDE.md 里的内容，Claude 就会遵循。**

#### 应该写什么

- 构建、测试和 lint 命令（如 `npm run test`、`make build`）
- 关键架构决策（如"基于 Turborepo 的 monorepo"）
- 不明显的注意事项（如"TypeScript 严格模式，未使用变量会报错"）
- 导入规范、命名模式、错误处理风格
- 主要模块的文件和目录结构

#### 不要写什么

- 本应写在 linter 或 formatter 配置里的内容
- 已经可以通过链接获取的完整文档
- 大段理论性解释

**控制在 200 行以内。** 文件太长会占用过多上下文，反而降低 Claude 对指令的遵循效果。

#### 多层级配置

- 项目根目录 `CLAUDE.md`：团队共享配置
- `CLAUDE.local.md`：个人偏好（自动 git 忽略）
- 子目录 `CLAUDE.md`：针对该目录的规则
- `~/.claude/CLAUDE.md`：全局个人配置
- Claude 会读取所有这些文件并合并使用

### rules/ — 模块化指令

解决 CLAUDE.md 膃胀问题。每个 Markdown 文件自动与 CLAUDE.md 一起加载。支持「按路径生效」的 frontmatter：

```yaml
---
paths:
  - "src/api/**/*.ts"
  - "src/handlers/**/*.ts"
---
# 这些规则只在处理 src/api/ 下的文件时启用
```

没有 `paths` 字段的规则是"全局生效"，每次会话都加载。按关注点拆分指令（code-style.md、testing.md、api-conventions.md），不同团队成员各维护自己负责的部分。

### hooks/ — 确定性行为控制

详见 [[claude-code-hooks]]。将 CLAUDE.md 的"软约束"升级为确定性执行。

#### 退出码语义

- **exit 0**：成功，继续执行
- **exit 1**：报错，但**不会阻止执行**（仅记录）
- **exit 2**：**阻止执行**，并将 stderr 返回给 Claude 用于自我修正

> ⚠️ 安全类 hook 常见错误：使用 exit 1 只会记录日志但不会阻止操作。需要阻止请用 exit 2。

#### 常用事件类型

| 事件 | 触发时机 | 典型用途 |
|------|---------|---------|
| PreToolUse | 工具执行前 | 安全关卡、拦截危险命令 |
| PostToolUse | 工具执行成功后 | 自动格式化、lint |
| Stop | Claude 完成任务时 | 质量检查（如"必须通过测试"） |
| UserPromptSubmit | 用户按下回车时 | 提示词校验 |
| Notification | — | 桌面通知 |
| SessionStart / SessionEnd | 会话开始/结束 | 注入上下文或清理环境 |

#### matcher 正则

- `"Write|Edit|MultiEdit"`：针对文件修改
- `"Bash"`：针对命令行操作
- 不写 matcher：匹配所有工具

#### Stop Hook 防死循环

必须检查 JSON 负载中的 `stop_hook_active` 标志，否则会出现死循环：hook 阻止 Claude → Claude 重试 → hook 再阻止。正确做法是允许第二次通过。

#### 注意事项

- hooks 在会话中**不会热更新**
- PostToolUse 无法"撤销"操作（已执行），需要拦截请用 PreToolUse
- hooks 对子 agent 也会递归触发
- hooks 以用户权限执行，没有沙箱保护：必须对 shell 变量加引号、校验 JSON 输入、使用绝对路径

### skills/ — 可复用工作流

详见 [[skills]]。Claude 可以根据上下文自动调用的能力模块，每个 skill 包含 SKILL.md 定义触发条件。Skills 可以打包多个辅助文件（如 DETAILED_GUIDE.md）。个人 skills 可以放在 `~/.claude/skills/`，在所有项目中通用。

SKILL.md 使用 YAML 前置块定义触发条件（name、description、allowed-tools），description 字段的关键词匹配决定何时触发。也可通过 `/skill-name` 手动调用。

### agents/ — 子代理（子人格）

定义专用子代理（subagent），拥有：
- **独立上下文**：主对话不会被大量中间分析过程淹没
- **工具白名单**：权限控制（如安全审计 agent 只需要读取权限）
- **模型选择**：Haiku（快速只读分析）/ Sonnet / Opus（复杂任务），用于成本优化

个人 agents 可以放在 `~/.claude/agents/`，在所有项目中使用。

### settings.json — 权限控制

控制 Claude 可以做什么、不能做什么：

- **allow**：免确认执行（如 `Bash(npm run *)`、`Read`、`Write`）
- **deny**：完全禁止（如 `Bash(rm -rf *)`、`Read(./.env)`）
- 未列出的操作会**先询问再执行**（安全缓冲区）

使用 `$schema` 启用 VS Code / Cursor 中的自动补全和校验。`settings.local.json` 存放个人权限配置（自动 git 忽略）。

## 入门配置流程（5 步）

1. 运行 `/init` 自动生成初始 [[claude-md|CLAUDE.md]]，精简为核心内容
2. 创建 `.claude/settings.json`（至少含 allow 运行命令、deny 读取 .env）
3. 创建 1-2 个常用 commands（如代码审查、修复 issue）
4. 当 CLAUDE.md 臃肿时，拆分到 `.claude/rules/`，按路径作用域管理
5. 创建 `~/.claude/CLAUDE.md` 写个人偏好

> 对于 95% 的项目，这已经完全够用。skills 和 agents 是进阶工具。

## 核心原则

> **优先把 CLAUDE.md 写好。这是杠杆最高的部分，其它都是优化。**

从简单开始，逐步迭代，把它当作项目基础设施的一部分来维护——一旦配置好，它每天都会为你持续产生价值。

## Relationships

- 核心配置层，支撑 [[claude-code]] 的所有行为
- [[claude-md]] 是其中最重要的单文件
- [[claude-code-hooks]] 提供确定性控制
- [[skills]] 实现可复用能力扩展
- 是 [[harness-engineering]] 实践的基础设施

## See Also

- [[claude-code]] — 这些配置运行的平台
- [[claude-md]] — 核心指令文件
- [[claude-code-hooks]] — 确定性行为控制
- [[skills]] — 按需工作流
- [[harness-engineering]] — 配置设计背后的方法论
