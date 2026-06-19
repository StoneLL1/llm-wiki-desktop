---
title: OpenAI Codex
created: 2026-05-17
updated: 2026-06-10
type: entity
tags:
  - agent
  - code
  - tool
  - open-source
sources:
  - raw/articles/2026-05-07-effective-harnesses-for-long-running-agents.md
  - raw/articles/2026-05-11-harness-engineering-knowledge.md
  - raw/articles/2026-05-12-10-new-open-source-github-projects.md
  - raw/articles/2026-05-24-jason-liu-getting-most-out-of-codex.md
  - raw/articles/2026-05-24-openai-codex-tips-jason-liu.md
  - raw/articles/2026-05-19-codex-goals-guide.md
  - raw/articles/2026-05-28-codex-best-practices-openai-official.md
  - raw/articles/2026-06-09-openai-codex-best-practices-guide.md
---

# OpenAI Codex

## 概述

OpenAI Codex 是 OpenAI 推出的编程 Agent 产品，代表了 AI 辅助编程从代码补全向自主编程 Agent 的演进。作为 [[harness-engineering]] 三大标志性实践之一，Codex 展示了如何通过精心设计的脚手架来引导和约束 AI 模型的编程能力。

2026 年 5 月，Codex 团队的 [[jason-liu]] 发表了官方指南《Getting the Most out of Codex》，系统揭示了 Codex 已从编程工具扩展为跨应用、跨设备的工作系统。同月，OpenAI 官方发布了 Codex 入门最佳实践指南，核心理念：**把 Codex 当成需要持续配置和改进的队友，而不是一次性的助手。**

## Prompt 工程

### 四要素 Prompt 结构

一个好的 Codex prompt 应包含四个要素：

1. **目标**（Goal）— 你想改变或构建什么
2. **上下文**（Context）— 哪些文件、文档、示例或报错与任务相关，可用 `@` 直接提及具体文件
3. **约束条件**（Constraints）— 需遵循的标准、架构要求或规范
4. **完成标志**（Completion Criteria）— 任务结束的判断依据（测试通过、行为改变、Bug 不复现）

### 推理强度

根据任务难度选择推理强度：简单任务用低强度，复杂任务或调试用中高强度，长时间推理密集型任务用最高强度。

### 语音输入

Codex 应用中可直接用语音输入描述需求，比打字快得多。

## 规划模式

### 三种规划方式

复杂任务应先让 Codex 规划再动手：

1. **Plan 模式**（最简单有效）：通过 `/plan` 或 `Shift+Tab` 切换，让 Codex 先收集上下文、提出澄清问题，再制定完善计划
2. **让 Codex 采访你**：模糊想法不知如何描述时，让 Codex 先采访你、挑战假设，把模糊想法变为具体需求
3. **PLANS.md 模板**：适合更复杂的多步骤工作流，类似 [[spec-driven-development]] 中的规划文档

## 核心配置

### AGENTS.md 配置

Codex 支持 AGENTS.md 配置文件，类似于 [[claude-code]] 的 CLAUDE.md（参见 [[claude-md]]）。AGENTS.md 是面向 AI Agent 的 README 文件，自动加载到上下文中。

**多层级支持**（越具体优先级越高）：
- `~/.codex/` — 个人全局默认配置
- 仓库根目录 — 团队共享标准
- 子目录 — 局部规则

**一份好的 AGENTS.md 涵盖**：仓库结构和重要目录、项目启动方式、构建/测试/Lint 命令、工程规范和 PR 要求、约束条件和禁止事项、任务完成标准和验证方式。

**维护策略**：用 `/init` 生成初始模板，保持简洁（短而准比长而模糊更有用），发现 Codex 反复犯错时复盘并更新 AGENTS.md。如果文件膨胀，保持主文件简洁，将规划、代码审查、架构等具体内容放到独立 markdown 文件中引用。

### config.toml 配置

配置是让 Codex 跨会话表现稳定的主要手段：

| 配置层 | 路径 | 使用场景 |
|--------|------|----------|
| 个人默认 | `~/.codex/config.toml` | 全局设置（Codex 应用→设置→配置 打开） |
| 仓库专属 | `.codex/config.toml` | 项目特定配置 |
| 命令行覆盖 | 临时覆盖 | 临时调整 |

