---
title: OpenSpec
created: 2026-05-21
updated: 2026-05-27
type: entity
tags: [tool, engineering, open-source, skill]
sources:
  - raw/articles/2026-04-18-multi-ai-sdd-coding-practice.md
  - raw/articles/2026-04-18-zero-human-coding-ai-native-dev-handbook.md
---

# OpenSpec

## Overview

OpenSpec 是由 Fission AI 开发的轻量级命令行工具（CLI），用于实现 [[spec-driven-development]]（SDD，规范驱动开发）。它通过"提案→审查→实现→归档"流程，确保在写代码前，人和 AI 就"要做什么"达成共识。

## 核心特点

### 1. 轻量可嵌入
OpenSpec 仅通过 CLI 注入即可启用，无需改造现有系统。

### 2. 多 AI 友好
原生支持多种 AI 工具（[[claude-code]]、Codex、Cursor、OpenCode、Qoder 等）。当某个 AI 服务异常时，可无缝切换至其他工具，无需修改工作流——"同一份规范，多个模型协同"。

### 3. 变更可管理
通过 `changes/` 提案机制，每次修改都有记录、可评审、可归档。

## 工作流

```
Draft Change Proposal → Review & Align → Implement Tasks → Archive & Update
```

- `/openspec:proposal` — 创建变更提案，与 AI 共享意图
- `/openspec:apply` — 基于批准的规范执行实现
- `/openspec-archive` — 归档变更，更新源规范

## 安装

```bash
npm install -g @fission-ai/openspec@latest
openspec init  # 在项目中初始化
```

初始化会创建 `openspec/` 目录结构，并为选定的 AI 工具配置斜杠命令。

## 项目目录结构

```
项目根目录/
└── openspec/
    ├── config.yaml              ← 项目配置（技术栈、规范风格、业务背景）
    ├── specs/                   ← 归档区：系统功能的"活文档"
    │   └── [功能名称]/
    │       └── spec.md
    └── changes/                 ← 工作区：进行中的变更提案
        └── [任务名称]/
            ├── proposal.md      ← 为什么做、做什么
            ├── design.md        ← 怎么做
            ├── tasks.md         ← AI 的施工清单
            └── specs/           ← 变更增量
```

`config.yaml` 是 AI 生成代码时的基础"宪法"，需认真填写技术栈、代码规范风格、核心业务背景。

## 与多 AI 协同

OpenSpec 在 [[multi-agent-collaboration]] 场景中发挥关键作用：
- Claude 作为协调者（通过 [[claude-md]] 定义决策规则）
- Codex 作为代码专家（通过 AGENTS.md 定义行为规范）
- Gemini 作为分析专家
- 统一的 Spec 确保各模型在同一框架下工作

## 企业实践（binxiong 团队案例）

binxiong 团队使用 OpenSpec + [[codebuddy]] 实现了「0 人工 Coding」的 AI Native 研发模式。

### 核心洞察

> 把 AI 当打字员用，天花板很低。把 AI 当施工队用，才有真正的效率革命。

### 人的角色重新定义

人的核心角色只有三个：**决策、审批、把关**。其他让 AI 去干：

| 阶段 | AI 负责 | 人负责 |
|------|---------|--------|
| 需求分析 | 提炼需求、发现遗漏、结构化输出 | 需求文档优先级判断，审批需求边界 |
| 方案设计 | 生成技术方案、架构图、接口定义 | 技术选型拍板，安全审查 |
| 代码开发 | 根据设计方案多文件编码、补充注释和单测 | 评审关键代码，控制合并门禁 |
| 测试验证 | 生成测试脚本、执行回归测试、分析失败原因 | 审核高风险场景，最终放行决策 |
| 发布上线 | 校验实现是否与需求一致，提供发布清单 | 审批发布窗口，处理事故升级 |

### 三步核心工作流

1. **`/opsx:propose`** — 严禁直接让 AI 写大段业务代码，先生成规划文档。AI 在 `openspec/changes/[变更名]/` 下自动生成：
   - `proposal.md` — 需求背景与目标（为什么做、做什么）
   - `design.md` — 技术方案与架构决策（怎么做）
   - `tasks.md` — 带复选框的实施清单（AI 的施工单）
   - `specs/` — 规范增量（差异记录）

