---
title: Hermes Agent
created: 2026-04-23
updated: 2026-05-30
type: entity
tags: [tool, agent, open-source]
sources:
  - hermes-agent-chinese-community-feishu
  - hermes-agent-lobster-hermes
  - hermes-multi-agent-collaboration-guide
  - turix-cua-agent-skill
  - raw/articles/2026-04-18-hermes-agent-chinese-community-feishu.md
  - raw/articles/2026-04-18-hermes-agent-lobster-hermes.md
  - raw/articles/2026-04-21-hermes-multi-agent-collaboration-guide.md
  - raw/articles/2026-04-21-turix-cua-agent-skill.md
  - raw/articles/2026-04-18-11-hot-github-projects-this-week.md
  - raw/articles/2026-04-21-github-top10-weekly-stars.md
  - raw/articles/2026-05-29-openclawhermesai-agent.md
---

# Hermes Agent

## Overview

Hermes Agent is [[nousresearch]]'s open-source AI agent platform. It features process-isolated profiles with independent configuration, memory, skills, and gateway for each agent instance. Hermes supports Discord integration and multi-agent collaboration capabilities, positioning itself as an open-source alternative to proprietary agent platforms like [[claude-code]]. Hermes appears in 4 articles in the corpus.

## Core Architecture

### Agent Profiles
Hermes Agent's defining feature is its process-isolated profile system. Each agent profile is a self-contained unit with:

- **Independent configuration**: Each profile has its own config file, separate from other agents
- **Independent memory**: Per-profile memory systems that don't leak between agents
- **Independent skills**: Different profiles can load different skill sets
- **Independent gateway**: Each profile has its own communication gateway for message routing

This isolation ensures that agents can operate autonomously without interference, while still participating in multi-agent collaboration when needed.

### SOUL.md Personality System
Like [[openclaw]], Hermes uses SOUL.md files to define agent personalities:

- Personality traits and communication style
- Role description and expertise
- Behavioral preferences
- Language and cultural context

The SOUL.md convention allows users to create highly customized agent personas without modifying core code.

### Discord Integration
Hermes uses Discord as its primary collaboration platform:

- Agents operate in dedicated Discord channels
- Users interact through direct messages or channel conversations
- Supports multi-agent coordination with shared communication channels
- Enables real-time collaboration between human and AI agents

### Multi-Agent Collaboration
Hermes supports sophisticated multi-agent workflows:

- Multiple agent profiles can collaborate on complex tasks
- Role-based agent specialization (researcher, writer, reviewer, etc.)
- Shared memory and context between collaborating agents
- Message routing and coordination protocols

## Key Features

### Process Isolation
Each Hermes agent runs as an isolated process, providing:

- **Security**: Compromise of one agent doesn't affect others
- **Stability**: Crashes in one agent don't cascade
- **Resource management**: Independent resource allocation per agent
- **Customization**: Complete independence in configuration and behavior

### Chinese Community Support
Hermes has an active Chinese community:

- **Feishu (Lark) integration**: Chinese community operates on ByteDance's Feishu platform
- **Community documentation**: Chinese-language guides and tutorials available
- **Local practitioner**: 林月半子 is a noted Hermes multi-agent collaboration practitioner

### Skill System with Self-Evolution
Hermes 的 Skill 系统是其最突出的差异化特性——**Skills 带有自我进化机制**，这让 Hermes 被定位为"为 Skill 自我进化而生的 Agent"。

#### Skill 作为程序性记忆
Hermes 把 skill 定义为"程序性记忆"（procedural memory），而非单纯的命令扩展：
- 复杂任务经验的沉淀
- 可复用工作流
- 未来任务的执行模板

#### 完整的 Skill 管理工具链
- `skills_list` — 列出已有技能
- `skill_view` — 查看技能详情
- `skill_manage` — 支持 create / patch / edit / delete 操作

这意味着 Agent 不只是"建议记住"，而是真正能把经验写回系统。

#### Prompt 层强制推动 Skill 演化
Hermes 的系统提示明确要求 Agent：
1. 先扫描可用 skills
2. 命中就加载 skill
3. 任务复杂后主动保存 skill
4. Skill 发现问题时立刻 patch