配置项包括：默认模型、推理强度、沙箱模式、审批策略、MCP 设置。

**两个关键权限控制**：
- **审批模式**（Approval Mode）：命令执行前是否需要确认
- **沙箱模式**（Sandbox Mode）：能读写哪些目录和文件

建议从严格权限开始，熟悉工作流后逐步放开可信仓库的权限。CLI、IDE 插件和 Codex 应用共享同一套配置层——很多质量问题其实是配置问题（工作目录不对、缺少写权限、模型默认值不合适等）。

### CLI + Agent 模式

Codex 与 [[claude-code]] 一样，采用 CLI（命令行界面）+ Agent 的双模式设计：

- **CLI 模式**：快速执行单次编程任务，适合简单直接的代码生成和修改
- **Agent 模式**：自主规划和执行复杂的多步骤编程任务，具备文件读写、命令执行等完整能力

## 高级功能（Jason Liu 官方指南）

### Durable Threads（持久线程）

持久线程是 Codex 的核心架构概念——线程不是一次性对话，而是持久化的工作空间。关掉再打开，之前的决策、偏好和工作上下文都保留。

推荐的固定线程类型：
- **Chief of Staff 线程**：处理日常杂务、收发邮件、安排优先级
- **Release 线程**：追踪版本发布进度
- **Documentation Review 线程**：持续审核和更新文档
- **External Monitoring 线程**：跟踪外部信息变化

用 `Command-1` 到 `Command-9` 快捷键直接跳到对应线程。

### Steering + Queuing（实时干预 + 任务排队）

- **Steering**：在 Agent 执行过程中随时打断纠正方向。例如看着它做网页，直接说"这个间距不对"
- **Queuing**：不打断当前任务，追加后续指令："做完之后，把预览链接发给 Slack 里的审阅者"

两者配合的效果是：用户可以一边看着 Agent 干活，一边实时调整方向 + 排好后续任务，整个过程不需要停下来重新写 prompt。

### 工具和触达范围

Codex 的操作范围从代码向多层级扩展：

| 层级 | 工具 | 用途 |
|------|------|------|
| 内置浏览器 | `$browser` | 在侧边栏中检查和标注网页 |
| Chrome 级 | `@chrome` | 使用已登录的浏览器状态，处理需认证的操作 |
| 桌面 GUI | `@computer` | 操作只有图形界面的应用 |
| 扩展连接 | MCP 服务器 + 连接器 | Slack、Gmail、Calendar 等外部工作流 |

### Thread Automations（线程自动化）

两种自动化模式：
- **Scheduled Automations**：按时间表运行，每次从头开始（如每天日报）
- **Thread Automations**：在同一个线程中定时唤醒，带着之前的上下文继续工作

典型用法——Chief of Staff 线程每 30 分钟运行一次，检查 Slack 和 Gmail 未回复消息、排列优先级、起草回复但不发送。用户回来时，上下文收集已完成，只需审阅确认。

核心洞察：**Agent 最有价值的能力，不在于它能替你做什么，而在于它能替你等什么。**

### Goals（目标驱动）

`/goal` 功能为 Agent 设定明确的终点线：

- **弱目标**："按照这个 Markdown 文件里的计划实现"——缺少验证标准
- **强目标**：把 Python 项目迁移到 Rust，用单元测试作为成功标准——有验证机制

好的 Goal 需要配套验证器：测试套件、Benchmark、Bug 复现步骤、端到端工作流。> 野心当然重要，但没有验证机制，它终归只是个愿望。

详细的 Goals 编写指南、强 Goal vs 弱 Goal 对比、研究 Goal 实战案例（Deep Hedging 论文复现）、Goals 内部架构设计，以及何时不用 Goals 的准则，见 → [[codex-goals]]

### 自测自审循环

不要让 Codex 只做改动——还要让它编写测试、运行检查、确认结果，并在接受前审查工作：

1. **编写或更新测试**，为改动编写相应测试
2. **运行测试套件**，确保改动不引入回归
3. **检查 Lint/格式化/类型**，保证代码质量
4. **确认最终行为**符合需求
5. **审查 diff** 是否存在 Bug、回归或风险模式