2. **`/opsx:apply`** — AI 读取 tasks.md 清单 + design.md + proposal.md 完整上下文，跨文件批量生成/修改代码，每完成一项在 tasks.md 打钩 ✓。开发者只需做 Code Review。

3. **`/opsx:archive`** — MR 通过后执行，系统自动将规范增量合并到主目录，清空变更草稿。项目文档永远和代码保持同步。

### 六步完整工作流

在三步核心基础上扩展为更严谨的六步：propose → review → prototype（前端） → apply → test → archive。Step 2 的 Review 是质量的生命线。

### 关键规则

- **原子化变更**：一次变更解决一个需求，tasks.md 控制在 15 项以内
- **双重视角审查**：审查维度同时看代码和规划文档（代码逻辑 vs design.md，架构合规 vs proposal.md，需求覆盖 vs specs/）
- **前端原型审查**：前端变更在 apply 前加一步原型确认环节

### 扩展命令

| 指令 | 作用 | 什么时候用 |
|------|------|-----------|
| `/opsx:new` | 只创建空脚手架，不生成内容 | 想自己控制生成节奏 |
| `/opsx:continue` | 步进式生成（先 Proposal → Review → 再 Design） | 逐步审查，适合复杂需求 |
| `/opsx:ff` | 快进生成，一次补全所有规划 | 需求明确，想快速推进 |
| `/opsx:verify` | AI 代码审计，比对 design.md | 写完代码想让 AI 自查 |
| `/opsx:explore` | 探索模式，和 AI 讨论需求 | 不知道怎么开始时（绝对不动代码） |

## 竞品对比

| 工具 | 来源 | 特点 |
|------|------|------|
| **OpenSpec** | Fission AI | 轻量 CLI，多 AI 友好，变更提案机制 |
| Spec Kit | GitHub | 厂商级标准化尝试 |
| BMAD | 社区开源 | Behavior-Model-Architecture-Data 分层思想 |

## See Also

- [[spec-driven-development]] — OpenSpec 实现的方法论
- [[claude-code]] — OpenSpec 的主要使用工具
- [[multi-agent-collaboration]] — 多 AI 协同场景
- [[document-first-system]] — 先文档后代码的开发哲学
- [[harness-engineering]] — 约束 AI 执行的脚手架方法
- [[context-engineering]] — 规范作为上下文管理
- [[codebuddy]] — 配合 OpenSpec 使用的 AI 编码助手
- [[skill-engineering]] — Skill 工程化设计方法论

## SDD + 多 AI 协同实战（2026-04，binxiong 团队）

基于 OpenSpec 的跨境保险产品全流程交付实录：

### 六阶段工作流
1. **生成 Spec-PRD** — 将原始产品需求文档结构化重写，明确变更内容与代码库映射
2. **总结系统架构** — 调用 Claude SubAgent（系统架构专家）分析代码库
3. **生成技术方案** — 调用 SubAgent（技术方案专家）生成可执行实现指引
4. **创建变更提案** — `/openspec:proposal` 生成 proposal.md + design.md + tasks.md
5. **分阶段实现** — `/openspec:apply` 配合 CLAUDE.md 中的分阶段约束，每完成一个阶段暂停等 Review
6. **提案归档** — `/openspec-archive` 将规范增量合并回主目录

### 关键设计
- **SubAgent 架构**：两个专用 SubAgent（architecture-expert / technical-solution-expert），使用 Claude Opus 模型
- **Codex MCP 集成**：通过 MCP 协议将 Codex 作为代码专家注入，生成 unified diff 原型
- **工具切换韧性**：当 Claude 服务不可用时，无缝切换到 iflow（Sonnet 4.5 Thinking），验证了 OpenSpec "工具无关"理念
- **分阶段约束**：CLAUDE.md 中规定每完成一个阶段必须暂停等人工 Review

### 核心洞察

> SDD 通过规范对齐，让 AI 的"概率输出"最终汇聚为"确定交付"。就像水利工程不靠堵水，而靠疏导入渠。