与其他 Agent 的核心区别：别的 Agent 的 skill 是"可以用"，Hermes 的 skill 是"应该维护、应该演化、应该沉淀"。

### Memory × Session Recall × Skills 三层联动
Hermes 的强项不在于单个系统，而在于三者联动：

| 层级 | 职责 | 示例 |
|------|------|------|
| **memory** | 记稳定事实与偏好 | "用户喜欢什么" |
| **session_search** | 召回过去会话经验 | "之前怎么处理过类似问题" |
| **skills** | 沉淀成可执行方法论 | "以后遇到这种任务怎么标准化执行" |

链条：**记住事实 → 回忆经验 → 固化流程**

### 记忆框架生态
Hermes Agent 内置支持多种记忆框架可选：Honcho、OpenViking、Mem0、Hindsight、Holographic、RetainDB、ByteRover 等。

### Gateway 聊天软件集成
支持飞书、企业微信、钉钉等国内聊天软件的 channel 集成。安装后通过 `hermes gateway setup` 添加新 channel。

## Use Cases

### Multi-Agent Research Workflows
Hermes is used for collaborative research tasks:

- Literature survey and analysis
- Data processing and analysis
- Paper writing and review
- Code development and testing

### Discord-Based Collaboration
The Discord integration makes Hermes well-suited for:

- Team collaboration with AI agents
- Community-oriented projects
- Real-time interactive workflows
- Educational and tutorial applications

### Computer Use Agent Integration
Hermes can integrate with Turix CUA for desktop automation:

- Operating desktop applications via screen recognition
- Mouse and keyboard simulation for GUI interaction
- Extending agent capabilities beyond text-based interfaces

## Comparison with Related Platforms

### Hermes vs. OpenClaw
Both Hermes and [[openclaw]] are open-source multi-agent platforms with significant overlap:

| Feature | Hermes Agent | OpenClaw |
|---------|-------------|----------|
| Creator | NousResearch | Community |
| Isolation | Process-isolated profiles | Session-based |
| Memory | Per-profile memory | 5-layer architecture |
| Governance | Profile config | Dual-track governance |
| Platforms | Discord, Feishu | Discord, WeChat |
| Skill system | SKILL.md | CLAWHUB + SKILL.md |
| Personality | SOUL.md | SOUL.md |
| Marketplace | No | CLAWHUB |

### Hermes vs. Claude Code
| Feature | Hermes Agent | Claude Code |
|---------|-------------|-------------|
| Open source | Yes | No |
| Multi-agent | Native support | Subagent pattern only |
| Platform | Discord/Feishu | CLI |
| Model | Configurable | Claude only |
| Context | Per-profile | Per-session |

## Key Relationships

- Created by [[nousresearch]]
- Related to [[openclaw]] — both are open-source multi-agent platforms
- Part of the [[multi-agent-collaboration]] paradigm
- Integrates with [[discord]] for communication
- Compatible with [[skills]] ecosystem (SKILL.md)
- Can connect to [[turix-cua]] for desktop automation
- Alternative to [[claude-code]] for open-source agent needs

## 2026-04 更新：架构深化

### Agent Loop 实现
Hermes Agent 基于 [[agent-loop]] 模式构建，支持 [[react-pattern]] 风格的推理-行动循环。与 [[openclaw]] 的 Pi Agent 类似，Hermes 的工具集精简为核心操作：

- **shell_exec** — 执行 Shell 命令
- **file_read / file_write** — 文件读写
- **python_exec** — Python 代码执行
- **[[mcp]] 工具** — 外部服务集成

### 多 Agent 协作模式
2026-04 的多 Agent 协作指南进一步明确了 Hermes 的协作模式：

1. **角色分工**：不同 Agent 承担不同角色（研究员、写手、审查者）
2. **消息路由**：基于频道和账户的确定性消息路由
3. **共享记忆**：协作 Agent 之间可访问共享记忆
4. **独立执行**：每个 Agent 有独立的 [[agent-loop]] 实例

