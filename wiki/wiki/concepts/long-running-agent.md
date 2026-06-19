---
title: Long-Running Agent
created: 2026-05-23
updated: 2026-05-25
type: concept
tags: [agent, engineering, methodology]
sources:
  - raw/articles/2026-05-07-effective-harnesses-for-long-running-agents.md
  - raw/articles/2026-05-07-anthropic-harness-guide-dead-weight.md
  - raw/articles/2026-05-24-jason-liu-getting-most-out-of-codex.md
  - raw/articles/2026-05-24-openai-codex-tips-jason-liu.md
---

# Long-Running Agent

## Definition

Long-Running Agent 是指需要跨多个 context window 持续工作数小时甚至数天的 AI Agent。核心挑战在于：每个新 session 开始时，Agent 没有前一个 session 的记忆，必须在有限上下文内理解项目状态并继续推进。

## 核心问题

> Imagine a software project staffed by engineers working in shifts, where each new engineer arrives with no memory of what happened on the previous shift.

Agent 的失败模式主要有两类：

1. **One-shot 倾向** — Agent 试图一次性完成全部任务，中途 context 耗尽，留下半成品代码和未文档化的进度
2. **过早宣布完成** — 后期 Agent 看到已有进展就认为任务结束，跳过未完成的功能

## Initializer Agent + Coding Agent 模式

[[anthropic]] 工程团队提出的解决方案，将 Agent 角色拆分为两种：

### Initializer Agent（初始化 Agent）

第一个 session 专用，负责搭建后续所有 Coding Agent 需要的环境：

- **Feature List**：将用户需求展开为详细的功能清单（如 200+ 条），用 JSON 格式记录，每条标注 `passes: false`
- **`init.sh` 脚本**：启动开发服务器的自动化脚本
- **`claude-progress.txt`**：进度日志文件
- **初始 Git 提交**：记录初始文件结构

### Coding Agent（编码 Agent）

后续每个 session 使用，遵循增量推进原则：

1. `pwd` 确认工作目录
2. 读 git log 和 progress file 了解当前状态
3. 读 feature list，选最高优先级未完成功能
4. 运行 `init.sh` 启动服务，端到端验证基础功能
5. 只做一个功能，完成后 git commit + 更新 progress file

## 增量进度原则

关键发现：**让 Agent 一次只做一个 feature**，比让它自由发挥效果显著更好：

- 每次只修改一个 feature，做完后提交
- 用 Git 作为安全网——坏代码可以 revert
- 进度文件 + Git commit log = 下一个 Agent 的"记忆"
- JSON 格式的 feature list 比 Markdown 更不容易被模型随意修改

## 端到端测试

Claude 倾向于用单元测试或 curl 命令验证功能，但往往不能发现端到端问题。解法：

- 让 Agent 使用浏览器自动化工具（如 Puppeteer MCP）
- 像人类用户一样测试——点击按钮、填写表单、验证页面
- 初始化阶段就建立基础 E2E 测试流程

## 与 Harness Engineering 的关系

Long-Running Agent 模式是 [[harness-engineering]] 在实际工程中的具体应用：

- Initializer Agent 的环境搭建是一种 Harness
- Feature List 防止过早完成是一种约束
- 随着模型变强，某些 Harness 可能成为 [[harness-engineering|Dead Weight]]
- Anthropic 内部 Sprint 分解案例：Opus 4.6 后拆掉 Sprint 层，成本省 37%

## Dead Weight 警示

来自 Lance Martin 的补充观察：

> Sonnet 4.5 会在感觉到 context 上限时提前收工。加了 context 重置机制来应对。到了 Opus 4.5，这个行为消失了。那套重置机制成了 Dead Weight。

这意味着长程 Agent 的 Harness 需要定期审视：**模型自己能做这件事了吗？能了，就拆掉。**

## Codex 的长程 Agent 实践

[[jason-liu]] 在官方指南中揭示了 [[openai-codex]] 的长程 Agent 设计：

### Durable Threads（持久线程）

Codex 的线程是持久化的工作空间，而非一次性对话。通过 `Command-1` 到 `Command-9` 快捷键，用户可以为不同工作流分配固定线程（如 Chief of Staff、Release、Documentation Review），跨会话保留决策、偏好和上下文。

### Thread Automations（线程自动化）

Thread Automations 是一种"心跳"机制——在同一个线程中定时唤醒，带着完整上下文继续工作。典型用法：每 30 分钟检查 Slack/Gmail 未回复消息、排列优先级、起草回复。

核心洞察：**Agent 最有价值的能力，不在于它能替你做什么，而在于它能替你等什么。** 这与 Anthropic 的 Initializer Agent + Coding Agent 模式异曲同工，但 Codex 选择了"让线程持续运行"而非"每个 session 新建 Agent + 传递进度文件"的路径。

### Goals（目标驱动）

`/goal` 为长程 Agent 设定可验证的终点线。强目标配套验证器（测试套件、Benchmark、Bug 复现），Agent 持续推进直到验证通过。这与 Feature List + passes: false 的增量推进模式互补——一个强调"做什么"，一个强调"什么时候算做完"。

## 开放问题

- 单一通用 Coding Agent vs 多 Agent 架构（测试 Agent、QA Agent、代码清理 Agent）哪个更好？
- 这些经验能否泛化到非 Web 开发领域（科研、金融建模等）？
- Compaction + Memory Folder 的组合何时足够，何时需要更复杂的基础设施？

## See Also

- [[harness-engineering]] — 长程 Agent 的方法论基础
- [[agent-loop]] — Agent 的核心运行机制
- [[context-engineering]] — 上下文管理策略
- [[agent-memory-systems]] — 跨 session 记忆方案
- [[claude-code]] — Long-Running Agent 的主要实践平台
- [[openai-codex]] — Codex 的 Durable Threads 和 Thread Automations 是长程 Agent 的另一实践路径
- [[jason-liu]] — OpenAI Codex 官方指南作者