**Diff 面板**：在 Codex 应用中切换 diff 面板可直接查看本地改动，点击具体行提供反馈作为下一轮上下文。

**`/review` 命令**支持多种审查方式：
- 对比基础分支的 PR 式审查
- 审查未提交的改动
- 审查某个 commit
- 使用自定义审查指令

**code_review.md**：如果团队有此文件并在 AGENTS.md 中引用，Codex 审查时会遵循那些规范。OpenAI 内部 100% 的 PR 都经过 Codex 审查（可自动触发或 `@Codex` 手动触发）。

### MCP 集成

当 Codex 需要的上下文在代码库之外时，用 [[mcp]] 连接。Codex 同时支持 STDIO 和带 OAuth 的 Streamable HTTP 服务器。

**设置方式**：Codex 应用→设置→MCP 服务器（查看自定义和推荐服务器），CLI 中 `codex mcp add` 添加。Codex 通常能帮你安装所需服务器，直接问它就行。

**接入原则**：有节制，只在能真正打通某个工作流时才加。从一两个能明确减少手动操作的工具开始逐步扩展。

### Codex Skills

当工作流可重复时，做成 Skill（SKILL.md），Codex 会持续应用这套指令和上下文。技能在 CLI、IDE 插件和 Codex 应用里都能用。

**Skill 设计原则**：
- 每个技能只做一件事
- 从 2-3 个具体用例开始，定义清晰的输入和输出
- 描述要写清楚这个技能做什么以及什么时候用
- 包含用户实际会说的触发短语
- 先让典型任务跑通，再做成技能持续优化

**`/skill-creator`** 是搭建第一个技能的最佳起点。个人技能存放在 `$HOME/.agents/skills`，团队共享技能提交到 `.agents/skills`。

**适合做成 Skill 的场景**：日志分析、发布说明起草、对照清单的 PR 审查、迁移规划、遥测或事故摘要、标准调试流程。如果你发现自己一直在复用同一个提示词或反复纠正同一个工作流，那它就应该变成一个 Skill。

### 自动化

一旦工作流稳定，让 Codex 在后台定时执行。在 Codex 应用自动化标签页中选择项目、提示词、频率和运行环境。可调用 Skill 作为提示词，可选独立 git 工作树或本地环境运行。

**适合自动化的任务**：汇总近期提交、扫描潜在 Bug、起草发布说明、检查 CI 失败、生成每日站会摘要、定时运行可重复分析。

**区分方式**：Skill 定义方法，自动化定义时间表。如果工作流还需大量人工引导，先做成 Skill，等它可预测了再自动化。自动化也可用来做回顾和维护——定期审视会话、总结反复出现的问题，持续优化提示词和工作流设置。

### 线程管理

Codex 的会话不只是聊天记录，而是积累上下文、决策和操作的工作线程。

**关键斜杠命令**：
| 命令 | 用途 |
|------|------|
| `/resume` | 恢复保存的对话 |
| `/fork` | 创建新线程同时保留原始记录 |
| `/compact` | 线程过长时生成早期上下文摘要（Codex 也会自动压缩） |
| `/agent` | 在并行多 Agent 间切换活跃线程 |
| `/experimental` | 切换实验性功能并写入 config.toml |
| `/theme` | 选择语法高亮主题 |
| `/apps` | 在 Codex 里直接使用 ChatGPT 应用 |
| `/status` | 查看当前会话状态 |

**线程管理原则**：每个连贯工作单元保持一个线程。同一问题的延续待在同一线程（保留推理轨迹），只有工作真正分叉时才 fork。可用子 Agent 把有边界的任务从主线程分出去：主 Agent 聚焦核心问题，子 Agent 处理探索、测试或分类任务。

### 常见错误

Codex 新手的 8 个常见陷阱：

1. 把持久性规则堆进 prompt，而不是放进 AGENTS.md 或 Skill
2. 没告诉 Agent 如何运行构建和测试命令 → 看不到自己的工作结果
3. 跳过多步骤复杂任务的规划阶段
4. 还没搞清楚工作流就给 Codex 开放了完整权限
5. 在同一批文件上并行运行多个会话却没用 git 工作树
6. 在任务还不稳定时就把它变成自动化
7. 像监工一样盯着 Codex 一步步执行，而不是让它并行工作
8. 用一个线程对应一个项目而不是一个线程对应一个任务 → 上下文膨胀、效果变差