### 中文社区活跃
飞书社区持续活跃，林月半子分享了多个 Hermes 多 Agent 协作实践案例。

## Chinese Community

The Hermes Agent Chinese community is active on Feishu (飞书), ByteDance's enterprise collaboration suite. Key community members include 林月半子, who practices multi-agent collaboration workflows and shares tutorials.

## Sources

- hermes-agent-chinese-community-feishu — Chinese community overview and Feishu integration
- hermes-agent-lobster-hermes — Agent architecture and capabilities
- hermes-multi-agent-collaboration-guide — Multi-agent collaboration patterns and workflows
- turix-cua-agent-skill — Computer Use Agent integration capabilities
See also: [[agent-memory-systems]] for cross-session memory design.
See also: [[agent-loop]] for the core runtime pattern.
See also: [[skill-engineering]] for Skill 设计的工程化方法论。
See also: [[agent-building-tutorial]] for Agent 构建实战方法论。

## 2026-04 更新：Hermes 多 Agent 实战（林月半子）

### Profile 系统详解

Hermes 的 Profile 通过 `HERMES_HOME` 环境变量切换根目录实现**进程级隔离**——每个 profile 有独立的 config.yaml、.env、SOUL.md、memory、skills、甚至独立的 gateway 进程。

#### 三档克隆策略
- **空白创建**（`hermes profile create mybot`）— 连 API key 都要重新配
- **--clone**（推荐用于多 Agent）— 只复制 config.yaml、.env、SOUL.md，记忆和 session 全新，共享模型和 API key
- **--clone-all** — 连 memory、sessions、skills、cron jobs 全拷贝，适合备份

#### Wrapper 命令
创建后每个 profile 自动生成独立命令（如 `ink chat`、`ink gateway start`），不用每次写 `hermes -p ink xxx`。

### Discord 多 Agent 实践踩坑

#### 三人小组架构
- **林小墨 (Ink)** — 文案与笔记整理专家
- **林小探 (Search)** — 搜索与调研专家
- **林小管 (Admin)** — 任务分发与调度员

选择 Discord 而非飞书的原因：飞书不支持 bot 被 @，而多 Agent 协作核心动作是"一个 agent @ 另一个 agent 来接力"。

#### 三大踩坑与解决

**坑 1：没有 @，直接结束**
LLM 理解"林小探是团队里的人"，但不知道 Discord 里要用 `<@用户ID>` 格式触发。解决方案：在 SOUL.md 的花名册中把每个人的 Discord ID 挂在名字后面，严格区分计划阶段（纯文字）和执行阶段（用 `<@ID>`）。

**坑 2：停不下来的死循环**
三层兜底方案：
1. `DISCORD_ALLOW_BOTS=mentions` — 只响应 @，不响应其他 bot 消息
2. `replied_user: false` — 关闭 Discord reply 的自动 mention
3. SOUL.md 终止协议 — 明确任务结束标记"【任务结束】"，禁止冗余表情和寒暄

**坑 3：同时 @ 多人导致混乱**
在 SOUL.md 中强制时序规范：逐一唤醒，先 @ 林小探查资料，等其明确回复"调研完成"后再 @ 林小墨整理笔记。

### 关键洞察

> **多 Agent 是管理问题，不是技术问题。** Profile 给你工位，Discord 给你会议室，真正让 AI 像团队一样跑起来的是那份反复打磨的 SOUL.md——那是职责说明书、协作流程、以及明确的下班时间。

> 坑 1 = 下属不知道该找谁汇报；坑 2 = 没有明确的项目终结机制；坑 3 = 任务分派时序混乱。拿去套人类公司一样成立。

### delegate_task vs 真多 Agent

Hermes 内置的 `delegate_task` 模式会 spawn 临时 subagent（用完即焚，不是精心配置的独立 profile），且 subagent 的 `send_message` 被 Blocked。想让真多 Agent 跑起来，必须在 Admin 的 SOUL.md 里写死协作协议。

## 2026-05 源码架构深度剖析（腾讯技术工程）

基于腾讯技术工程对 Hermes Agent 源码的深入分析，以下是其与 [[openclaw]] 架构的工程对比核心洞察：

### 设计路线差异

| 维度 | Hermes Agent | OpenClaw |
|------|-------------|----------|
| 设计路线 | 工具密度 + 自我改进 | 平台化 + 微内核 |
| 核心架构 | 单体 AIAgent 类 | Gateway 微内核 + 插件系统 |
| Channel 抽象 | 轻量消息收发管道 | 完整 IM 域协作单元（25+ Adapter） |
| 凭证管理 | Credential Pool（API Key 数组轮换） | Auth Profile（带健康状态的对象） |
| 记忆管理 | 半自动（Memory Nudge + Session Search） | 全自动（Dreaming 三阶段加权晋升） |
| 循环耦合 | 循环和编排耦合在 AIAgent 万行类中 | pi-agent-core 独立包，循环/编排分离 |

### AIAgent 核心 — 单体执行引擎

`AIAgent` 类（`run_agent.py`）是将执行引擎、API 调度、模型降级、记忆管理、工具编排等职责集中在一个类中的大型单体类。`run_conversation()` 执行循环分为五阶段：
1. 预处理（恢复运行时/加载上下文/设置预算）
2. 系统提示缓存（首轮构建后冻结，保护 Anthropic prompt cache）
3. 预压缩（ContextCompressor 超过 50% 窗口触发）
4. 主循环（max_iterations=90）
5. 后处理（memory_manager.prefetch_all()）

### 四种 API 模式

支持 `chat_completions`（OpenAI 兼容，200+ 模型）、`codex_responses`（OpenAI Codex/xAI/GPT-5.x）、`anthropic_messages`（原生 Anthropic API，支持 prompt caching）、`bedrock_converse`（AWS Bedrock）。自动检测优先级：显式参数 > provider 名 > base_url 模式 > 默认。

### Credential Pool vs Auth Profile

Hermes 的 Credential Pool 只是 API Key 数组，按顺序尝试，不区分错误类型，不记录「上次哪个 key 挂了」。对比 OpenClaw 的 Auth Profile：将每个账号建模为带健康状态的对象（类型/状态/冷却原因），错误时智能识别 → 标记冷却 → 毫秒级切换 → 自动探针恢复。

### Tool Registry — 导入时自注册

`ToolRegistry` 是模块级全局单例，工具在导入时自注册（非装饰器模式）。`discover_builtin_tools()` 通过 AST 静态分析自动发现含 `registry.register()` 调用的模块。`tools/` 目录含 76 个文件，6 组分类（核心/Agent 协作/技能&记忆/媒体&通信/安全&运维/执行环境），支持 8 种沙箱后端。

### Session Search — SQLite FTS5 全文搜索

Hermes 独有的「翻日记本式回忆」：双 FTS5 索引（标准分词 + trigram 覆盖中英文），Agent 搜索过去所有对话的完整原始历史（非摘要），取 top 3 会话 → 截断（25% 前文 + 75% 后文）→ 辅助 LLM 摘要（max 10K tokens）→ 返回主 Agent。关键设计：摘要而非原文、沿 parent_session_id 溯源、排除当前 session、WAL 模式支持并发。

### Skill 渐进式披露 — 三级访问

`skills_list`（仅元数据，低 Token）→ `skill_view`（完整 SKILL.md）→ `skill_view + 子路径`（按需加载支撑文件）。将技能加载成本从 O(N) 降到 O(被实际用到的)。Name ≤ 64 chars、Description ≤ 1024 chars 是代码强制约束，非建议。

### Context Compressor — 四步算法

当会话历史超过 50% 上下文窗口时触发：智能摘要（16+ 种工具类型专用摘要格式）→ 对话压缩 → 反抖动保护（连续 2 次 < 10% 效果自动跳过）→ 冻结快照（保护 Anthropic prompt cache 前缀在会话期间持续命中）。

### 新来源

- `raw/articles/2026-05-29-openclawhermesai-agent.md` — 腾讯技术工程源码级架构剖析