### Side Panel（侧边栏）

侧边栏承担四个功能：检查产物、标注修改、操作网页、审查代码变更。支持 Markdown、电子表格、数据表、文档、幻灯片、PDF 等。

推荐产物格式：
- `index.html`：轻量级静态产物，不需要服务器
- Storybook：UI 组件审查
- Remotion Studio：程序化动画
- 浏览器幻灯片：演示文稿
- 数据应用：分析工作流

### Shared Memory（共享记忆）

Codex 推荐用 [[obsidian]] 知识库存储跨线程持久上下文。通过 AGENTS.md 定义知识库使用规则：

- 把 ~/vault 当作持久工作记忆
- 优先更新已有笔记，而不是到处创建新文件
- 保留决策、阻塞项、负责人、日期和有用链接
- 如果没有有意义的变化，就不要动知识库

代码归代码仓库，滚动的工作上下文归知识库。Codex 还有内置记忆系统（Settings > Personalization > Memories）用于偏好和常用工作流。

### 移动端

Codex 移动端允许任务从电脑启动、手机继续跟进。工作状态跟着账号走，不绑定本地终端。

## 官方最佳实践框架（六大支柱）^[raw/articles/2026-06-09-openai-codex-best-practices-guide.md]

字节笔记本逐条拆解 OpenAI 官方 Codex Best Practices 文档，核心理念：**把 Codex 当可配置的队友，给它建立对你的仓库、规范、工作习惯的持久记忆。**

### 六大支柱

| 支柱 | 核心内容 | 对应章节 |
|------|----------|----------|
| 任务上下文 | 四要素 Prompt（Goal + Context + Constraints + Done when） | [[prompt-engineering]] |
| [[claude-md]]\|AGENTS.md | 持久指引，自动加载到上下文 | 类比 [[claude-code]] 的 CLAUDE.md |
| 配置文件 | 跨会话行为一致（config.toml 分层） | [[harness-engineering]] |
| MCP | 连接外部系统（Slack/Gmail/Calendar 等） | [[mcp]] |
| Skills | 可复用工作流封装（SKILL.md） | [[skills]] |
| Automations | 稳定工作流后台自动执行 | [[automation]] |

### 四要素 Prompt 框架

- **Goal** — 想改什么/做什么
- **Context** — 哪些文件、目录、文档、报错信息相关（可直接 `@` 文件）
- **Constraints** — 规范、架构约定、安全要求
- **Done when** — 什么条件满足算完成（测试通过、行为符合预期、bug 不复现）

### 推理级别选择

| 级别 | 适用场景 |
|------|----------|
| Low | 快速、范围明确的任务 |
| Medium / High | 复杂改动或调试（推荐 Medium 兼顾速度和性价比） |
| Extra High | 长时间运行的 agentic 任务、深度推理 |

### 三种规划方法

1. **Plan mode**（`/plan` 或 `Shift+Tab`）：让 Codex 先收集上下文、问清问题，再开始写代码
2. **让 Codex 采访你**：模糊想法时让 Codex 先挑战假设，把模糊想法变为具体需求
3. **PLANS.md 模板**：仓库里维护规划模板，适合长期或多步骤工作流

### AGENTS.md 三级层级 + 维护策略

- `~/.codex/` 全局 → 仓库根目录 → 子目录（越近优先级越高）
- **短而准确 > 长而模糊**：先写基础，只在发现重复错误后增加新规则
- **从错误进化**：Codex 犯同一错误两次 → 让它复盘 → 把纠正规则更新进 AGENTS.md
- 文件膨胀时：主文件保持精简，把具体任务指导抽成独立 markdown 文件引用

### Config.toml 分层

`~/.codex/config.toml`（个人默认）→ `.codex/config.toml`（仓库专属）→ CLI 临时覆盖。可配置：模型、推理强度、沙箱模式、审批策略、[[mcp]] 设置、profiles 等。

### Skill 设计与存储

- 用 `$skill-creator` 生成第一版框架，`$skill-installer` 安装
- 个人 Skills：`$HOME/.agents/skills`；团队共享：仓库 `.agents/skills`（可提交 git）
- 设计原则：每个 Skill 聚焦一件事，从 2-3 个具体用例出发，定义清晰输入输出

### 线程管理要点

- **每个任务一个线程**（不是每个项目一个线程），防止上下文膨胀
- `/fork`：工作真正分叉时才用（同问题留在同线程保留推理上下文）
- `/compact`：线程过长时生成摘要版上下文
- `/agent`：并行多 Agent 间切换活跃线程

### 成熟工作流

| 阶段 | 动作 |
|------|------|
| **初始化**（一次性） | `/init` 生成 AGENTS.md → 修改写入实际命令/约束 → 配置 `~/.codex/config.toml` → 接入 1-2 个高价值 [[mcp]] 服务器 |
| **日常任务** | 每任务开新线程 → 四要素 prompt → 复杂任务先 Plan mode → 启动后去做别的 → 回来用 diff 面板 review → 稳定 pattern 封装成 [[skills]] |
| **持续改进** | 错误犯两次 → 更新 AGENTS.md → Skill 稳定后设 Automation → 定期让 Codex 回顾 session 更新配置 |

## Codex++

Codex++ 是社区开发者基于 OpenAI Codex 构建的增强补丁。它通过扩展 Codex 的 Harness 层，添加了更多实用的功能：

- 增强的上下文管理
- 自定义 [[skills]] 集成
- 更精细的任务控制

Codex++ 的出现证明了 [[harness-engineering]] 的一个核心论点：**在模型之上构建的 Harness 层是可社区化、可迭代的确定性价值**。

## 与 Claude Code 的对比

| 维度 | OpenAI Codex | [[claude-code]] |
|------|-------------|-----------------|
| 配置文件 | AGENTS.md | CLAUDE.md |
| 运行模式 | CLI + Agent | CLI + Agent |
| 底层模型 | OpenAI GPT 系列 | Anthropic Claude |
| 生态增强 | Codex++、[[oh-my-codex]] | Skills、MCP、[[oh-my-claudecode]] |
| Harness 机制 | 项目级配置 | 项目级 + 全局级 |
| 持久上下文 | Durable Threads + Obsidian | Memory 系统 |
| 自动化 | Thread Automations / Goals | 无原生等价物 |
| 侧边栏 | 原生支持多格式审查 | 无原生等价物 |
| 移动端 | 原生 App | 无原生 App |

两者在架构理念上高度一致，都体现了 [[harness-engineering]] 的核心思想。但 Codex 在自动化（Thread Automations、Goals）和跨应用触达（Side Panel、浏览器集成）方面已部分超越 Claude Code。Claude Code 在生态和用户心智上仍有先发优势。

## 在 Harness Engineering 中的地位

Codex 作为 [[harness-engineering]] 三大标志性实践之一，证明了 Harness 理念的普适性。不同的模型提供商（OpenAI、Anthropic、Cursor）都在实践中独立演化出了类似的 Harness 机制，这验证了一个重要洞察：

> 当模型能力达到一定阈值后，决定 Agent 效能的上限不再是模型本身，而是围绕模型构建的 Harness 质量。

## 相关概念

- [[claude-code]] — Anthropic 的 CLI 编程 Agent，与 Codex 形成对标
- [[claude-md]] — Claude Code 的配置文件机制，类似 Codex 的 AGENTS.md
- [[claude-code-session-management]] — Claude Code 会话管理，与 Codex 线程管理对比
- [[jason-liu]] — OpenAI Codex 团队的开发者体验工程师，官方指南作者
- [[skills]] — Claude Code 的可复用能力模块，Codex++ 也提供了类似机制
- [[harness-engineering]] — 脚手架工程方法论，Codex 是其三大标志性实践之一
- [[obsidian]] — Jason Liu 推荐的 Codex 共享记忆存储方案
- [[agent-memory-systems]] — 跨会话记忆的系统性方案
- [[long-running-agent]] — Codex Durable Threads 是长程 Agent 的实践方案
- [[codex-goals]] — Goals 功能完整使用指南和最佳实践
- [[mcp]] — Codex 支持的外部工具连接协议
- [[harness-engineering]] — 配置文件和 Harness 设计的理论基础
